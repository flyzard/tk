//! The brief: what a fresh session needs to pick a ticket up, derived from the
//! ticket and its typed log. Nothing here is stored.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use tk_core::{
    Actor, Entry, Kind, NonEmpty, Priority, StateName, Status, TicketId, TicketView, Timestamp,
    Title,
};
use tk_store::{Loaded, LogEntry, LogLine, Version, log_lines, without_log};

use crate::ctx::Ctx;
use crate::error::AppError;
use crate::query::{Scope, snapshot, stores, waiting};
use crate::refs::{self, Found, Linked, LiveRef, live_refs, ticket_targets};

/// How many of the latest notes a brief shows.
const NOTES_SHOWN: usize = 5;
/// How many ruled-out and decided entries a brief shows, each.
const FACTS_SHOWN: usize = 10;
/// How many refs a brief shows. A ticket with more is probably several tickets.
const REFS_SHOWN: usize = 10;
/// How many decided entries a brief shows from each done dependency.
const UPSTREAM_SHOWN: usize = 3;

#[derive(Serialize, Debug)]
pub struct Brief {
    pub id: TicketId,
    pub title: Title,
    pub kind: Kind,
    pub priority: Priority,
    pub status: Status,
    pub version: Version,
    /// Dependencies that aren't done.
    pub waiting_on: Vec<Waiting>,
    /// The body without its log.
    pub prose: String,
    /// The latest handoff, if there was one.
    pub handoff: Option<LastHandoff>,
    /// Where to look, in the order the targets were first added, each located now.
    pub refs: Capped<RefItem>,
    /// Newest first.
    pub ruled_out: Capped<Item>,
    /// Newest first.
    pub decided: Capped<Item>,
    /// What done dependencies decided: the ground this ticket builds on.
    pub upstream: Vec<Upstream>,
    /// The latest notes, oldest first. Lines tk can't parse count as notes.
    pub notes: Capped<Item>,
}

#[derive(Serialize, Debug)]
pub struct Waiting {
    pub id: TicketId,
    /// `None` if its file can't be read.
    pub state: Option<StateName>,
}

/// The newest group of `next` entries: one handoff writes them all with one timestamp.
#[derive(Serialize, Debug)]
pub struct LastHandoff {
    pub at: Timestamp,
    pub by: Actor,
    pub age_minutes: i64,
    pub next: Vec<NonEmpty>,
    /// Entries logged after the handoff. More than none means its plan may be stale.
    pub entries_since: usize,
}

/// A live ref, and where its target is now.
#[derive(Serialize, Debug)]
pub struct RefItem {
    #[serde(flatten)]
    pub live: LiveRef,
    pub found: Found,
}

/// A done dependency's decisions, newest first.
#[derive(Serialize, Debug)]
pub struct Upstream {
    pub id: TicketId,
    pub title: Title,
    pub decided: Capped<Item>,
}

#[derive(Serialize, Debug)]
pub struct Capped<T> {
    pub items: Vec<T>,
    /// How many more there are; `tk show` has them all.
    pub older: usize,
}

/// One log line. A line tk can't parse has no time or actor.
#[derive(Serialize, Debug)]
pub struct Item {
    pub at: Option<Timestamp>,
    pub by: Option<Actor>,
    pub text: String,
}

/// The brief of one ticket.
///
/// # Errors
/// [`AppError::NotFound`], an invalid file or a store error.
pub fn brief(ctx: &Ctx, id: &TicketId) -> Result<Brief, AppError> {
    ctx.store(id.project())?
        .transaction(|tx| -> Result<Brief, AppError> {
            let loaded = tx.load(id)?;
            // This project's lock is held: its tickets are read through `tx`.
            brief_of(ctx, &loaded, |id| tx.load(id).ok().map(|l| Linked::of(&l)))
        })
}

/// Briefs of the tickets this checkout has in `doing` by the current actor.
/// Each worktree's files show only its own starts, so this is per worktree.
///
/// # Errors
/// [`AppError::NoProject`] outside a project, or a store error.
pub fn my_briefs(ctx: &Ctx) -> Result<Vec<Brief>, AppError> {
    let mut briefs = Vec::new();
    for store in stores(ctx, Scope::Current)? {
        let snapshot = snapshot(store)?;
        for loaded in snapshot
            .tickets
            .values()
            .filter(|l| matches!(l.ticket().status(), Status::Doing { by, .. } if *by == ctx.actor))
        {
            briefs.push(brief_of(ctx, loaded, |id| {
                snapshot.tickets.get(id).map(Linked::of)
            })?);
        }
    }
    Ok(briefs)
}

