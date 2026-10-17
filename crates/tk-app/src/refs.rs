//! Refs: the map of where to look that sessions build up on a ticket. The live
//! set is folded from the log and every target is located again on each read,
//! so line numbers are never stale. Nothing here is stored.

use std::collections::BTreeMap;

use camino::Utf8Path;
use serde::Serialize;
use tk_core::{
    Actor, Anchor, Entry, ProjectKey, RefTarget, Reference, RepoPath, StateName, TicketId,
    Timestamp, Title,
};
use tk_store::{Loaded, LogLine, StoreError, log_lines, markdown_headings};

use crate::brief::{Item, facts};
use crate::ctx::Ctx;
use crate::error::AppError;

/// A ref that no unref has dropped, with who added it last.
#[derive(Serialize, Debug)]
pub struct LiveRef {
    #[serde(flatten)]
    pub reference: Reference,
    pub at: Timestamp,
    pub by: Actor,
}

/// The refs still standing, in the order their targets were first added. A
/// later ref to the same target replaces its why; an unref removes it.
pub(crate) fn live_refs(lines: &[LogLine<'_>]) -> Vec<LiveRef> {
    let mut live: Vec<LiveRef> = Vec::new();
    for line in lines {
        let LogLine::Entry(e) = line else { continue };
        match &e.entry {
            Entry::Ref(r) => {
                let new = LiveRef {
                    reference: r.clone(),
                    at: e.at,
                    by: e.by.clone(),
                };
                match live.iter_mut().find(|l| l.reference.target == r.target) {
                    Some(old) => *old = new,
                    None => live.push(new),
                }
            }
            Entry::Unref(r) => live.retain(|l| l.reference.target != r.target),
            Entry::Note(_) | Entry::RuledOut(_) | Entry::Decided(_) | Entry::Next(_) => {}
        }
    }
    live
}

/// The tickets that live refs point at.
pub(crate) fn ticket_targets(live: &[LiveRef]) -> impl Iterator<Item = &TicketId> {
    live.iter().filter_map(|r| match &r.reference.target {
        RefTarget::Ticket(id) => Some(id),
        RefTarget::Url(_) | RefTarget::File { .. } => None,
    })
}

/// What a session needs from another ticket: what it is and, once it's done,
/// what it decided.
#[derive(Debug)]
pub(crate) struct Linked {
    pub title: Title,
    pub state: StateName,
    /// Oldest first; empty unless the ticket is done.
    pub decided: Vec<Item>,
}

impl Linked {
    pub fn of(loaded: &Loaded) -> Self {
        let t = loaded.ticket();
        let state = t.status().name();
        let decided = if state == StateName::Done {
            facts(log_lines(t.body.as_ref())).decided
        } else {
            Vec::new()
        };
        Self {
            title: t.title.clone(),
            state,
            decided,
        }
    }
}

/// The tickets `ids` that can be read. Those in `own` project come from
/// `local`, since the caller may hold that project's lock; others are read
/// under their own.
pub(crate) fn linked<'a>(
    ctx: &Ctx,
    own: &ProjectKey,
    ids: impl Iterator<Item = &'a TicketId>,
    local: impl Fn(&TicketId) -> Option<Linked>,
) -> BTreeMap<TicketId, Linked> {
    ids.filter_map(|id| {
        let found = if id.project() == own {
            local(id)
        } else {
            ctx.store(id.project())
                .ok()?
                .transaction(|tx| -> Result<Linked, AppError> { Ok(Linked::of(&tx.load(id)?)) })
                .ok()
        };
        found.map(|l| (id.clone(), l))
    })
    .collect()
}

/// Where a ref's target is now.
#[derive(Serialize, Debug)]
#[serde(tag = "found", rename_all = "snake_case")]
pub enum Found {
    /// A whole file, and how many lines it has.
    File {
        lines: usize,
    },
    /// The Markdown section under the anchor's heading: read lines `start` to `end`.
    Section {
        start: usize,
        end: usize,
    },
    /// The first line containing the anchor.
    Line {
        line: usize,
    },
    Dir,
    Ticket {
        title: Title,
        state: StateName,
    },
    /// Not checked: tk never fetches links.
    Link,
    /// The target is gone, usually renamed: point the ref somewhere else or drop it.
    Stale {
        problem: Stale,
    },
}

