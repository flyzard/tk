//! The status state machine. Each state carries only the data valid in it,
//! and every move between states is decided in one exhaustive match.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString, IntoStaticStr, VariantNames};

use crate::fields::Actor;
use crate::ids::TicketId;
use crate::text::NonEmpty;
use jiff::Timestamp;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Status {
    Todo,
    Doing {
        by: Actor,
        since: Timestamp,
    },
    Blocked {
        reason: NonEmpty,
        since: Timestamp,
    },
    Done {
        at: Timestamp,
        resolution: Resolution,
    },
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Completed,
    WontDo,
    Duplicate(TicketId),
}

/// What callers can ask for. There is deliberately no `Start` here:
/// only [`Ready::start`](crate::Ready::start) can start a ticket.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Move {
    Block { reason: NonEmpty },
    Unblock,
    Release,
    Finish { resolution: Resolution },
    Reopen,
}

/// Every transition, `Start` included. Private to tk-core.
pub(crate) enum Transition {
    Start { by: Actor },
    Move(Move),
}

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    Serialize,
    Deserialize,
    Display,
    EnumString,
    IntoStaticStr,
    VariantNames,
    EnumIter,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum StateName {
    Todo,
    Doing,
    Blocked,
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Display, IntoStaticStr, Serialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TransitionName {
    Start,
    Block,
    Unblock,
    Release,
    Finish,
    Reopen,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
#[error("cannot {attempted} a ticket that is {from}")]
pub struct InvalidTransition {
    pub from: StateName,
    pub attempted: TransitionName,
}

impl Status {
    #[must_use]
    pub fn name(&self) -> StateName {
        match self {
            Self::Todo => StateName::Todo,
            Self::Doing { .. } => StateName::Doing,
            Self::Blocked { .. } => StateName::Blocked,
            Self::Done { .. } => StateName::Done,
        }
    }
}

impl Move {
    #[must_use]
    pub fn name(&self) -> TransitionName {
        match self {
            Self::Block { .. } => TransitionName::Block,
            Self::Unblock => TransitionName::Unblock,
            Self::Release => TransitionName::Release,
            Self::Finish { .. } => TransitionName::Finish,
            Self::Reopen => TransitionName::Reopen,
        }
    }
}

impl Transition {
    fn name(&self) -> TransitionName {
        match self {
            Self::Start { .. } => TransitionName::Start,
            Self::Move(m) => m.name(),
        }
    }
}

/// The one place transitions are defined.
pub(crate) fn next(
    current: &Status,
    t: Transition,
    now: Timestamp,
) -> Result<Status, InvalidTransition> {
    use Move as M;
    use Status as S;
    use Transition as T;
    match (current, t) {
        (S::Todo, T::Start { by }) => Ok(S::Doing { by, since: now }),
        (S::Todo | S::Doing { .. }, T::Move(M::Block { reason })) => {
            Ok(S::Blocked { reason, since: now })
        }
        (S::Blocked { .. }, T::Move(M::Unblock))
        | (S::Doing { .. }, T::Move(M::Release))
        | (S::Done { .. }, T::Move(M::Reopen)) => Ok(S::Todo),
        (S::Todo | S::Doing { .. } | S::Blocked { .. }, T::Move(M::Finish { resolution })) => {
            Ok(S::Done {
                at: now,
                resolution,
            })
        }
        // Every invalid pair is named, never `_`: a new state, move or transition
        // fails to compile until each pairing is decided.
        (S::Doing { .. } | S::Blocked { .. } | S::Done { .. }, t @ T::Start { .. })
        | (S::Todo, t @ T::Move(M::Unblock | M::Release | M::Reopen))
        | (S::Doing { .. }, t @ T::Move(M::Unblock | M::Reopen))
        | (S::Blocked { .. }, t @ T::Move(M::Block { .. } | M::Release | M::Reopen))
        | (
            S::Done { .. },
            t @ T::Move(M::Block { .. } | M::Unblock | M::Release | M::Finish { .. }),
        ) => Err(InvalidTransition {
            from: current.name(),
            attempted: t.name(),
        }),
    }
}
