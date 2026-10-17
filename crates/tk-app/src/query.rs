//! Read-only views. Derived facts like `waiting_on` are computed here, never stored.

use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8PathBuf;
use serde::Serialize;
use tk_core::{
    GraphProblem, Kind, Label, Labels, Priority, RefTarget, StateName, Status, Ticket, TicketId,
    TicketView, Timestamp, Title, ValidGraph,
};
use tk_store::{InvalidFile, Loaded, Snapshot, Store, Tx, Version, log_lines};

use crate::ctx::Ctx;
use crate::error::AppError;
use crate::refs::{self, Linked, LiveRef, Stale, live_refs, ticket_targets};
use crate::report::Finding;

/// A ticket as JSON, with its version and the dependencies it is still waiting on.
#[derive(Serialize, Debug)]
pub struct TicketJson {
    pub id: TicketId,
    pub title: Title,
    pub kind: Kind,
    pub priority: Priority,
    pub labels: Labels,
    pub depends_on: Vec<TicketId>,
    /// Dependencies that aren't done yet. Derived, never stored.
    pub waiting_on: Vec<TicketId>,
    pub created: Timestamp,
    pub status: Status,
    pub body: String,
    pub version: Version,
}

impl TicketJson {
    fn new(loaded: &Loaded, states: &BTreeMap<TicketId, StateName>) -> Self {
        // No `..`: a new Ticket field breaks this line until the JSON decides about it.
        let TicketView {
            id,
            title,
            kind,
            priority,
            labels,
            depends_on,
            created,
            status,
            body,
        } = loaded.ticket().view();
        Self {
            id: id.clone(),
            title: title.clone(),
            kind,
            priority,
            labels: labels.clone(),
            depends_on: depends_on.iter().cloned().collect(),
            waiting_on: waiting(depends_on, |d| states.get(d).copied())
                .map(|(d, _)| d.clone())
                .collect(),
            created,
            status: status.clone(),
            body: body.as_ref().to_owned(),
            version: loaded.version(),
        }
    }
}

/// This project, or every project in the workspace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Current,
    All,
}

impl Scope {
    /// What a `--workspace` flag asks for.
    #[must_use]
    pub fn from_workspace_flag(workspace: bool) -> Self {
        if workspace { Self::All } else { Self::Current }
    }
}

#[derive(Debug)]
pub struct ListFilter {
    pub scope: Scope,
    /// Show done tickets too. Ignored when `states` is given.
    pub all: bool,
    pub states: Vec<StateName>,
    /// Tickets must carry every one of these.
    pub labels: Vec<Label>,
}

/// Listed tickets, and how many files were skipped because they don't parse,
/// so a broken file is never silently missing from a list.
#[derive(Serialize, Debug)]
pub struct Listing {
    pub tickets: Vec<TicketJson>,
    pub needs_fixing: usize,
}

/// # Errors
/// [`AppError::NoProject`] for the current scope outside a project, or a store error.
pub fn list(ctx: &Ctx, filter: &ListFilter) -> Result<Listing, AppError> {
    let mut tickets = Vec::new();
    let mut needs_fixing = 0;
    for store in stores(ctx, filter.scope)? {
        let snapshot = snapshot(store)?;
        needs_fixing += snapshot.invalid.len();
        let states = states(&snapshot);
        tickets.extend(
            snapshot
                .tickets
                .values()
                .filter(|l| matches(l.ticket(), filter))
                .map(|l| TicketJson::new(l, &states)),
        );
    }
    tickets.sort_by(|a, b| order(a).cmp(&order(b)));
    Ok(Listing {
        tickets,
        needs_fixing,
    })
}

fn matches(t: &Ticket, filter: &ListFilter) -> bool {
    let state = t.status().name();
    let state_ok = if filter.states.is_empty() {
        filter.all || state != StateName::Done
    } else {
        filter.states.contains(&state)
    };
    state_ok && filter.labels.iter().all(|l| t.labels.contains(l))
}

/// How tickets are listed everywhere: most urgent first, then oldest.
fn order(t: &TicketJson) -> (Priority, Timestamp, &TicketId) {
    (t.priority, t.created, &t.id)
}

/// One ticket, reading only it and the tickets it depends on.
///
/// # Errors
/// [`AppError::NotFound`], an invalid file or a store error.
pub fn show(ctx: &Ctx, id: &TicketId) -> Result<TicketJson, AppError> {
    ctx.store(id.project())?
        .transaction(|tx| -> Result<TicketJson, AppError> {
            let loaded = tx.load(id)?;
            Ok(TicketJson::new(&loaded, &dep_states(tx, &loaded)))
        })
}

/// The state of each of the ticket's dependencies whose file can be read.
fn dep_states(tx: &Tx<'_>, loaded: &Loaded) -> BTreeMap<TicketId, StateName> {
    loaded
        .ticket()
        .depends_on()
        .iter()
        .filter_map(|d| {
            tx.load(d)
                .ok()
                .map(|l| (d.clone(), l.ticket().status().name()))
        })
        .collect()
}

