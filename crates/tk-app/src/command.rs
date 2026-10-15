use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::Deserialize;
use tk_core::{
    Body, Kind, Label, Labels, NonEmpty, Priority, ProjectKey, Resolution, TicketId, Title,
};
use tk_store::VersionHex;

/// Everything that changes tickets. Every interface parses its input into one of
/// these; [`execute`](crate::execute) is the only thing that runs them. A bad
/// JSON line from `tk apply` is rejected while it is parsed, naming the field.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    New {
        /// Defaults to the project of the current directory.
        #[serde(default)]
        project: Option<ProjectKey>,
        title: Title,
        #[serde(default)]
        kind: Kind,
        #[serde(default)]
        priority: Priority,
        #[serde(default)]
        labels: Labels,
        #[serde(default)]
        depends_on: BTreeSet<TicketId>,
        #[serde(default)]
        body: Body,
    },
    Edit {
        id: TicketId,
        #[schemars(with = "Option<String>")]
        if_version: Option<VersionHex>,
        changes: Vec<FieldChange>,
    },
    Start {
        id: TicketId,
        #[schemars(with = "Option<String>")]
        if_version: Option<VersionHex>,
    },
    Block {
        id: TicketId,
        #[schemars(with = "Option<String>")]
        if_version: Option<VersionHex>,
        reason: NonEmpty,
    },
    Unblock {
        id: TicketId,
        #[schemars(with = "Option<String>")]
        if_version: Option<VersionHex>,
    },
    Release {
        id: TicketId,
        #[schemars(with = "Option<String>")]
        if_version: Option<VersionHex>,
    },
    Finish {
        id: TicketId,
        #[schemars(with = "Option<String>")]
        if_version: Option<VersionHex>,
        #[serde(default = "completed")]
        resolution: Resolution,
    },
    Reopen {
        id: TicketId,
        #[schemars(with = "Option<String>")]
        if_version: Option<VersionHex>,
    },
    /// Appends a line to the ticket's `## Log`. Appends don't conflict, so there is no version check.
    Log { id: TicketId, text: NonEmpty },
    DepAdd {
        id: TicketId,
        dep: TicketId,
        #[schemars(with = "Option<String>")]
        if_version: Option<VersionHex>,
    },
    DepRemove {
        id: TicketId,
        dep: TicketId,
        #[schemars(with = "Option<String>")]
        if_version: Option<VersionHex>,
    },
    /// The top ready ticket; with `claim`, also starts it.
    Next {
        #[serde(default)]
        project: Option<ProjectKey>,
        #[serde(default)]
        claim: bool,
    },
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldChange {
    Title(Title),
    Kind(Kind),
    Priority(Priority),
    AddLabel(Label),
    RemoveLabel(Label),
    Body(Body),
}

fn completed() -> Resolution {
    Resolution::Completed
}

/// The JSON Schema of [`Command`], for `tk schema`.
#[must_use]
pub fn schema() -> schemars::Schema {
    schemars::schema_for!(Command)
}
