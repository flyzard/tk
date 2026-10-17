//! The tk binary: clap CLI for agents, ratatui TUI for you (phase 4).
//!
//! This is the only place that reads the clock, random bytes, the environment
//! and stdin. Everything below it takes them as arguments.

mod cli;
mod mcp;
mod output;
mod tui;

use std::io::{self, IsTerminal as _, Read as _};
use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::{CommandFactory as _, Parser as _};
use tk_app::{AppError, Command, Ctx, ListFilter, Outcome, Scope, execute};
use tk_core::{
    Actor, AfterHandoff, Body, Explain as _, Handoff, LogKind, Name, NextSteps, NonEmpty,
    ProjectKey, Resolution, TicketId, Timestamp,
};
use tk_store::{Store, Workspace};

use crate::cli::{Cli, Cmd, DepCmd, HandoffArgs, RefArgs, Target, WriteCmd};
use crate::output::{Out, json_line, print};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let out = Out {
        // `tk apply` speaks JSON both ways, errors included.
        json: cli.json || matches!(cli.cmd, Some(Cmd::Apply)),
        styled: io::stderr().is_terminal(),
    };
    match run(cli, &out) {
        Ok(code) => code,
        Err(e) => out.error(&e),
    }
}

fn run(cli: Cli, out: &Out) -> Result<ExitCode, AppError> {
    let Cli {
        json: _,
        actor,
        cmd,
    } = cli;
    let Some(cmd) = cmd else {
        if !io::stdout().is_terminal() {
            Cli::command().print_help()?;
            return Ok(ExitCode::SUCCESS);
        }
        let ctx = open(actor)?;
        tui::run(ctx, &editor(&Workspace::load()?), clock)?;
        return Ok(ExitCode::SUCCESS);
    };
    match cmd {
        Cmd::Guide => print(include_str!("guide.md").trim_end()).map(|()| ExitCode::SUCCESS),
        Cmd::Schema => {
            let schema = serde_json::to_string_pretty(&tk_app::schema())
                .map_err(|e| AppError::Internal(e.to_string()))?;
            print(&schema).map(|()| ExitCode::SUCCESS)
        }
        Cmd::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "tk", &mut io::stdout());
            Ok(ExitCode::SUCCESS)
        }
        Cmd::MergeDriver {
            base,
            ours,
            theirs,
            path: _,
        } => merge_driver(&base, &ours, &theirs),
        Cmd::Init { key, name } => init(key, name, out),
        Cmd::Ls {
            all,
            states,
            labels,
            workspace,
            fields,
        } => {
            let filter = ListFilter {
                scope: Scope::from_workspace_flag(workspace),
                all,
                states,
                labels,
            };
            out.listing(&tk_app::list(&open(actor)?, &filter)?, &fields)
                .map(|()| ExitCode::SUCCESS)
        }
        Cmd::Show { id } => out
            .ticket(&tk_app::show(&open(actor)?, &id)?)
            .map(|()| ExitCode::SUCCESS),
        Cmd::Brief { id } => {
            let ctx = open(actor)?;
            match id {
                Some(id) => out.brief(&tk_app::brief(&ctx, &id)?),
                None => out.briefs(&tk_app::my_briefs(&ctx)?, &ctx.actor),
            }
            .map(|()| ExitCode::SUCCESS)
        }
        Cmd::Lint { workspace } => out.lint(&tk_app::lint(
            &open(actor)?,
            Scope::from_workspace_flag(workspace),
        )?),
        Cmd::Apply => apply(&mut open(actor)?),
        Cmd::Mcp => mcp::serve(open(actor)?, clock).map(|()| ExitCode::SUCCESS),
        Cmd::Write(WriteCmd::Next {
            claim,
            project,
            brief: true,
        }) => {
            let mut ctx = open(actor)?;
            let outcome = execute(Command::Next { project, claim }, &mut ctx)?;
            warn(&mut ctx);
            match outcome {
                Outcome::Updated { id, .. } | Outcome::Next { id, .. } => {
                    out.brief(&tk_app::brief(&ctx, &id)?)
                }
                Outcome::Created { .. } | Outcome::NothingReady => out.outcome(&outcome),
            }
            .map(|()| ExitCode::SUCCESS)
        }
        Cmd::Write(write) => {
            let command = command(write)?;
            let mut ctx = open(actor)?;
            let outcome = execute(command, &mut ctx)?;
            warn(&mut ctx);
            out.outcome(&outcome).map(|()| ExitCode::SUCCESS)
        }
    }
}