/// Dependencies that aren't done, with their state. One whose state is
/// unknown, because its file can't be read, counts as not done.
pub(crate) fn waiting(
    depends_on: &BTreeSet<TicketId>,
    state_of: impl Fn(&TicketId) -> Option<StateName>,
) -> impl Iterator<Item = (&TicketId, Option<StateName>)> {
    depends_on
        .iter()
        .map(move |d| (d, state_of(d)))
        .filter(|(_, state)| *state != Some(StateName::Done))
}

/// Every file that doesn't parse, every dependency problem, and every ref on
/// an open ticket whose target is gone.
#[derive(Debug, Default)]
pub struct LintReport {
    pub invalid: Vec<InvalidFile>,
    pub problems: Vec<GraphProblem>,
    pub stale_refs: Vec<StaleRef>,
}

/// A ref on an open ticket that points at nothing, usually after a rename.
/// Refs on done tickets are history and aren't checked.
#[derive(Debug)]
pub struct StaleRef {
    pub id: TicketId,
    /// The ticket's file.
    pub path: Utf8PathBuf,
    pub target: RefTarget,
    pub problem: Stale,
}

impl LintReport {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.invalid.is_empty() && self.problems.is_empty() && self.stale_refs.is_empty()
    }

    /// Everything as findings, the same for `tk lint --json`, MCP and the TUI.
    #[must_use]
    pub fn findings(&self) -> Vec<Finding> {
        self.invalid
            .iter()
            .map(Finding::from)
            .chain(self.problems.iter().map(Finding::from))
            .chain(self.stale_refs.iter().map(Finding::from))
            .collect()
    }
}

/// The refs on open tickets in `snapshot` whose targets can't be found.
fn stale_refs(ctx: &Ctx, store: &Store, snapshot: &Snapshot) -> Vec<StaleRef> {
    let mut stale = Vec::new();
    let open = snapshot
        .tickets
        .values()
        .filter(|l| l.ticket().status().name() != StateName::Done);
    for loaded in open {
        let live = live_refs(&log_lines(loaded.ticket().body.as_ref()));
        if live.is_empty() {
            continue;
        }
        let id = loaded.ticket().id();
        let linked = refs::linked(ctx, id.project(), ticket_targets(&live), |id| {
            snapshot.tickets.get(id).map(Linked::of)
        });
        for LiveRef {
            reference,
            at: _,
            by: _,
        } in live
        {
            if let Some(problem) = refs::locate(store.repo(), &reference.target, &linked).stale() {
                stale.push(StaleRef {
                    id: id.clone(),
                    path: store.path(id),
                    target: reference.target,
                    problem,
                });
            }
        }
    }
    stale
}

/// # Errors
/// [`AppError::NoProject`] for the current scope outside a project, or a store error.
pub fn lint(ctx: &Ctx, scope: Scope) -> Result<LintReport, AppError> {
    let mut report = LintReport::default();
    for store in stores(ctx, scope)? {
        let snapshot = snapshot(store)?;
        report.stale_refs.extend(stale_refs(ctx, store, &snapshot));
        check(&mut report, snapshot);
    }
    Ok(report)
}

/// Every ticket, every problem, and what `tk next` could start: what the TUI shows.
#[derive(Debug, Default)]
pub struct Survey {
    pub tickets: Vec<TicketJson>,
    pub report: LintReport,
    /// Tickets the same graph `tk next` uses would offer, ignoring leases.
    pub ready: BTreeSet<TicketId>,
}

/// All of it from one read of each project.
///
/// # Errors
/// [`AppError::NoProject`] for the current scope outside a project, or a store error.
pub fn survey(ctx: &Ctx, scope: Scope) -> Result<Survey, AppError> {
    let mut survey = Survey::default();
    for store in stores(ctx, scope)? {
        let snapshot = snapshot(store)?;
        let states = states(&snapshot);
        survey.tickets.extend(
            snapshot
                .tickets
                .values()
                .map(|l| TicketJson::new(l, &states)),
        );
        survey
            .report
            .stale_refs
            .extend(stale_refs(ctx, store, &snapshot));
        let graph = check(&mut survey.report, snapshot);
        survey.ready.extend(graph.ready_ids().cloned());
    }
    survey.tickets.sort_by(|a, b| order(a).cmp(&order(b)));
    Ok(survey)
}

/// Adds one project's unreadable files and dependency problems to `report`,
/// and returns the graph of the rest.
fn check(report: &mut LintReport, snapshot: Snapshot) -> ValidGraph {
    let (graph, problems) = ValidGraph::build(snapshot.tickets());
    report.problems.extend(problems);
    report.invalid.extend(snapshot.invalid);
    graph
}

pub(crate) fn stores(ctx: &Ctx, scope: Scope) -> Result<Vec<&Store>, AppError> {
    match scope {
        Scope::All => Ok(ctx.stores.values().collect()),
        Scope::Current => Ok(vec![ctx.store(&ctx.project(None)?)?]),
    }
}

pub(crate) fn snapshot(store: &Store) -> Result<Snapshot, AppError> {
    store.transaction(|tx| -> Result<Snapshot, AppError> { Ok(tx.snapshot()?) })
}

pub(crate) fn states(snapshot: &Snapshot) -> BTreeMap<TicketId, StateName> {
    snapshot
        .tickets()
        .map(|t| (t.id().clone(), t.status().name()))
        .collect()
}
