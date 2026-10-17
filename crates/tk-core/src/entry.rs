//! What a `## Log` line records: notes, what a handoff leaves behind, and refs,
//! the map of where to look that sessions build up as they find things.

use core::fmt;
use core::str::FromStr;

use alloc::string::String;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::ids::TicketId;
use crate::text::{
    Anchor, AnchorError, Explain as _, NonEmpty, NonEmptyError, REF_SEPARATOR, RepoPath,
    RepoPathError, Url, UrlError,
};

/// The kind a log line names, as written in the file.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Display, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum EntryKind {
    Note,
    RuledOut,
    Decided,
    Next,
    Ref,
    Unref,
}

/// One log entry, parsed. A note is free text; the others are what the next
/// session starts from: facts, the plan and the refs.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Entry {
    Note(NonEmpty),
    RuledOut(NonEmpty),
    Decided(NonEmpty),
    /// Written only by a handoff, so the newest group of these is the current plan.
    Next(NonEmpty),
    /// A place worth reading for this ticket, kept until an `Unref` of the same target.
    Ref(Reference),
    /// A ref that stopped mattering, and why.
    Unref(Reference),
}

impl Entry {
    #[must_use]
    pub fn kind(&self) -> EntryKind {
        match self {
            Self::Note(_) => EntryKind::Note,
            Self::RuledOut(_) => EntryKind::RuledOut,
            Self::Decided(_) => EntryKind::Decided,
            Self::Next(_) => EntryKind::Next,
            Self::Ref(_) => EntryKind::Ref,
            Self::Unref(_) => EntryKind::Unref,
        }
    }

    /// The entry a line's kind and text make, or `None` if the text doesn't fit the kind.
    #[must_use]
    pub fn parse(kind: EntryKind, text: &str) -> Option<Self> {
        let text_of = || NonEmpty::try_new(String::from(text)).ok();
        Some(match kind {
            EntryKind::Note => Self::Note(text_of()?),
            EntryKind::RuledOut => Self::RuledOut(text_of()?),
            EntryKind::Decided => Self::Decided(text_of()?),
            EntryKind::Next => Self::Next(text_of()?),
            EntryKind::Ref => Self::Ref(text.parse().ok()?),
            EntryKind::Unref => Self::Unref(text.parse().ok()?),
        })
    }
}

/// The text after the kind: the exact inverse of `parse`.
impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Note(t) | Self::RuledOut(t) | Self::Decided(t) | Self::Next(t) => t.fmt(f),
            Self::Ref(r) | Self::Unref(r) => r.fmt(f),
        }
    }
}

/// What `tk log` may write. There is deliberately no `Next`: one stray next
/// step would replace the last handoff's plan. Refs have their own command,
/// since their text has a shape.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LogKind {
    #[default]
    Note,
    RuledOut,
    Decided,
}

impl LogKind {
    #[must_use]
    pub fn entry(self, text: NonEmpty) -> Entry {
        match self {
            Self::Note => Entry::Note(text),
            Self::RuledOut => Entry::RuledOut(text),
            Self::Decided => Entry::Decided(text),
        }
    }
}

/// Where a ref points. Written as a ticket ID, an `http(s)://` link, or a
/// repository path with an optional `#anchor`: `plan.md#Handoffs`, `src/lib.rs#fn build`.
/// Text that parses as a ticket ID is one.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RefTarget {
    /// Another ticket, for its decisions.
    Ticket(TicketId),
    /// A page outside the repository. tk never fetches it.
    Url(Url),
    /// A file or directory in the repository. The anchor is text on the line
    /// to start at; for a Markdown heading, its whole section.
    File {
        path: RepoPath,
        anchor: Option<Anchor>,
    },
}

impl FromStr for RefTarget {
    type Err = RefTargetError;

    fn from_str(s: &str) -> Result<Self, RefTargetError> {
        let s = s.trim();
        if s.starts_with("http://") || s.starts_with("https://") {
            return Url::try_new(String::from(s))
                .map(Self::Url)
                .map_err(RefTargetError::Url);
        }
        if let Ok(id) = s.parse::<TicketId>() {
            return Ok(Self::Ticket(id));
        }
        let (path, anchor) = match s.split_once('#') {
            Some((path, anchor)) => (path, Some(anchor)),
            None => (s, None),
        };
        Ok(Self::File {
            path: RepoPath::try_new(String::from(path)).map_err(RefTargetError::Path)?,
            anchor: anchor
                .map(|a| Anchor::try_new(String::from(a)))
                .transpose()
                .map_err(RefTargetError::Anchor)?,
        })
    }
}

/// The exact inverse of `FromStr`.
impl fmt::Display for RefTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ticket(id) => id.fmt(f),
            Self::Url(url) => url.fmt(f),
            Self::File { path, anchor: None } => path.fmt(f),
            Self::File {
                path,
                anchor: Some(anchor),
            } => write!(f, "{path}#{anchor}"),
        }
    }
}

serde_via_str!(RefTarget);

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum RefTargetError {
    #[error("{}", .0.explain())]
    Url(UrlError),
    #[error("{}", .0.explain())]
    Path(RepoPathError),
    #[error("{}", .0.explain())]
    Anchor(AnchorError),
}

/// A ref: where to look, and why it matters, so a session can tell whether to
/// open it without opening it. In a log line, `<target> | <why>`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub target: RefTarget,
    pub why: NonEmpty,
}

impl FromStr for Reference {
    type Err = ReferenceError;

    fn from_str(s: &str) -> Result<Self, ReferenceError> {
        // A target can't contain the separator, so the first one ends it.
        let (target, why) = s
            .split_once(REF_SEPARATOR)
            .ok_or(ReferenceError::Separator)?;
        Ok(Self {
            target: target.parse().map_err(ReferenceError::Target)?,
            why: NonEmpty::try_new(String::from(why)).map_err(ReferenceError::Why)?,
        })
    }
}

/// The exact inverse of `FromStr`.
impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { target, why } = self;
        write!(f, "{target}{REF_SEPARATOR}{why}")
    }
}

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum ReferenceError {
    #[error("a ref is `<target> | <why>`, like `plan.md#Handoffs | the log line format`")]
    Separator,
    #[error(transparent)]
    Target(RefTargetError),
    #[error("a ref's why: {}", .0.explain())]
    Why(NonEmptyError),
}
