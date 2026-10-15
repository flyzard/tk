use std::collections::BTreeSet;

use serde::Serialize;
use tk_core::{
    Actor, Body, Move, NewTicket, Priority, ProjectKey, Resolution, StateName, Status, Suffix,
    Ticket, TicketId, Timestamp, Title, ValidGraph,
};
use tk_store::{Leases, Loaded, Snapshot, Store, StoreError, Tx, Version, VersionHex, append_log};

use crate::command::{Command, FieldChange};
use crate::ctx::Ctx;
use crate::error::AppError;

/// How many random IDs `new` tries before giving up. A clash is about one in a million per try.
const ID_ATTEMPTS: usize = 16;

/// How long a started ticket stays claimed without a write from its holder. A first guess; tune once agents run in parallel.
const LEASE_TTL: jiff::SignedDuration = jiff::SignedDuration::from_hours(2);

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Created {
        id: TicketId,
        version: Version,
    },
    Updated {
        id: TicketId,
        version: Version,
        state: StateName,
    },
    Next {
        id: TicketId,
        title: Title,
        priority: Priority,
    },
    NothingReady,
}

/// Runs one command. The CLI, `tk apply`, the TUI and later MCP all end up here.
/// Problems that don't undo the command, such as a lease that couldn't be
/// renewed after a save, go to `ctx.warnings`.
///
/// # Errors
/// Any [`AppError`]; nothing is written when a command fails.
pub fn execute(cmd: Command, ctx: &mut Ctx) -> Result<Outcome, AppError> {
    let now = ctx.now;
    match cmd {
        Command::New {
            project,
            title,
            kind,
            priority,
            labels,
            depends_on,
            body,
        } => {
            let key = ctx.project(project)?;
            let spec = || {
                NewTicket::builder()
                    .title(title.clone())
                    .kind(kind)
                    .priority(priority)
                    .labels(labels.clone())
                    .body(body.clone())
                    .build()
            };
            create(ctx, &key, spec, &depends_on)
        }
        Command::Edit {
            id,
            if_version,
            changes,
        } => change(ctx, &id, if_version, |_, mut t| {
            for c in changes {
                edit(&mut t, c);
            }
            Ok(t)
        }),
        Command::Start { id, if_version } => {
            let store = ctx.store(id.project())?;
            let leases = store.leases()?;
            store.transaction(|tx| {
                let mut snapshot = tx.snapshot()?;
                let graph = usable_graph(&snapshot);
                let loaded = take(tx, &mut snapshot, &id)?;
                start_ticket(
                    tx,
                    &graph,
                    leases.as_ref(),
                    loaded,
                    if_version.as_ref(),
                    &ctx.actor,
                    now,
                )
            })
        }
        Command::Block {
            id,
            if_version,
            reason,
        } => moved(ctx, &id, if_version, Move::Block { reason }),
        Command::Unblock { id, if_version } => moved(ctx, &id, if_version, Move::Unblock),
        Command::Release { id, if_version } => moved(ctx, &id, if_version, Move::Release),
        Command::Finish {
            id,
            if_version,
            resolution,
        } => {
            if let Resolution::Duplicate(of) = &resolution {
                check_duplicate(ctx, &id, of)?;
            }
            moved(ctx, &id, if_version, Move::Finish { resolution })
        }
        Command::Reopen { id, if_version } => moved(ctx, &id, if_version, Move::Reopen),
        Command::Log { id, text } => {
            let entry = format!("- {now} {}: {text}", ctx.actor);
            change(ctx, &id, None, |_, mut t| {
                t.body = Body::new(append_log(t.body.as_ref(), &entry));
                Ok(t)
            })
        }
        Command::DepAdd {
            id,
            dep,
            if_version,
        } => change(ctx, &id, if_version, |tx, t| {
            usable_graph(&tx.snapshot()?)
                .add_dep(t, dep)
                .map_err(|(_, e)| AppError::Dep(e))
        }),
        Command::DepRemove {
            id,
            dep,
            if_version,
        } => change(ctx, &id, if_version, |_, t| Ok(t.remove_dep(&dep))),
        Command::Next { project, claim } => next(ctx, project, claim),
    }
}