impl Found {
    /// Why the target can't be found, if it can't.
    #[must_use]
    pub fn stale(&self) -> Option<Stale> {
        match self {
            Self::Stale { problem } => Some(*problem),
            Self::File { .. }
            | Self::Section { .. }
            | Self::Line { .. }
            | Self::Dir
            | Self::Ticket { .. }
            | Self::Link => None,
        }
    }
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Stale {
    #[error("no such file or directory in the repository")]
    NoFile,
    #[error("no line in the file contains the anchor")]
    NoAnchor,
    #[error("no such ticket")]
    NoTicket,
}

/// Locates a target. `repo` is the root that paths are relative to: the
/// repository of the ticket that holds the ref.
pub(crate) fn locate(
    repo: &Utf8Path,
    target: &RefTarget,
    linked: &BTreeMap<TicketId, Linked>,
) -> Found {
    match target {
        RefTarget::Ticket(id) => match linked.get(id) {
            Some(Linked {
                title,
                state,
                decided: _,
            }) => Found::Ticket {
                title: title.clone(),
                state: *state,
            },
            None => Found::Stale {
                problem: Stale::NoTicket,
            },
        },
        RefTarget::Url(_) => Found::Link,
        RefTarget::File { path, anchor } => locate_file(repo, path, anchor.as_ref()),
    }
}

fn locate_file(repo: &Utf8Path, path: &RepoPath, anchor: Option<&Anchor>) -> Found {
    let full = repo.join(path.as_ref());
    let stale = |problem| Found::Stale { problem };
    if full.is_dir() {
        return match anchor {
            None => Found::Dir,
            Some(_) => stale(Stale::NoAnchor),
        };
    }
    let Ok(bytes) = std::fs::read(&full) else {
        return stale(Stale::NoFile);
    };
    let text = String::from_utf8_lossy(&bytes);
    match anchor {
        None => Found::File {
            lines: text.lines().count(),
        },
        Some(anchor) => {
            let markdown = full
                .extension()
                .is_some_and(|ext| ext == "md" || ext == "markdown");
            locate_anchor(&text, anchor.as_ref(), markdown).unwrap_or(stale(Stale::NoAnchor))
        }
    }
}

/// The first line containing `anchor`. In Markdown, a heading containing it
/// wins over prose that mentions it, and gives its whole section, up to the
/// next heading of the same or a higher level, without trailing blank lines.
fn locate_anchor(text: &str, anchor: &str, markdown: bool) -> Option<Found> {
    let headings: BTreeMap<usize, usize> = if markdown {
        markdown_headings(text).into_iter().collect()
    } else {
        BTreeMap::new()
    };
    let lines: Vec<&str> = text.lines().collect();
    let hit = (0..lines.len())
        .find(|i| headings.contains_key(i) && lines.get(*i).is_some_and(|l| l.contains(anchor)))
        .or_else(|| lines.iter().position(|l| l.contains(anchor)))?;
    let start = hit + 1;
    let Some(&level) = headings.get(&hit) else {
        return Some(Found::Line { line: start });
    };
    let next = headings
        .range(start..)
        .find(|&(_, &l)| l <= level)
        .map_or(lines.len(), |(&i, _)| i);
    let end = lines
        .get(..next)
        .and_then(|before| before.iter().rposition(|l| !l.trim().is_empty()))
        .map_or(start, |i| (i + 1).max(start));
    Some(Found::Section { start, end })
}

/// Why a ref can't be written.
#[derive(Debug, thiserror::Error)]
pub enum RefError {
    #[error("ref {target}: {problem}")]
    Stale { target: RefTarget, problem: Stale },
    #[error("{0} has no ref to {1}")]
    NotReferenced(TicketId, RefTarget),
}

/// Refuses a ref to a target that isn't there, so a typo is caught when it's
/// written rather than by the next session. Paths are in `project`'s repository.
/// Takes no lock that a caller could be holding.
pub(crate) fn check(ctx: &Ctx, project: &ProjectKey, target: &RefTarget) -> Result<(), AppError> {
    let problem = match target {
        RefTarget::Ticket(id) => {
            match ctx
                .store(id.project())?
                .transaction(|tx| tx.load(id).map(|_| ()))
            {
                Ok(()) => None,
                Err(StoreError::NotFound(_)) => Some(Stale::NoTicket),
                Err(e) => return Err(e.into()),
            }
        }
        RefTarget::Url(_) => None,
        RefTarget::File { path, anchor } => {
            locate_file(ctx.store(project)?.repo(), path, anchor.as_ref()).stale()
        }
    };
    match problem {
        Some(problem) => Err(RefError::Stale {
            target: target.clone(),
            problem,
        }
        .into()),
        None => Ok(()),
    }
}

/// An unref must name a ref the ticket has, or a typo would drop nothing and say nothing.
pub(crate) fn check_unref(id: &TicketId, body: &str, target: &RefTarget) -> Result<(), AppError> {
    if live_refs(&log_lines(body))
        .iter()
        .any(|l| &l.reference.target == target)
    {
        Ok(())
    } else {
        Err(RefError::NotReferenced(id.clone(), target.clone()).into())
    }
}

#[cfg(test)]
mod tests {
    use super::{Found, locate_anchor};

    const DOC: &str = "# Plan\n\n## Handoffs\ntext\n\n```sh\n# not a heading\n```\n### Detail\nmore\n\n## Next part\nx\n";

    fn span(found: Option<Found>) -> Option<(usize, usize)> {
        match found? {
            Found::Section { start, end } => Some((start, end)),
            Found::Line { line } => Some((line, line)),
            Found::File { .. }
            | Found::Dir
            | Found::Ticket { .. }
            | Found::Link
            | Found::Stale { .. } => None,
        }
    }

    /// Markdown is text: a heading's section runs to the next heading at its level,
    /// skips fenced code and trailing blanks, and beats prose that mentions it.
    #[test]
    fn anchors_find_sections_and_lines() {
        assert_eq!(span(locate_anchor(DOC, "Handoffs", true)), Some((3, 10)));
        assert_eq!(span(locate_anchor(DOC, "Detail", true)), Some((9, 10)));
        assert_eq!(
            span(locate_anchor(DOC, "not a heading", true)),
            Some((7, 7))
        );
        assert_eq!(span(locate_anchor(DOC, "Next part", true)), Some((12, 13)));
        assert_eq!(
            span(locate_anchor("a\nb Plan\n# Plan\n", "Plan", true)),
            Some((3, 3))
        );
        assert_eq!(
            span(locate_anchor("Plan\n====\ntext\n", "Plan", true)),
            Some((1, 3))
        );
        assert_eq!(span(locate_anchor(DOC, "Handoffs", false)), Some((3, 3)));
        assert_eq!(span(locate_anchor(DOC, "missing", true)), None);
    }
}
