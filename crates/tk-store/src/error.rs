use std::io;
use std::ops::Range;

use camino::Utf8PathBuf;
use tk_core::{ProjectKey, TicketId};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("ticket {0} not found")]
    NotFound(TicketId),
    #[error("ticket {0} already exists")]
    Exists(TicketId),
    #[error(transparent)]
    Conflict(Conflict),
    #[error(transparent)]
    Invalid(Box<InvalidFile>),
    /// The render-and-reparse check in `save` or `create` failed: a bug in tk,
    /// caught before a file tk can't read reached disk.
    #[error("refusing to write {0}: the rendered file doesn't read back as the same ticket")]
    Unreadable(TicketId),
    #[error("no tk project at {0}; run `tk init --key <key>` there")]
    NoProject(Utf8PathBuf),
    #[error("a project already exists at {0}")]
    ProjectExists(Utf8PathBuf),
    #[error("{path} has format {found}; this tk reads format 1")]
    Format { path: Utf8PathBuf, found: u32 },
    #[error("{path}: {message}")]
    Config { path: Utf8PathBuf, message: String },
    #[error("ticket {id} belongs to project {}, not {key}", id.project())]
    WrongProject { id: TicketId, key: ProjectKey },
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[error("ticket {id} changed since you read it")]
pub struct Conflict {
    pub id: TicketId,
    pub expected: String,
    pub found: String,
}

/// A ticket file that didn't parse. Loading a project keeps going past it.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[error("{path}: {error}")]
pub struct InvalidFile {
    pub path: Utf8PathBuf,
    /// The file's text, when it was readable as UTF-8, for pointing at the error.
    pub text: Option<String>,
    pub error: ParseError,
}

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum ParseError {
    #[error("a ticket file is named <id>.md, like app-k3x9.md")]
    FileName,
    #[error("ticket {id} doesn't belong to project {key}")]
    WrongProject { id: TicketId, key: ProjectKey },
    #[error("the file isn't UTF-8")]
    NotUtf8,
    #[error("the file must start with a +++ line")]
    NoFrontmatter,
    #[error("the frontmatter has no closing +++ line")]
    Unclosed,
    /// The frontmatter isn't valid TOML.
    #[error("{message}")]
    Toml {
        message: String,
        span: Option<Range<usize>>,
    },
    /// Valid TOML, but a field is missing, unknown or breaks its rules.
    #[error("{message}")]
    Fields {
        message: String,
        span: Option<Range<usize>>,
    },
}

impl ParseError {
    /// Where in the file the problem is, as a byte range.
    #[must_use]
    pub fn span(&self) -> Option<Range<usize>> {
        match self {
            Self::Toml { span, .. } | Self::Fields { span, .. } => span.clone(),
            Self::FileName
            | Self::WrongProject { .. }
            | Self::NotUtf8
            | Self::NoFrontmatter
            | Self::Unclosed => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError<E> {
    #[error(transparent)]
    Rejected(E),
    #[error("an update must return the same ticket it was given")]
    IdChanged,
}
