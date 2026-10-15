//! Text types. Each is a nutype with its rules declared; construction and
//! deserialization both run them, so there is no other way in.

use alloc::collections::BTreeSet;
// nutype's generated code calls `to_string`, which no_std has no prelude for.
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use nutype::nutype;

/// A ticket title: trimmed, 1-120 characters, one line.
#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 120, predicate = |s: &str| is_one_line(s)),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Display, Serialize, Deserialize),
)]
pub struct Title(String);

/// A label: kebab-case, 1-32 characters, like `export` or `credit-notes`.
#[nutype(
    sanitize(trim, lowercase),
    validate(len_char_max = 32, predicate = |s: &str| is_kebab(s)),
    derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, AsRef, Display, Serialize, Deserialize),
)]
pub struct Label(String);

/// Sorted, without duplicates.
pub type Labels = BTreeSet<Label>;

/// The name in an actor such as `agent:claude`: 1-32 of a-z, 0-9, `.`, `_` and `-`.
#[nutype(
    sanitize(trim, lowercase),
    validate(not_empty, len_char_max = 32, predicate = |s: &str| s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))),
    derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, AsRef, Display, Serialize, Deserialize),
)]
pub struct Name(String);

/// Block reasons and log text: at least one non-blank character, on one line,
/// so a status stays one line in the file and a log entry stays one Markdown line.
#[nutype(
    sanitize(trim),
    validate(not_empty, predicate = |s: &str| is_one_line(s)),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Display, Serialize, Deserialize),
)]
pub struct NonEmpty(String);

/// What a handoff says comes next: at least one step. If nothing does, the ticket is done.
#[nutype(
    validate(predicate = |steps: &Vec<NonEmpty>| !steps.is_empty()),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Serialize, Deserialize),
)]
pub struct NextSteps(Vec<NonEmpty>);

/// The Markdown body. Any text is valid; it belongs to whoever writes the ticket.
#[nutype(
    default = String::new(),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Display, Default, Serialize, Deserialize),
)]
pub struct Body(String);

fn is_one_line(s: &str) -> bool {
    !s.contains(['\n', '\r'])
}

fn is_kebab(s: &str) -> bool {
    s.split('-').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    })
}

/// Why text was rejected, as one sentence for people. Each match is exhaustive
/// over the error nutype generates, so a new rule fails to compile until it is explained.
pub trait Explain {
    fn explain(&self) -> &'static str;
}

impl Explain for TitleError {
    fn explain(&self) -> &'static str {
        match self {
            Self::NotEmptyViolated => "a title can't be blank",
            Self::LenCharMaxViolated => "a title has at most 120 characters",
            Self::PredicateViolated => "a title is one line",
        }
    }
}

impl Explain for LabelError {
    fn explain(&self) -> &'static str {
        match self {
            Self::LenCharMaxViolated => "a label has at most 32 characters",
            Self::PredicateViolated => {
                "a label is lowercase letters and digits joined by single dashes, like credit-notes"
            }
        }
    }
}

impl Explain for NameError {
    fn explain(&self) -> &'static str {
        match self {
            Self::NotEmptyViolated => "a name can't be blank",
            Self::LenCharMaxViolated => "a name has at most 32 characters",
            Self::PredicateViolated => "a name uses only a-z, 0-9, '.', '_' and '-'",
        }
    }
}

impl Explain for NonEmptyError {
    fn explain(&self) -> &'static str {
        match self {
            Self::NotEmptyViolated => "this can't be blank",
            Self::PredicateViolated => "this must be one line",
        }
    }
}

impl Explain for NextStepsError {
    fn explain(&self) -> &'static str {
        match self {
            Self::PredicateViolated => {
                "a handoff needs at least one next step; if nothing is next, finish the ticket"
            }
        }
    }
}
