//! The brief: what a fresh session needs to pick a ticket up, derived from the
//! ticket and its typed log. Nothing here is stored.

use std::collections::BTreeMap;

use serde::Serialize;
use tk_core::{
    Actor, EntryKind, Kind, NonEmpty, Priority, StateName, Status, TicketId, TicketView, Timestamp,
    Title,
};
use tk_store::{Loaded, LogEntry, LogLine, Version, log_lines, without_log};

use crate::ctx::Ctx;
use crate::error::AppError;
use crate::query::{Scope, dep_states, snapshot, states, stores, waiting};

/// How many of the latest notes a brief shows.
const NOTES_SHOWN: usize = 5;
/// How many ruled-out and decided entries a brief shows, each.
const FACTS_SHOWN: usize = 10;

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
    /// Newest first.
    pub ruled_out: Capped<Item>,
    /// Newest first.
    pub decided: Capped<Item>,
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
            Ok(build(&loaded, &dep_states(tx, &loaded), ctx.now))
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
        let states = states(&snapshot);
        briefs.extend(
            snapshot
                .tickets
                .values()
                .filter(
                    |l| matches!(l.ticket().status(), Status::Doing { by, .. } if *by == ctx.actor),
                )
                .map(|l| build(l, &states, ctx.now)),
        );
    }
    Ok(briefs)
}

fn build(loaded: &Loaded, states: &BTreeMap<TicketId, StateName>, now: Timestamp) -> Brief {
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
    let handoff = handoff(&lines, now);
    let (mut ruled_out, mut decided, mut notes) = (Vec::new(), Vec::new(), Vec::new());
    for line in lines {
        match line {
            LogLine::Entry(e) => {
                let bucket = match e.kind {
                    EntryKind::Note => &mut notes,
                    EntryKind::RuledOut => &mut ruled_out,
                    EntryKind::Decided => &mut decided,
                    // The handoff above has the ones that matter.
                    EntryKind::Next => continue,
                };
                bucket.push(Item {
                    at: Some(e.at),
                    by: Some(e.by),
                    text: e.text.into_inner(),
                });
            }
            LogLine::Raw(raw) => notes.push(Item {
                at: None,
                by: None,
                text: raw.to_owned(),
            }),
        }
    }
    Brief {
        id: id.clone(),
        title: title.clone(),
        kind,
        priority,
        status: status.clone(),
        version: loaded.version(),
        waiting_on: waiting(depends_on, states)
            .map(|(id, state)| Waiting {
                id: id.clone(),
                state,
            })
            .collect(),
        prose: without_log(body.as_ref()).trim().to_owned(),
        handoff,
        ruled_out: newest(ruled_out, FACTS_SHOWN),
        decided: newest(decided, FACTS_SHOWN),
        notes: latest(notes, NOTES_SHOWN),
    }
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
        .filter(|e| e.kind == EntryKind::Next)
        .map(|e| e.at)
        .max()?;
    let group: Vec<&LogEntry> = entries
        .iter()
        .copied()
        .filter(|e| e.kind == EntryKind::Next && e.at == at)
        .collect();
    Some(LastHandoff {
        at,
        by: group.first()?.by.clone(),
        age_minutes: now.duration_since(at).as_mins(),
        next: group.iter().map(|e| e.text.clone()).collect(),
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
