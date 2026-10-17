//! The ticket file format and reliable writes: transactions, versions and atomic renames.

mod error;
pub mod format;
mod lease;
mod log;
mod merge;
mod store;
mod version;
mod workspace;

pub use error::{Conflict, InvalidFile, ParseError, StoreError, UpdateError};
pub use format::TicketDoc;
pub use lease::{AcquireError, Claimed, Lease, Leases};
pub use log::{LogEntry, LogLine, append_entries, log_lines, markdown_headings, without_log};
pub use merge::{Merged, merge};
pub use store::{Edited, Loaded, Saved, Snapshot, Store, Tx, find_repo};
pub use version::{Version, VersionHex, VersionHexError};
pub use workspace::Workspace;