/// Turns a writing subcommand into the `Command` that `execute` runs.
fn command(cmd: WriteCmd) -> Result<Command, AppError> {
    Ok(match cmd {
        WriteCmd::New {
            title,
            kind,
            priority,
            labels,
            deps,
            body,
            refs,
            project,
        } => Command::New {
            project,
            title,
            kind: kind.unwrap_or_default(),
            priority: priority.unwrap_or_default(),
            labels: labels.into_iter().collect(),
            depends_on: deps.into_iter().collect(),
            body: Body::new(
                body.map(|b| text_or_stdin(&b))
                    .transpose()?
                    .unwrap_or_default(),
            ),
            refs,
        },
        WriteCmd::Next {
            claim,
            project,
            brief: _,
        } => Command::Next { project, claim },
        WriteCmd::Start(t) => Command::Start {
            id: t.id,
            if_version: t.if_version,
        },
        WriteCmd::Block { target: t, reason } => Command::Block {
            id: t.id,
            if_version: t.if_version,
            reason,
        },
        WriteCmd::Unblock(t) => Command::Unblock {
            id: t.id,
            if_version: t.if_version,
        },
        WriteCmd::Release(t) => Command::Release {
            id: t.id,
            if_version: t.if_version,
        },
        WriteCmd::Reopen(t) => Command::Reopen {
            id: t.id,
            if_version: t.if_version,
        },
        WriteCmd::Done {
            target: t,
            wont_do,
            duplicate_of,
        } => Command::Finish {
            id: t.id,
            if_version: t.if_version,
            resolution: resolution(wont_do, duplicate_of),
        },
        WriteCmd::Edit(args) => {
            let (Target { id, if_version }, changes) = args.into_parts();
            if changes.is_empty() {
                return Err(AppError::Invalid(String::from(
                    "nothing to change; pass --title, --priority, --add-label, ...",
                )));
            }
            Command::Edit {
                id,
                if_version,
                changes,
            }
        }
        WriteCmd::Log {
            id,
            text,
            ruled_out,
            decided,
        } => log(id, &text, ruled_out, decided)?,
        WriteCmd::Ref(args) => reference(args),
        WriteCmd::Handoff(args) => handoff(args)?,
        WriteCmd::Dep(DepCmd::Add(a)) => Command::DepAdd {
            id: a.target.id,
            dep: a.dep,
            if_version: a.target.if_version,
        },
        WriteCmd::Dep(DepCmd::Rm(a)) => Command::DepRemove {
            id: a.target.id,
            dep: a.dep,
            if_version: a.target.if_version,
        },
    })
}

/// A log line, its text read from stdin for `-`.
fn log(id: TicketId, text: &str, ruled_out: bool, decided: bool) -> Result<Command, AppError> {
    let text = text_or_stdin(text)?;
    let text = NonEmpty::try_new(text.trim_end().to_owned())
        .map_err(|e| AppError::Invalid(format!("log text: {}", e.explain())))?;
    let kind = match (ruled_out, decided) {
        (true, _) => LogKind::RuledOut,
        (false, true) => LogKind::Decided,
        (false, false) => LogKind::Note,
    };
    Ok(Command::Log { id, text, kind })
}

/// A ref, or with `--drop` an unref.
fn reference(args: RefArgs) -> Command {
    let RefArgs {
        id,
        target,
        why,
        drop,
    } = args;
    if drop {
        Command::Unref { id, target, why }
    } else {
        Command::Ref { id, target, why }
    }
}

/// A handoff, with its next steps checked: there must be at least one.
fn handoff(args: HandoffArgs) -> Result<Command, AppError> {
    let HandoffArgs {
        target: Target { id, if_version },
        next,
        ruled_out,
        decided,
        refs,
        release,
        block,
    } = args;
    Ok(Command::Handoff {
        id,
        if_version,
        handoff: Handoff {
            next: NextSteps::try_new(next)
                .map_err(|e| AppError::Invalid(e.explain().to_owned()))?,
            ruled_out,
            decided,
            refs,
        },
        then: match (release, block) {
            (true, _) => AfterHandoff::Release,
            (false, Some(reason)) => AfterHandoff::Block { reason },
            (false, None) => AfterHandoff::Keep,
        },
    })
}

/// Creates `.tickets/` here and adds this repository to the workspace.
fn init(key: ProjectKey, name: Option<String>, out: &Out) -> Result<ExitCode, AppError> {
    let mut workspace = Workspace::load()?;
    let here = cwd()?;
    let name = name.unwrap_or_else(|| key.to_string());
    let store = Store::init(&here, key, name)?;
    workspace.add_project(&here)?;
    out.initialized(&store, &workspace)
        .map(|()| ExitCode::SUCCESS)
}