/// Loads one ticket, checks the expected version, applies `f` and saves, all
/// under the project lock. Then keeps the lease in step with the new status.
fn change(
    ctx: &mut Ctx,
    id: &TicketId,
    if_version: Option<VersionHex>,
    f: impl FnOnce(&Tx<'_>, Ticket) -> Result<Ticket, AppError>,
) -> Result<Outcome, AppError> {
    let store = ctx.store(id.project())?;
    let (outcome, before, after) = store.transaction(|tx| -> Result<_, AppError> {
        let loaded = tx.load(id)?;
        loaded
            .check_version(if_version.as_ref())
            .map_err(AppError::Conflict)?;
        let before = loaded.ticket().status().name();
        let edited = loaded.update(|t| f(tx, t))?;
        let after = edited.after().status().clone();
        let saved = tx.save(edited)?;
        Ok((
            Outcome::Updated {
                id: saved.id,
                version: saved.version,
                state: after.name(),
            },
            before,
            after,
        ))
    })?;
    // The change is on disk; a lease that can't keep up only warns.
    if let Err(e) = keep_lease(store, id, before, &after, &ctx.actor, ctx.now) {
        ctx.warnings
            .push(format!("{id} saved, but its lease wasn't updated: {e}"));
    }
    Ok(outcome)
}

/// Renewed while its holder works on the ticket; released when this checkout
/// moves it out of `doing`. A ticket this checkout never saw as `doing` may be
/// leased from another worktree, so its lease is left alone.
fn keep_lease(
    store: &Store,
    id: &TicketId,
    before: StateName,
    after: &Status,
    actor: &Actor,
    now: Timestamp,
) -> Result<(), AppError> {
    let renew = matches!(after, Status::Doing { by, .. } if by == actor);
    let release = before == StateName::Doing && after.name() != StateName::Doing;
    if !(renew || release) {
        return Ok(());
    }
    let Some(leases) = store.leases()? else {
        return Ok(());
    };
    if renew {
        leases.renew(id, actor, lease_until(now)?)?;
    } else {
        leases.release(id)?;
    }
    Ok(())
}

fn lease_until(now: Timestamp) -> Result<Timestamp, AppError> {
    now.checked_add(LEASE_TTL)
        .map_err(|e| AppError::Internal(e.to_string()))
}

fn moved(
    ctx: &mut Ctx,
    id: &TicketId,
    if_version: Option<VersionHex>,
    m: Move,
) -> Result<Outcome, AppError> {
    let now = ctx.now;
    change(ctx, id, if_version, |_, t| Ok(t.transition(m, now)?))
}

/// The one way to start a ticket, for `start` and `next --claim`. It needs a
/// `Ready` ticket, which only the graph hands out, and the lease, which only one
/// agent across all worktrees can hold. The lease is a guard: it is kept only
/// once the start is saved, and released if anything fails before that.
fn start_ticket(
    tx: &mut Tx<'_>,
    graph: &ValidGraph,
    leases: Option<&Leases>,
    loaded: Loaded,
    if_version: Option<&VersionHex>,
    actor: &Actor,
    now: Timestamp,
) -> Result<Outcome, AppError> {
    loaded
        .check_version(if_version)
        .map_err(AppError::Conflict)?;
    let mut lease = None;
    let edited = loaded.update(|t| -> Result<Ticket, AppError> {
        let ready = graph.ready(t).map_err(|(_, e)| AppError::NotReady(e))?;
        if let Some(leases) = leases {
            lease = Some(leases.acquire(ready.ticket().id(), actor, now, lease_until(now)?)?);
        }
        Ok(ready.start(actor.clone(), now)?)
    })?;
    let saved = tx.save(edited)?;
    if let Some(lease) = lease {
        lease.keep();
    }
    Ok(Outcome::Updated {
        id: saved.id,
        version: saved.version,
        state: StateName::Doing,
    })
}

/// The ticket from a snapshot already read, or from disk for the error a missing or broken file gives.
fn take(tx: &Tx<'_>, snapshot: &mut Snapshot, id: &TicketId) -> Result<Loaded, AppError> {
    match snapshot.tickets.remove(id) {
        Some(loaded) => Ok(loaded),
        None => Ok(tx.load(id)?),
    }
}

/// The part of the project without dependency problems, so a broken ticket
/// elsewhere doesn't stop unrelated work. `tk lint` reports the problems.
fn usable_graph(snapshot: &Snapshot) -> ValidGraph {
    ValidGraph::build(snapshot.tickets()).0
}

/// A duplicate must point at another ticket that exists.
fn check_duplicate(ctx: &Ctx, id: &TicketId, of: &TicketId) -> Result<(), AppError> {
    if of == id {
        return Err(AppError::Invalid(String::from(
            "a ticket can't be a duplicate of itself",
        )));
    }
    ctx.store(of.project())?
        .transaction(|tx| -> Result<(), AppError> {
            tx.load(of).map(|_| ()).map_err(AppError::from)
        })
}

fn edit(t: &mut Ticket, change: FieldChange) {
    match change {
        FieldChange::Title(title) => t.title = title,
        FieldChange::Kind(kind) => t.kind = kind,
        FieldChange::Priority(priority) => t.priority = priority,
        FieldChange::AddLabel(label) => {
            t.labels.insert(label);
        }
        FieldChange::RemoveLabel(label) => {
            t.labels.remove(&label);
        }
        FieldChange::Body(body) => t.body = body,
    }
}

/// Writes a new ticket under a random ID. The write refuses to overwrite, so a
/// taken ID just means another try; the project is only read when dependencies
/// need checking.
fn create(
    ctx: &mut Ctx,
    key: &ProjectKey,
    spec: impl Fn() -> NewTicket,
    depends_on: &BTreeSet<TicketId>,
) -> Result<Outcome, AppError> {
    let Ctx {
        stores,
        now,
        entropy,
        ..
    } = ctx;
    let store = stores
        .get(key)
        .ok_or_else(|| AppError::UnknownProject(key.clone()))?;
    store.transaction(|tx| {
        let graph = if depends_on.is_empty() {
            None
        } else {
            Some(usable_graph(&tx.snapshot()?))
        };
        for _ in 0..ID_ATTEMPTS {
            let mut ticket = Ticket::new(
                spec(),
                TicketId::new(key.clone(), Suffix::from_entropy(entropy()?)),
                *now,
            );
            if let Some(graph) = &graph {
                for dep in depends_on {
                    ticket = graph
                        .add_dep(ticket, dep.clone())
                        .map_err(|(_, e)| AppError::Dep(e))?;
                }
            }
            match tx.create(ticket) {
                Ok(saved) => {
                    return Ok(Outcome::Created {
                        id: saved.id,
                        version: saved.version,
                    });
                }
                Err(StoreError::Exists(_)) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Err(AppError::Internal(format!(
            "no free ticket ID after {ID_ATTEMPTS} tries"
        )))
    })
}

/// The top ready ticket by (priority, created) that nobody else holds a lease
/// on: in `project`, else the current project, else across all projects.
/// With `claim`, starts it, picking again under the lock it is started in.
fn next(ctx: &Ctx, project: Option<ProjectKey>, claim: bool) -> Result<Outcome, AppError> {
    let stores: Vec<&Store> = match project.or_else(|| ctx.current.clone()) {
        Some(key) => vec![ctx.store(&key)?],
        None => ctx.stores.values().collect(),
    };
    let (actor, now) = (&ctx.actor, ctx.now);
    // One project: pick and claim in a single read.
    if claim && let [store] = stores.as_slice() {
        return claim_in(store, actor, now);
    }
    let mut best: Option<Candidate<'_>> = None;
    for store in stores {
        let top = store.transaction(|tx| -> Result<_, AppError> {
            let snapshot = tx.snapshot()?;
            for id in usable_graph(&snapshot).ready_ids() {
                if let Some(t) = snapshot.tickets.get(id).map(Loaded::ticket)
                    && free(store, id, actor, now)?
                {
                    return Ok(Some((
                        t.priority,
                        t.created(),
                        t.id().clone(),
                        t.title.clone(),
                    )));
                }
            }
            Ok(None)
        })?;
        if let Some((priority, created, id, title)) = top
            && best
                .as_ref()
                .is_none_or(|b| (priority, created, &id) < (b.priority, b.created, &b.id))
        {
            best = Some(Candidate {
                priority,
                created,
                id,
                title,
                store,
            });
        }
    }
    match best {
        None => Ok(Outcome::NothingReady),
        Some(Candidate {
            priority,
            created: _,
            id,
            title,
            store: _,
        }) if !claim => Ok(Outcome::Next {
            id,
            title,
            priority,
        }),
        Some(Candidate {
            priority: _,
            created: _,
            id: _,
            title: _,
            store,
        }) => claim_in(store, actor, now),
    }
}

/// Starts the first ready ticket in `store` whose lease is free.
fn claim_in(store: &Store, actor: &Actor, now: Timestamp) -> Result<Outcome, AppError> {
    let leases = store.leases()?;
    store.transaction(|tx| {
        let mut snapshot = tx.snapshot()?;
        let graph = usable_graph(&snapshot);
        for id in graph.ready_ids() {
            let loaded = take(tx, &mut snapshot, id)?;
            match start_ticket(tx, &graph, leases.as_ref(), loaded, None, actor, now) {
                // Someone in another worktree has it: try the next one.
                Err(AppError::Claimed(_)) => {}
                result => return result,
            }
        }
        Ok(Outcome::NothingReady)
    })
}

/// The best ready ticket found so far, and where it lives.
struct Candidate<'a> {
    priority: Priority,
    created: Timestamp,
    id: TicketId,
    title: Title,
    store: &'a Store,
}

/// Whether `actor` may take `id`: no live lease, or its own.
fn free(store: &Store, id: &TicketId, actor: &Actor, now: Timestamp) -> Result<bool, AppError> {
    match store.leases()? {
        Some(leases) => Ok(leases.holder(id, now)?.is_none_or(|by| &by == actor)),
        None => Ok(true),
    }
}
