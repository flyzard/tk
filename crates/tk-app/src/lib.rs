//! One `Command` enum and one `execute` function, shared by the CLI, the TUI and later MCP.

mod brief;
mod command;
mod ctx;
mod edit_file;
mod error;
mod execute;
mod query;
mod report;

pub use brief::{Brief, Capped, Item, LastHandoff, Waiting, brief, my_briefs};
pub use command::{Command, FieldChange, schema};
pub use ctx::Ctx;
pub use edit_file::{edit_command, read_file};
pub use error::{AppError, ErrorCode, ErrorJson};
pub use execute::{Outcome, execute};
pub use query::{
    LintReport, ListFilter, Listing, Scope, Survey, TicketJson, lint, list, show, survey,
};
pub use report::{Finding, render_invalid};
