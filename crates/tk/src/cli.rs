//! The command line. Every value is parsed into a domain type here, at the edge,
//! so bad input is rejected before `execute` runs, with suggestions.

use clap::builder::{PossibleValuesParser, TypedValueParser as _};
use clap::{Args, Parser, Subcommand};
use strum::VariantNames as _;
use tk_app::FieldChange;
use tk_core::{
    Actor, Explain as _, Kind, Label, NonEmpty, Priority, ProjectKey, StateName, TicketId, Title,
};
use tk_store::VersionHex;

#[derive(Parser, Debug)]
#[command(
    name = "tk",
    version,
    about = "Tickets as Markdown files in git: a TUI for you, a CLI for agents"
)]
pub struct Cli {
    /// Print JSON instead of text
    #[arg(long, global = true)]
    pub json: bool,
    /// Who is acting, like agent:claude [default: the workspace's actor, else human:$USER]
    #[arg(long = "as", global = true, env = "TK_ACTOR", value_name = "ACTOR", value_parser = parsed::<Actor>)]
    pub actor: Option<Actor>,
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Create .tickets/ in this directory and add it to the workspace
    Init {
        #[arg(long, value_parser = project_key)]
        key: ProjectKey,
        /// A display name [default: the key]
        #[arg(long)]
        name: Option<String>,
    },
    #[command(flatten)]
    Write(WriteCmd),
    /// List tickets, hiding done ones
    Ls {
        /// Include done tickets
        #[arg(long)]
        all: bool,
        #[arg(long = "status", value_parser = PossibleValuesParser::new(StateName::VARIANTS).try_map(|s| s.parse::<StateName>()))]
        states: Vec<StateName>,
        #[arg(long = "label", value_parser = label)]
        labels: Vec<Label>,
        /// Every project in the workspace, not just this one
        #[arg(long, short)]
        workspace: bool,
        /// Only these fields, comma-separated (text columns keep this order); also trims `--json` output
        #[arg(long, value_delimiter = ',', value_parser = PossibleValuesParser::new(Field::VARIANTS).try_map(|s| s.parse::<Field>()))]
        fields: Vec<Field>,
    },
    /// One ticket, with its version and what it is waiting on
    Show {
        #[arg(value_parser = parsed::<TicketId>)]
        id: TicketId,
    },
    /// What a fresh session needs to pick a ticket up: its handoff, what was
    /// ruled out and decided, and the latest notes
    Brief {
        /// [default: the tickets you have in doing in this checkout]
        #[arg(value_parser = parsed::<TicketId>)]
        id: Option<TicketId>,
    },
    /// Validate every ticket file and the dependency graph
    Lint {
        #[arg(long, short)]
        workspace: bool,
    },
    /// Run JSON commands from stdin, one per line
    Apply,
    /// Print the JSON Schema of the commands `tk apply` accepts
    Schema,
    /// Print the usage guide for agents, for AGENTS.md or a skill
    Guide,
    /// Serve the commands over MCP on stdio, acting as `--as` / `TK_ACTOR`
    Mcp,
    /// Print shell completions
    Completions { shell: clap_complete::Shell },
    /// Called by git to merge one ticket file (%O %A %B %P); see `tk guide`
    #[command(hide = true)]
    MergeDriver {
        base: String,
        /// Ours; the result is written here
        ours: String,
        theirs: String,
        path: Option<String>,
    },
}

/// The subcommands that change tickets: each becomes one `Command`.
#[derive(Subcommand, Debug)]
pub enum WriteCmd {
    /// Create a ticket
    New {
        #[arg(value_parser = title)]
        title: Title,
        #[arg(long, value_parser = PossibleValuesParser::new(Kind::VARIANTS).try_map(|s| s.parse::<Kind>()))]
        kind: Option<Kind>,
        #[arg(long, value_parser = PossibleValuesParser::new(Priority::VARIANTS).try_map(|s| s.parse::<Priority>()))]
        priority: Option<Priority>,
        #[arg(long = "label", value_parser = label)]
        labels: Vec<Label>,
        #[arg(long = "dep", value_parser = parsed::<TicketId>)]
        deps: Vec<TicketId>,
        /// The Markdown body; `-` reads it from stdin
        #[arg(long)]
        body: Option<String>,
        /// [default: the project of the current directory]
        #[arg(long, value_parser = project_key)]
        project: Option<ProjectKey>,
    },
    /// The top ready ticket
    Next {
        /// Also start it
        #[arg(long)]
        claim: bool,
        /// Print its brief instead of one line
        #[arg(long)]
        brief: bool,
        #[arg(long, value_parser = project_key)]
        project: Option<ProjectKey>,
    },
    /// todo -> doing; only when every dependency is done
    Start(Target),
    /// todo or doing -> blocked
    Block {
        #[command(flatten)]
        target: Target,
        #[arg(long, value_parser = non_empty)]
        reason: NonEmpty,
    },
    /// blocked -> todo
    Unblock(Target),
    /// doing -> todo
    Release(Target),
    /// Finish a ticket
    Done {
        #[command(flatten)]
        target: Target,
        #[arg(long, conflicts_with = "duplicate_of")]
        wont_do: bool,
        #[arg(long, value_parser = parsed::<TicketId>)]
        duplicate_of: Option<TicketId>,
    },
    /// done -> todo
    Reopen(Target),
    /// Change fields
    Edit(EditArgs),
    /// Append a line to the ticket's ## Log
    Log {
        #[arg(value_parser = parsed::<TicketId>)]
        id: TicketId,
        /// One line of text; `-` reads it from stdin
        text: String,
        /// Something tried that didn't work, so the next session skips it
        #[arg(long, conflicts_with = "decided")]
        ruled_out: bool,
        /// A choice made, so the next session doesn't reopen it
        #[arg(long)]
        decided: bool,
    },
    /// End a session: log what's next, what was ruled out and decided, in one write.
    /// The ticket stays yours unless you --release or --block it
    Handoff(HandoffArgs),
    /// Add or remove a dependency
    #[command(subcommand)]
    Dep(DepCmd),
}