/// Merges one ticket file for git: writes the result over `ours` and exits 1
/// when a field needs a person, leaving conflict markers on just that line.
fn merge_driver(base: &str, ours: &str, theirs: &str) -> Result<ExitCode, AppError> {
    let read = |path: &str| std::fs::read_to_string(path);
    let (text, code) = match tk_store::merge(&read(base)?, &read(ours)?, &read(theirs)?) {
        tk_store::Merged::Clean(text) => (text, ExitCode::SUCCESS),
        tk_store::Merged::Conflicted(text) => (text, ExitCode::FAILURE),
    };
    std::fs::write(ours, text)?;
    Ok(code)
}

/// Runs one JSON command per stdin line and prints one JSON outcome per line.
/// Stops at the first failure, since later lines may depend on it.
fn apply(ctx: &mut Ctx) -> Result<ExitCode, AppError> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    for (n, line) in input
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let command: Command = serde_json::from_str(line)
            .map_err(|e| AppError::Invalid(format!("line {}: {e}", n + 1)))?;
        json_line(&execute(command, ctx)?)?;
        warn(ctx);
    }
    Ok(ExitCode::SUCCESS)
}

fn resolution(wont_do: bool, duplicate_of: Option<TicketId>) -> Resolution {
    match (wont_do, duplicate_of) {
        (_, Some(of)) => Resolution::Duplicate(of),
        (true, None) => Resolution::WontDo,
        (false, None) => Resolution::Completed,
    }
}

/// `-` means stdin. Reading only on request keeps agents' shells, whose stdin may be an open pipe, from hanging.
fn text_or_stdin(arg: &str) -> Result<String, AppError> {
    if arg != "-" {
        return Ok(arg.to_owned());
    }
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    Ok(text)
}

/// The projects around the current directory and in the workspace, acting as
/// `--as` / `TK_ACTOR`, else the workspace's actor, else `human:$USER`.
fn open(actor: Option<Actor>) -> Result<Ctx, AppError> {
    let workspace = Workspace::load()?;
    let actor = match actor.or_else(|| workspace.actor.clone()) {
        Some(actor) => actor,
        None => user_actor()?,
    };
    let mut ctx = Ctx::open(&workspace, &cwd()?, actor, clock(), Box::new(entropy))?;
    warn(&mut ctx);
    Ok(ctx)
}

/// Prints and clears the warnings collected so far.
fn warn(ctx: &mut Ctx) {
    for warning in ctx.warnings.drain(..) {
        eprintln!("warning: {warning}");
    }
}

/// The workspace's `editor` array, else `$VISUAL` or `$EDITOR` split on spaces, else `vi`.
fn editor(workspace: &Workspace) -> Vec<String> {
    if !workspace.editor.is_empty() {
        return workspace.editor.clone();
    }
    ["VISUAL", "EDITOR"]
        .iter()
        .find_map(|var| std::env::var(var).ok().filter(|v| !v.trim().is_empty()))
        .map_or_else(
            || vec![String::from("vi")],
            |v| v.split_whitespace().map(String::from).collect(),
        )
}

fn cwd() -> Result<Utf8PathBuf, AppError> {
    Utf8PathBuf::try_from(std::env::current_dir()?).map_err(|e| AppError::Config(e.to_string()))
}

fn user_actor() -> Result<Actor, AppError> {
    std::env::var("USER")
        .ok()
        .and_then(|user| Name::try_new(user).ok())
        .map(Actor::Human)
        .ok_or_else(|| {
            AppError::Config(String::from(
                "no actor: pass --as, set TK_ACTOR, or set `actor` in workspace.toml",
            ))
        })
}

/// The one clock read. Rounded to seconds so files stay readable.
fn clock() -> Timestamp {
    #[expect(
        clippy::disallowed_methods,
        reason = "the one clock read; everything else takes `now` as an argument"
    )]
    let now = Timestamp::now();
    now.round(jiff::Unit::Second).unwrap_or(now)
}

/// The one source of random bytes, for ticket IDs.
fn entropy() -> io::Result<[u8; 4]> {
    let mut bytes = [0; 4];
    #[expect(
        clippy::disallowed_methods,
        reason = "the one entropy source; tk-core takes random bytes as an argument"
    )]
    getrandom::fill(&mut bytes).map_err(io::Error::from)?;
    Ok(bytes)
}
