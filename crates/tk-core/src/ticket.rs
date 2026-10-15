//! The ticket. Status and dependencies are private: they change only through
//! transitions and [`ValidGraph`](crate::ValidGraph).

use alloc::collections::BTreeSet;
use serde::Deserialize;

use crate::fields::{Kind, Priority};
use crate::ids::TicketId;
use crate::status::{InvalidTransition, Move, Status, Transition, next};
use crate::text::{Body, Labels, Title};
use jiff::Timestamp;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ticket {
    id: TicketId,
    pub title: Title,
    pub kind: Kind,
    pub priority: Priority,
    pub labels: Labels,
    depends_on: BTreeSet<TicketId>,
    created: Timestamp,
    status: Status,
    pub body: Body,
}

/// What a new ticket needs. Leaving out the title is a compile error.
/// Dependencies are added afterwards through `ValidGraph::add_dep`, so they are cycle-checked.
#[derive(bon::Builder)]
pub struct NewTicket {
    title: Title,
    #[builder(default)]
    kind: Kind,
    #[builder(default)]
    priority: Priority,
    #[builder(default)]
    labels: Labels,
    #[builder(default)]
    body: Body,
}

/// A ticket's stored fields. Private fields and no constructor: the only way
/// to get one is to deserialize a file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frontmatter {
    title: Title,
    kind: Kind,
    priority: Priority,
    #[serde(default)]
    labels: Labels,
    #[serde(default)]
    depends_on: BTreeSet<TicketId>,
    created: Timestamp,
    status: Status,
}

/// Every field, borrowed, for rendering. Destructure it without `..`, so a new
/// field fails to compile until each renderer decides about it.
#[derive(Clone, Copy, Debug)]
pub struct TicketView<'a> {
    pub id: &'a TicketId,
    pub title: &'a Title,
    pub kind: Kind,
    pub priority: Priority,
    pub labels: &'a Labels,
    pub depends_on: &'a BTreeSet<TicketId>,
    pub created: Timestamp,
    pub status: &'a Status,
    pub body: &'a Body,
}

impl Ticket {
    /// New tickets always start as `Todo`, with no dependencies.
    #[must_use]
    pub fn new(spec: NewTicket, id: TicketId, now: Timestamp) -> Self {
        let NewTicket {
            title,
            kind,
            priority,
            labels,
            body,
        } = spec;
        Self {
            id,
            title,
            kind,
            priority,
            labels,
            depends_on: BTreeSet::new(),
            created: now,
            status: Status::Todo,
            body,
        }
    }

    /// The way in for a ticket read from disk. The ID comes from the filename.
    #[must_use]
    pub fn from_stored(id: TicketId, front: Frontmatter, body: Body) -> Self {
        let Frontmatter {
            title,
            kind,
            priority,
            labels,
            depends_on,
            created,
            status,
        } = front;
        Self {
            id,
            title,
            kind,
            priority,
            labels,
            depends_on,
            created,
            status,
            body,
        }
    }

    /// # Errors
    /// [`InvalidTransition`] when the move isn't allowed from the current status.
    pub fn transition(self, m: Move, now: Timestamp) -> Result<Self, InvalidTransition> {
        self.apply(Transition::Move(m), now)
    }

    pub(crate) fn apply(self, t: Transition, now: Timestamp) -> Result<Self, InvalidTransition> {
        let status = next(&self.status, t, now)?;
        Ok(Self { status, ..self })
    }

    /// Removing a dependency can't create a cycle, so it needs no graph.
    #[must_use]
    pub fn remove_dep(mut self, dep: &TicketId) -> Self {
        self.depends_on.remove(dep);
        self
    }

    /// Only `ValidGraph::add_dep` calls this, after the cycle check.
    pub(crate) fn with_dep(mut self, dep: TicketId) -> Self {
        self.depends_on.insert(dep);
        self
    }

    #[must_use]
    pub fn id(&self) -> &TicketId {
        &self.id
    }

    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }

    #[must_use]
    pub fn status(&self) -> &Status {
        &self.status
    }

    #[must_use]
    pub fn depends_on(&self) -> &BTreeSet<TicketId> {
        &self.depends_on
    }

    #[must_use]
    pub fn view(&self) -> TicketView<'_> {
        let Self {
            id,
            title,
            kind,
            priority,
            labels,
            depends_on,
            created,
            status,
            body,
        } = self;
        TicketView {
            id,
            title,
            kind: *kind,
            priority: *priority,
            labels,
            depends_on,
            created: *created,
            status,
            body,
        }
    }
}