#[derive(Args, Debug)]
pub struct HandoffArgs {
    #[command(flatten)]
    pub target: Target,
    /// What the next session should do; repeat for several steps
    #[arg(long = "next", required = true, value_parser = non_empty)]
    pub next: Vec<NonEmpty>,
    /// Something tried that didn't work; repeatable
    #[arg(long = "ruled-out", value_parser = non_empty)]
    pub ruled_out: Vec<NonEmpty>,
    /// A choice made; repeatable
    #[arg(long = "decided", value_parser = non_empty)]
    pub decided: Vec<NonEmpty>,
    /// Put it back to todo for anyone
    #[arg(long, conflicts_with = "block")]
    pub release: bool,
    /// Block it, with this reason
    #[arg(long, value_parser = non_empty)]
    pub block: Option<NonEmpty>,
}

#[derive(Args, Debug)]
pub struct EditArgs {
    #[command(flatten)]
    pub target: Target,
    #[arg(long, value_parser = title)]
    pub title: Option<Title>,
    #[arg(long, value_parser = PossibleValuesParser::new(Kind::VARIANTS).try_map(|s| s.parse::<Kind>()))]
    pub kind: Option<Kind>,
    #[arg(long, value_parser = PossibleValuesParser::new(Priority::VARIANTS).try_map(|s| s.parse::<Priority>()))]
    pub priority: Option<Priority>,
    #[arg(long, value_parser = label)]
    pub add_label: Vec<Label>,
    #[arg(long, value_parser = label)]
    pub rm_label: Vec<Label>,
}

impl EditArgs {
    /// The ticket, and every flag given as a field change, in a fixed order.
    pub fn into_parts(self) -> (Target, Vec<FieldChange>) {
        let Self {
            target,
            title,
            kind,
            priority,
            add_label,
            rm_label,
        } = self;
        let changes = title
            .map(FieldChange::Title)
            .into_iter()
            .chain(kind.map(FieldChange::Kind))
            .chain(priority.map(FieldChange::Priority))
            .chain(add_label.into_iter().map(FieldChange::AddLabel))
            .chain(rm_label.into_iter().map(FieldChange::RemoveLabel))
            .collect();
        (target, changes)
    }
}

#[derive(Subcommand, Debug)]
pub enum DepCmd {
    /// Make a ticket depend on another; refused if it would create a cycle
    Add(DepArgs),
    /// Remove a dependency
    Rm(DepArgs),
}

#[derive(Args, Debug)]
pub struct DepArgs {
    #[command(flatten)]
    pub target: Target,
    #[arg(value_parser = parsed::<TicketId>)]
    pub dep: TicketId,
}

/// A field `tk ls --fields` can show. The names are the JSON keys, plus `state`,
/// the status name alone.
#[derive(
    Clone, Copy, PartialEq, Eq, Debug, strum::EnumString, strum::VariantNames, strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum Field {
    Id,
    State,
    Priority,
    Kind,
    Title,
    Labels,
    DependsOn,
    WaitingOn,
    Created,
    Status,
    Version,
    Body,
}

/// A ticket to change, and optionally the version the change is based on.
#[derive(Args, Debug)]
pub struct Target {
    #[arg(value_parser = parsed::<TicketId>)]
    pub id: TicketId,
    /// Refuse if the ticket changed since this version (from `tk show`)
    #[arg(long, value_parser = parsed::<VersionHex>)]
    pub if_version: Option<VersionHex>,
}

fn parsed<T: std::str::FromStr>(s: &str) -> Result<T, T::Err> {
    s.parse()
}

fn title(s: &str) -> Result<Title, &'static str> {
    Title::try_new(s.to_owned()).map_err(|e| e.explain())
}

fn label(s: &str) -> Result<Label, &'static str> {
    Label::try_new(s.to_owned()).map_err(|e| e.explain())
}

fn project_key(s: &str) -> Result<ProjectKey, &'static str> {
    ProjectKey::try_new(s.to_owned()).map_err(|e| e.explain())
}

fn non_empty(s: &str) -> Result<NonEmpty, &'static str> {
    NonEmpty::try_new(s.to_owned()).map_err(|e| e.explain())
}
