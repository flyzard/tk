use std::io;

use serde::Serialize;
use tk_core::{DepError, InvalidTransition, NotReady, ProjectKey, TicketId};
use tk_store::{AcquireError, Claimed, Conflict, InvalidFile, StoreError, UpdateError};

use crate::refs::{RefError, Stale};

/// A new variant fails to compile until it has a code, an exit status and a hint.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("ticket {0} not found")]
    NotFound(TicketId),
    #[error(transparent)]
    InvalidTransition(#[from] InvalidTransition),
    #[error(transparent)]
    Conflict(Conflict),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error(transparent)]
    Dep(DepError),
    #[error(transparent)]
    NotReady(NotReady),
    #[error(transparent)]
    Claimed(Claimed),
    #[error(transparent)]
    InvalidFile(Box<InvalidFile>),
    #[error(transparent)]
    Ref(#[from] RefError),
    #[error("no project with key {0} in the workspace")]
    UnknownProject(ProjectKey),
    #[error("not inside a tk project")]
    NoProject,
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Internal(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    InvalidTransition,
    Conflict,
    Invalid,
    Dep,
    NotReady,
    Claimed,
    InvalidFile,
    BadRef,
    UnknownProject,
    NoProject,
    Config,
    Internal,
    Io,
}

/// What goes to stderr: `{"code":"conflict","message":"...","hint":"..."}`.
#[derive(Serialize, Debug)]
pub struct ErrorJson {
    pub code: ErrorCode,
    pub message: String,
    pub hint: Option<String>,
}

impl AppError {
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::NotFound(_) => ErrorCode::NotFound,
            Self::InvalidTransition(_) => ErrorCode::InvalidTransition,
            Self::Conflict(_) => ErrorCode::Conflict,
            Self::Invalid(_) => ErrorCode::Invalid,
            Self::Dep(_) => ErrorCode::Dep,
            Self::NotReady(_) => ErrorCode::NotReady,
            Self::Claimed(_) => ErrorCode::Claimed,
            Self::InvalidFile(_) => ErrorCode::InvalidFile,
            Self::Ref(_) => ErrorCode::BadRef,
            Self::UnknownProject(_) => ErrorCode::UnknownProject,
            Self::NoProject => ErrorCode::NoProject,
            Self::Config(_) => ErrorCode::Config,
            Self::Internal(_) => ErrorCode::Internal,
            Self::Io(_) => ErrorCode::Io,
        }
    }

    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::NotFound(_)
            | Self::Ref(RefError::Stale {
                target: _,
                problem: Stale::NoTicket,
            }) => Some(String::from("run `tk ls --all` to see ticket IDs")),
            Self::InvalidTransition(_) => {
                Some(String::from("`tk guide` lists the allowed status changes"))
            }
            Self::Conflict(c) => Some(format!(
                "run `tk show {}` and retry with its new version",
                c.id
            )),
            Self::Invalid(_) => Some(String::from("`tk schema` prints what each command accepts")),
            Self::NotReady(NotReady::WaitingOn(_)) => Some(String::from(
                "finish its dependencies first, or run `tk next`",
            )),
            Self::Claimed(_) => Some(String::from(
                "another agent is on it; run `tk next` to find other work",
            )),
            Self::InvalidFile(_) => Some(String::from("run `tk lint` to see every problem")),
            Self::Ref(RefError::Stale {
                target: _,
                problem: Stale::NoFile | Stale::NoAnchor,
            }) => Some(String::from(
                "a path is from the repository root, and an anchor is text on the line to start at, like plan.md#Handoffs",
            )),
            Self::Ref(RefError::NotReferenced(id, _)) => Some(format!(
                "`tk brief {id}` lists its refs under Where to look"
            )),
            Self::UnknownProject(_) => {
                Some(String::from("run `tk init --key <key>` in that repository"))
            }
            Self::NoProject => Some(String::from(
                "pass --project, or run `tk init --key <key>` in this repository",
            )),
            Self::Internal(_) => Some(String::from("this is a bug in tk")),
            Self::NotReady(NotReady::Broken) => {
                Some(String::from("run `tk lint` to see its dependency problems"))
            }
            Self::Dep(_) | Self::NotReady(NotReady::NotTodo(_)) | Self::Config(_) | Self::Io(_) => {
                None
            }
        }
    }

    #[must_use]
    pub fn to_json(&self) -> ErrorJson {
        ErrorJson {
            code: self.code(),
            message: self.to_string(),
            hint: self.hint(),
        }
    }
}

impl ErrorCode {
    /// A stable exit status per code. 1 is left for `tk lint` finding problems, 2 for usage errors.
    #[must_use]
    pub const fn exit(self) -> u8 {
        match self {
            Self::Invalid => 2,
            Self::NotFound => 3,
            Self::InvalidTransition => 4,
            Self::Conflict => 5,
            Self::Dep => 6,
            Self::NotReady => 7,
            Self::InvalidFile => 9,
            Self::UnknownProject => 10,
            Self::NoProject => 11,
            Self::Config => 12,
            Self::Internal => 13,
            Self::Io => 14,
            Self::Claimed => 15,
            Self::BadRef => 16,
        }
    }
}

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound(id) => Self::NotFound(id),
            StoreError::Conflict(c) => Self::Conflict(c),
            StoreError::Invalid(file) => Self::InvalidFile(file),
            StoreError::Io(e) => Self::Io(e),
            e @ (StoreError::NoProject(_)
            | StoreError::ProjectExists(_)
            | StoreError::Format { .. }
            | StoreError::Config { .. }
            | StoreError::WrongProject { .. }) => Self::Config(e.to_string()),
            e @ (StoreError::Exists(_) | StoreError::Unreadable(_)) => {
                Self::Internal(e.to_string())
            }
        }
    }
}

impl From<AcquireError> for AppError {
    fn from(e: AcquireError) -> Self {
        match e {
            AcquireError::Claimed(c) => Self::Claimed(c),
            AcquireError::Store(e) => e.into(),
        }
    }
}

impl From<UpdateError<AppError>> for AppError {
    fn from(e: UpdateError<AppError>) -> Self {
        match e {
            UpdateError::Rejected(e) => e,
            e @ UpdateError::IdChanged => Self::Internal(e.to_string()),
        }
    }
}
