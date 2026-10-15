//! The terminal UI: an Elm-style loop. `update` is pure; this runtime is the
//! only part that touches the terminal, the disk, the clock or your editor.
//!
//! Terminal events are read on this thread, not a separate one, so nothing
//! competes with your editor for keystrokes while it is open. File changes
//! arrive from notify over a channel.

mod model;
mod view;

use std::collections::VecDeque;
use std::process;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use crossterm::event;
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use ratatui::DefaultTerminal;
use tk_app::{AppError, Ctx, Scope, edit_command, execute, read_file};
use tk_core::{TicketId, Timestamp};

use self::model::{Data, Draft, Effect, Model, Msg, update};

/// How long to wait for a key before checking for file changes.
const TICK: Duration = Duration::from_millis(100);

/// Runs the TUI until `q`.
///
/// # Errors
/// Terminal or watcher setup failing. Errors from commands show inside the TUI.
pub fn run(mut ctx: Ctx, editor: &[String], clock: fn() -> Timestamp) -> Result<(), AppError> {
    let (tx, changes) = mpsc::channel();
    let _watcher = watch(&ctx, move |result: DebounceEventResult| {
        let touched_ticket = result.is_ok_and(|events| {
            events.iter().any(|e| {
                e.event
                    .paths
                    .iter()
                    .any(|p| p.extension().is_some_and(|ext| ext == "md"))
            })
        });
        if touched_ticket && tx.send(()).is_err() {
            // The TUI has quit; nothing is listening.
        }
    })?;
    let mut model = Model::new(ctx.current.clone());
    model.notice = ctx.warnings.first().map(|w| model::Notice {
        text: format!("warning: {w}"),
        error: true,
    });
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal, &mut ctx, &mut model, &changes, editor, clock);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    ctx: &mut Ctx,
    model: &mut Model,
    changes: &Receiver<()>,
    editor: &[String],
    clock: fn() -> Timestamp,
) -> Result<(), AppError> {
    let mut effects: VecDeque<Effect> = VecDeque::from([Effect::Reload]);
    // Draw only after something changed: input, a reload or a command.
    let mut dirty = true;
    loop {
        while let Some(effect) = effects.pop_front() {
            let msg = match effect {
                Effect::Quit => return Ok(()),
                Effect::Reload => Msg::Loaded(load(ctx)),
                Effect::Run(command) => {
                    ctx.now = clock();
                    Msg::Finished(execute(command, ctx))
                }
                Effect::Edit { id, draft } => edit(terminal, ctx, &id, draft, editor, clock),
            };
            effects.extend(update(model, msg));
            for warning in ctx.warnings.drain(..) {
                effects.extend(update(model, Msg::Warning(warning)));
            }
            dirty = true;
        }
        if dirty {
            terminal.draw(|frame| view::render(model, frame))?;
            dirty = false;
        }
        if event::poll(TICK)? {
            effects.extend(update(model, Msg::Input(event::read()?)));
            dirty = true;
        }
        // Many writes land as one burst; one reload covers them all.
        if changes.try_iter().count() > 0 {
            effects.extend(update(model, Msg::FilesChanged));
        }
    }
}

fn load(ctx: &Ctx) -> Result<Data, AppError> {
    let survey = tk_app::survey(ctx, Scope::All)?;
    Ok(Data {
        findings: survey.report.findings(),
        tickets: survey.tickets,
        ready: survey.ready,
    })
}

/// Copies the ticket to a temp file, hands the terminal to your editor, and
/// saves the result only if it parses and nobody changed the ticket meanwhile.
/// The real file is never left invalid.
fn edit(
    terminal: &mut DefaultTerminal,
    ctx: &mut Ctx,
    id: &TicketId,
    draft: Option<Draft>,
    editor: &[String],
    clock: fn() -> Timestamp,
) -> Msg {
    let (base, original, text) = match draft {
        Some(Draft {
            base,
            original,
            text,
        }) => (base, original, text),
        None => match read_file(ctx, id) {
            Ok((text, base)) => (base, text.clone(), text),
            Err(e) => return Msg::Finished(Err(e)),
        },
    };
    let edited = match run_editor(terminal, editor, id, &text) {
        Ok(edited) => edited,
        Err(e) => return Msg::Finished(Err(e)),
    };
    match edit_command(id, base, &original, &edited) {
        Ok(Some(command)) => {
            ctx.now = clock();
            Msg::Finished(execute(command, ctx))
        }
        Ok(None) => Msg::Info(format!("{id}: no changes")),
        Err(error) => Msg::EditInvalid {
            id: id.clone(),
            draft: Draft {
                base,
                original,
                text: edited,
            },
            error,
        },
    }
}

fn run_editor(
    terminal: &mut DefaultTerminal,
    editor: &[String],
    id: &TicketId,
    text: &str,
) -> Result<String, AppError> {
    let file = tempfile::Builder::new()
        .prefix(&format!("{id}-"))
        .suffix(".md")
        .tempfile()?;
    std::fs::write(file.path(), text)?;
    let Some((program, args)) = editor.split_first() else {
        return Err(AppError::Config(String::from(
            "no editor: set `editor` in workspace.toml, or $VISUAL / $EDITOR",
        )));
    };
    ratatui::restore();
    let status = process::Command::new(program)
        .args(args)
        .arg(file.path())
        .status();
    *terminal = ratatui::try_init()?;
    terminal.clear()?;
    let status = status.map_err(|e| AppError::Config(format!("couldn't run {program}: {e}")))?;
    if !status.success() {
        return Err(AppError::Config(format!(
            "{program} exited with {status}; nothing saved"
        )));
    }
    Ok(std::fs::read_to_string(file.path())?)
}

/// Watches every project's `.tickets/`. The full debouncer follows renames,
/// which is how every atomic write lands, so agents' changes show as they work.
fn watch(
    ctx: &Ctx,
    on_change: impl FnMut(DebounceEventResult) + Send + 'static,
) -> Result<Debouncer<RecommendedWatcher, RecommendedCache>, AppError> {
    let config = |e: notify_debouncer_full::notify::Error| {
        AppError::Config(format!("can't watch ticket files: {e}"))
    };
    let mut debouncer =
        new_debouncer(Duration::from_millis(200), None, on_change).map_err(config)?;
    for store in ctx.stores.values() {
        debouncer
            .watch(store.tickets_dir(), RecursiveMode::NonRecursive)
            .map_err(config)?;
    }
    Ok(debouncer)
}
