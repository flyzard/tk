//! Ticket fields with a fixed set of values.

use core::fmt;
use core::str::FromStr;

use alloc::string::String;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString, IntoStaticStr, VariantNames};

use crate::text::{Name, NameError};

/// Variants are declared most urgent first, so sorting puts P0 at the top.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    Default,
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
pub enum Priority {
    P0,
    P1,
    #[default]
    P2,
    P3,
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
    Default,
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
pub enum Kind {
    Bug,
    #[default]
    Feature,
    Chore,
    Spike,
}

/// Who did something: `human:<name>` or `agent:<name>`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Actor {
    Human(Name),
    Agent(Name),
}

impl Actor {
    #[must_use]
    pub fn name(&self) -> &Name {
        match self {
            Self::Human(name) | Self::Agent(name) => name,
        }
    }
}

impl FromStr for Actor {
    type Err = ActorError;

    fn from_str(s: &str) -> Result<Self, ActorError> {
        let (kind, name) = s.split_once(':').ok_or(ActorError::Prefix)?;
        let name = Name::try_new(String::from(name)).map_err(ActorError::Name)?;
        match kind {
            "human" => Ok(Self::Human(name)),
            "agent" => Ok(Self::Agent(name)),
            _ => Err(ActorError::Prefix),
        }
    }
}

/// The exact inverse of `FromStr`.
impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Human(name) => write!(f, "human:{name}"),
            Self::Agent(name) => write!(f, "agent:{name}"),
        }
    }
}

serde_via_str!(Actor);

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum ActorError {
    #[error("an actor is human:<name> or agent:<name>")]
    Prefix,
    #[error("an actor's name is 1-32 of a-z, 0-9, '.', '_' and '-'")]
    Name(NameError),
}
