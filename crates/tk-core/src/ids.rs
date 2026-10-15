//! Ticket IDs like `app-k3x9`. Parsing is the only way to get one from text.

use alloc::string::String;
use core::fmt::{self, Write as _};
use core::str::FromStr;

use nutype::nutype;

/// A project's key and the prefix of its ticket IDs: 2-8 lowercase ASCII letters.
#[nutype(
    validate(predicate = |s: &str| (2..=8).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase())),
    derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, AsRef, Display, Serialize, Deserialize),
)]
pub struct ProjectKey(String);

/// Crockford base32, lowercase: no i, l, o or u.
const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// Four symbols of [`ALPHABET`], so 32^4 = 1,048,576 IDs per project.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Suffix([u8; 4]);

impl Suffix {
    /// The caller passes in random bytes, so the core stays deterministic.
    #[must_use]
    pub fn from_entropy(bytes: [u8; 4]) -> Self {
        Self(bytes.map(symbol))
    }
}

/// The symbol for the low five bits of `byte`. 256 is a multiple of 32,
/// so random bytes give every symbol the same chance.
fn symbol(byte: u8) -> u8 {
    // The mask keeps the index below 32, so the fallback never runs.
    ALPHABET
        .get(usize::from(byte & 0x1f))
        .copied()
        .unwrap_or(b'0')
}

impl FromStr for Suffix {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, IdError> {
        let bytes: [u8; 4] = s.as_bytes().try_into().map_err(|_| IdError::SuffixLength)?;
        if bytes.iter().all(|b| ALPHABET.contains(b)) {
            Ok(Self(bytes))
        } else {
            Err(IdError::SuffixSymbol)
        }
    }
}

impl fmt::Display for Suffix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|&b| f.write_char(char::from(b)))
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TicketId {
    project: ProjectKey,
    suffix: Suffix,
}

impl TicketId {
    #[must_use]
    pub fn new(project: ProjectKey, suffix: Suffix) -> Self {
        Self { project, suffix }
    }

    #[must_use]
    pub fn project(&self) -> &ProjectKey {
        &self.project
    }

    #[must_use]
    pub fn suffix(&self) -> Suffix {
        self.suffix
    }
}

impl FromStr for TicketId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, IdError> {
        let (project, suffix) = s.split_once('-').ok_or(IdError::MissingDash)?;
        Ok(Self {
            project: ProjectKey::try_new(String::from(project)).map_err(IdError::Project)?,
            suffix: suffix.parse()?,
        })
    }
}

/// The exact inverse of `FromStr`.
impl fmt::Display for TicketId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.project, self.suffix)
    }
}

serde_via_str!(TicketId);

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum IdError {
    #[error("a ticket ID is <project>-<suffix>, like app-k3x9")]
    MissingDash,
    #[error("the project key must be 2-8 lowercase letters")]
    Project(ProjectKeyError),
    #[error("the suffix must be 4 characters")]
    SuffixLength,
    #[error("the suffix uses 0-9 and a-z without i, l, o or u")]
    SuffixSymbol,
}

impl crate::text::Explain for ProjectKeyError {
    fn explain(&self) -> &'static str {
        match self {
            Self::PredicateViolated => "a project key is 2-8 lowercase letters, like app",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::string::ToString;

    /// Display then parse gives back the same ID for all 1,048,576 suffixes,
    /// and no two suffixes display the same. Checking every value is a proof.
    #[test]
    fn every_id_round_trips() {
        let project = ProjectKey::try_new(String::from("app")).unwrap();
        let mut seen = BTreeSet::new();
        for a in 0..32 {
            for b in 0..32 {
                for c in 0..32 {
                    for d in 0..32 {
                        let id = TicketId::new(project.clone(), Suffix::from_entropy([a, b, c, d]));
                        let text = id.to_string();
                        assert_eq!(text.parse::<TicketId>(), Ok(id), "{text}");
                        seen.insert(text);
                    }
                }
            }
        }
        assert_eq!(seen.len(), 1 << 20);
    }
}
