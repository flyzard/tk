//! Every rule about tickets, as types. No I/O: time and random bytes arrive as arguments.

// The core cannot reach files, environment or clock.
#![cfg_attr(not(test), no_std)]
extern crate alloc;

/// Serde for types whose text form is their `Display` and `FromStr`,
/// so files, JSON and the CLI all parse through the same rules.
macro_rules! serde_via_str {
    ($($ty:ty),*) => {$(
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <alloc::string::String as serde::Deserialize>::deserialize(d)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }
    )*};
}

mod fields;
mod graph;
mod handoff;
mod ids;
#[cfg(feature = "schema")]
mod schema;
mod status;
mod text;
mod ticket;

pub use fields::{Actor, ActorError, EntryKind, Kind, LogKind, Priority};
pub use graph::{DepError, GraphProblem, NotReady, Ready, ValidGraph};
pub use handoff::{AfterHandoff, Handoff};
pub use ids::{IdError, ProjectKey, ProjectKeyError, Suffix, TicketId};
pub use status::{InvalidTransition, Move, Resolution, StateName, Status, TransitionName};
pub use text::{
    Body, Explain, Label, LabelError, Labels, Name, NameError, NextSteps, NextStepsError, NonEmpty,
    NonEmptyError, Title, TitleError,
};
pub use ticket::{Frontmatter, NewTicket, Ticket, TicketView};

/// A UTC instant. Always passed in, never read from a clock.
pub use jiff::Timestamp;
