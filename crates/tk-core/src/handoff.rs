//! What a session leaves behind, so the next one starts from a brief instead of the history.

use alloc::vec::Vec;
use serde::Deserialize;

use crate::fields::EntryKind;
use crate::status::Move;
use crate::text::{NextSteps, NonEmpty};

/// The log entries a handoff writes. Next steps are required; the facts are optional.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Handoff {
    pub next: NextSteps,
    /// Things tried that didn't work, so the next session skips them.
    #[serde(default)]
    pub ruled_out: Vec<NonEmpty>,
    /// Choices made, so the next session doesn't reopen them.
    #[serde(default)]
    pub decided: Vec<NonEmpty>,
}

impl Handoff {
    /// Every entry, in the order they are logged: the facts, then the plan.
    #[must_use]
    pub fn entries(self) -> Vec<(EntryKind, NonEmpty)> {
        let Self {
            next,
            ruled_out,
            decided,
        } = self;
        ruled_out
            .into_iter()
            .map(|t| (EntryKind::RuledOut, t))
            .chain(decided.into_iter().map(|t| (EntryKind::Decided, t)))
            .chain(next.into_inner().into_iter().map(|t| (EntryKind::Next, t)))
            .collect()
    }
}

/// What happens to the ticket after a handoff.
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum AfterHandoff {
    /// The same actor carries on in a fresh session; the lease is renewed.
    #[default]
    Keep,
    /// Back to todo for anyone.
    Release,
    Block {
        reason: NonEmpty,
    },
}

impl AfterHandoff {
    /// The move to make, if any. It goes through the same transitions as every other.
    #[must_use]
    pub fn into_move(self) -> Option<Move> {
        match self {
            Self::Keep => None,
            Self::Release => Some(Move::Release),
            Self::Block { reason } => Some(Move::Block { reason }),
        }
    }
}