/// One brief. `local` reads tickets of the brief's own project.
fn brief_of(
    ctx: &Ctx,
    loaded: &Loaded,
    local: impl Fn(&TicketId) -> Option<Linked>,
) -> Result<Brief, AppError> {
    // No `..`: a new Ticket field breaks this line until the brief decides about it.
    let TicketView {
        id,
        title,
        kind,
        priority,
        labels: _,
        depends_on,
        created: _,
        status,
        body,
    } = loaded.ticket().view();
    let lines = log_lines(body.as_ref());
    let handoff = handoff(&lines, ctx.now);
    let live = live_refs(&lines);
    let linked = refs::linked(
        ctx,
        id.project(),
        depends_on.iter().chain(ticket_targets(&live)),
        local,
    );
    let repo = ctx.store(id.project())?.repo();
    let refs = live
        .into_iter()
        .map(|live| RefItem {
            found: refs::locate(repo, &live.reference.target, &linked),
            live,
        })
        .collect();
    let Facts {
        ruled_out,
        decided,
        notes,
    } = facts(lines);
    Ok(Brief {
        id: id.clone(),
        title: title.clone(),
        kind,
        priority,
        status: status.clone(),
        version: loaded.version(),
        waiting_on: waiting(depends_on, |d| linked.get(d).map(|l| l.state))
            .map(|(id, state)| Waiting {
                id: id.clone(),
                state,
            })
            .collect(),
        prose: without_log(body.as_ref()).trim().to_owned(),
        handoff,
        refs: latest(refs, REFS_SHOWN),
        ruled_out: newest(ruled_out, FACTS_SHOWN),
        decided: newest(decided, FACTS_SHOWN),
        upstream: upstream(depends_on, linked),
        notes: latest(notes, NOTES_SHOWN),
    })
}

/// A log's facts and notes, oldest first. Lines tk can't parse count as notes.
pub(crate) struct Facts {
    pub ruled_out: Vec<Item>,
    pub decided: Vec<Item>,
    pub notes: Vec<Item>,
}

pub(crate) fn facts(lines: Vec<LogLine<'_>>) -> Facts {
    let (mut ruled_out, mut decided, mut notes) = (Vec::new(), Vec::new(), Vec::new());
    for line in lines {
        match line {
            LogLine::Entry(LogEntry { at, by, entry }) => {
                let (bucket, text) = match entry {
                    Entry::Note(text) => (&mut notes, text),
                    Entry::RuledOut(text) => (&mut ruled_out, text),
                    Entry::Decided(text) => (&mut decided, text),
                    // The brief takes next steps from the handoff, and refs from `live_refs`.
                    Entry::Next(_) | Entry::Ref(_) | Entry::Unref(_) => continue,
                };
                bucket.push(Item {
                    at: Some(at),
                    by: Some(by),
                    text: text.into_inner(),
                });
            }
            LogLine::Raw(raw) => notes.push(Item {
                at: None,
                by: None,
                text: raw.to_owned(),
            }),
        }
    }
    Facts {
        ruled_out,
        decided,
        notes,
    }
}

/// What each done dependency decided, in dependency order.
fn upstream(
    depends_on: &BTreeSet<TicketId>,
    mut linked: BTreeMap<TicketId, Linked>,
) -> Vec<Upstream> {
    depends_on
        .iter()
        .filter_map(|id| {
            let Linked {
                title,
                state: _,
                decided,
            } = linked.remove(id)?;
            (!decided.is_empty()).then(|| Upstream {
                id: id.clone(),
                title,
                decided: newest(decided, UPSTREAM_SHOWN),
            })
        })
        .collect()
}

fn handoff(lines: &[LogLine<'_>], now: Timestamp) -> Option<LastHandoff> {
    let entries: Vec<&LogEntry> = lines
        .iter()
        .filter_map(|l| match l {
            LogLine::Entry(e) => Some(e),
            LogLine::Raw(_) => None,
        })
        .collect();
    let at = entries
        .iter()
        .filter(|e| matches!(e.entry, Entry::Next(_)))
        .map(|e| e.at)
        .max()?;
    let group: Vec<(&LogEntry, &NonEmpty)> = entries
        .iter()
        .copied()
        .filter(|e| e.at == at)
        .filter_map(|e| match &e.entry {
            Entry::Next(text) => Some((e, text)),
            Entry::Note(_)
            | Entry::RuledOut(_)
            | Entry::Decided(_)
            | Entry::Ref(_)
            | Entry::Unref(_) => None,
        })
        .collect();
    Some(LastHandoff {
        at,
        by: group.first()?.0.by.clone(),
        age_minutes: now.duration_since(at).as_mins(),
        next: group.iter().map(|(_, text)| (*text).clone()).collect(),
        entries_since: entries.iter().filter(|e| e.at > at).count(),
    })
}

/// The last `n` items, newest first.
fn newest<T>(items: Vec<T>, n: usize) -> Capped<T> {
    let mut capped = latest(items, n);
    capped.items.reverse();
    capped
}

/// The last `n` items, oldest first.
fn latest<T>(mut items: Vec<T>, n: usize) -> Capped<T> {
    let older = items.len().saturating_sub(n);
    Capped {
        items: items.split_off(older),
        older,
    }
}
