//! The TUI's state and its pure `update`: each message becomes a new model plus
//! a list of effects. Only the runtime touches disk, the clock or the terminal.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::BTreeSet;

use tk_app::{AppError, Command, ErrorCode, FieldChange, Finding, Outcome, Scope, TicketJson};
use tk_core::{
    Explain as _, NonEmpty, Priority, ProjectKey, Resolution, StateName, TicketId, Title,
};
use tk_store::{Version, VersionHex};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler as _;

/// What was loaded from every project.
#[derive(Debug, Default)]
pub struct Data {
    pub tickets: Vec<TicketJson>,
    /// Files that don't parse and dependency problems, for the Needs fixing view.
    pub findings: Vec<Finding>,
    /// What `tk next` could start, from the same graph it uses.
    pub ready: BTreeSet<TicketId>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Board,
    List,
    Ready,
    NeedsFixing,
}

impl View {
    pub const ALL: [Self; 4] = [Self::Board, Self::List, Self::Ready, Self::NeedsFixing];
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sort {
    Priority,
    Created,
    Title,
    State,
}

/// The board's columns, left to right.
pub const COLUMNS: [StateName; 4] = [
    StateName::Todo,
    StateName::Doing,
    StateName::Blocked,
    StateName::Done,
];

/// State that exists only in one mode lives in that variant: a filter input
/// exists only while filtering, and a confirm prompt knows exactly what it runs.
#[derive(Debug)]
pub enum Mode {
    Normal,
    Detail {
        id: TicketId,
        scroll: u16,
    },
    Filtering(Input),
    Prompt(Prompt, Input),
    /// `y` runs exactly this command.
    Confirm {
        question: String,
        command: Command,
    },
    /// The editor produced text tk can't save; `y` reopens it with that text.
    Reedit {
        id: TicketId,
        draft: Draft,
        error: String,
    },
    Help,
}

#[derive(Debug)]
pub enum Prompt {
    NewTitle {
        project: ProjectKey,
    },
    BlockReason {
        id: TicketId,
        if_version: VersionHex,
    },
}

/// An edit in progress: the file as read, its version, and the edited text.
#[derive(Clone, Debug)]
pub struct Draft {
    pub base: Version,
    pub original: String,
    pub text: String,
}

#[derive(Debug)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

#[derive(Debug)]
pub enum Msg {
    Input(Event),
    FilesChanged,
    Loaded(Result<Data, AppError>),
    Finished(Result<Outcome, AppError>),
    EditInvalid {
        id: TicketId,
        draft: Draft,
        error: String,
    },
    Info(String),
    /// Something went wrong that didn't undo the change.
    Warning(String),
}

#[derive(Debug)]
pub enum Effect {
    Run(Command),
    Edit { id: TicketId, draft: Option<Draft> },
    Reload,
    Quit,
}

#[derive(Debug)]
pub struct Model {
    pub data: Data,
    pub view: View,
    pub mode: Mode,
    /// By ID, never by index, so the cursor stays on its ticket when files reload.
    pub selected: Option<TicketId>,
    pub column: StateName,
    pub scope: Scope,
    pub current: Option<ProjectKey>,
    pub filter: String,
    pub sort: Sort,
    pub notice: Option<Notice>,
    /// A ticket created from the TUI opens in the editor for its body once it exists.
    edit_created: bool,
}

impl Model {
    pub fn new(current: Option<ProjectKey>) -> Self {
        let scope = if current.is_some() {
            Scope::Current
        } else {
            Scope::All
        };
        Self {
            data: Data::default(),
            view: View::Board,
            mode: Mode::Normal,
            selected: None,
            column: StateName::Todo,
            scope,
            current,
            filter: String::new(),
            sort: Sort::Priority,
            notice: None,
            edit_created: false,
        }
    }

    /// The tickets the current view shows, in its order.
    pub fn visible(&self) -> Vec<&TicketJson> {
        let in_scope = |t: &&TicketJson| match (self.scope, &self.current) {
            (Scope::Current, Some(key)) => t.id.project() == key,
            (Scope::Current, None) | (Scope::All, _) => true,
        };
        let mut rows: Vec<&TicketJson> = match self.view {
            View::Board | View::List => self.data.tickets.iter().filter(in_scope).collect(),
            // What `tk next` would pick, across all projects.
            View::Ready => self
                .data
                .tickets
                .iter()
                .filter(|t| self.data.ready.contains(&t.id))
                .collect(),
            View::NeedsFixing => Vec::new(),
        };
        match (self.view, self.sort) {
            (View::List, Sort::Created) => rows.sort_by_key(|t| t.created),
            (View::List, Sort::Title) => {
                rows.sort_by(|a, b| a.title.as_ref().cmp(b.title.as_ref()));
            }
            (View::List, Sort::State) => {
                rows.sort_by_key(|t| (t.status.name(), t.priority, t.created));
            }
            (View::List, Sort::Priority) | (View::Board | View::Ready | View::NeedsFixing, _) => {
                rows.sort_by_key(|t| (t.priority, t.created));
            }
        }
        if self.filter.trim().is_empty() {
            return rows;
        }
        let haystacks: Vec<String> = rows
            .iter()
            .map(|t| {
                format!(
                    "{} {} {}",
                    t.id,
                    t.title,
                    t.labels
                        .iter()
                        .map(AsRef::as_ref)
                        .collect::<Vec<&str>>()
                        .join(" ")
                )
            })
            .collect();
        let config = frizbee::Config {
            max_typos: Some(0),
            ..frizbee::Config::default()
        };
        let mut hits: Vec<usize> = frizbee::Matcher::from_query(&self.filter, &config)
            .match_list(&haystacks)
            .iter()
            .filter_map(|m| usize::try_from(m.index).ok())
            .collect();
        hits.sort_unstable();
        hits.into_iter()
            .filter_map(|i| rows.get(i).copied())
            .collect()
    }

    /// The board column of `state`, filtered like the rest of the view.
    pub fn column_rows(&self, state: StateName) -> Vec<&TicketJson> {
        self.visible()
            .into_iter()
            .filter(|t| t.status.name() == state)
            .collect()
    }

    pub fn selected_ticket(&self) -> Option<&TicketJson> {
        let id = self.selected.as_ref()?;
        self.data.tickets.iter().find(|t| &t.id == id)
    }

    /// The rows the cursor moves through: the focused board column, or the
    /// whole list. `None` in a view without tickets.
    fn rows(&self) -> Option<Vec<&TicketJson>> {
        match self.view {
            View::Board => Some(self.column_rows(self.column)),
            View::List | View::Ready => Some(self.visible()),
            View::NeedsFixing => None,
        }
    }

    /// Keeps the selection on a visible ticket after anything changes what is visible.
    fn settle(&mut self) {
        if self.view == View::Board
            && let Some(t) = self.selected_ticket()
            && self
                .column_rows(t.status.name())
                .iter()
                .any(|r| r.id == t.id)
        {
            self.column = t.status.name();
            return;
        }
        let Some(rows) = self.rows() else {
            return;
        };
        if !rows.iter().any(|t| Some(&t.id) == self.selected.as_ref()) {
            self.selected = rows.first().map(|t| t.id.clone());
        }
    }

    fn info(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            error: false,
        });
    }

    fn error(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            error: true,
        });
    }
}

/// Pure: no I/O and no clock. Every write becomes an `Effect::Run`, which goes
/// through the same `execute` as the CLI.
pub fn update(model: &mut Model, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => key_press(model, key),
        Msg::Input(_) => Vec::new(),
        Msg::FilesChanged => vec![Effect::Reload],
        Msg::Loaded(Ok(data)) => {
            model.data = data;
            model.settle();
            Vec::new()
        }
        Msg::Loaded(Err(e)) => {
            model.error(format!("couldn't load tickets: {e}"));
            Vec::new()
        }
        Msg::Finished(Ok(outcome)) => {
            match outcome {
                Outcome::Created { id, version: _ } => {
                    model.info(format!("created {id}"));
                    model.selected = Some(id.clone());
                    if std::mem::take(&mut model.edit_created) {
                        return vec![Effect::Reload, Effect::Edit { id, draft: None }];
                    }
                }
                Outcome::Updated {
                    id,
                    state,
                    version: _,
                } => model.info(format!("{id} {state}")),
                Outcome::Next {
                    id,
                    title: _,
                    priority: _,
                } => model.info(format!("next: {id}")),
                Outcome::NothingReady => model.info("nothing ready"),
            }
            vec![Effect::Reload]
        }
        Msg::Finished(Err(e)) => {
            model.edit_created = false;
            // The CLI's hints name commands to run; here a reload already happened.
            let reloaded = if e.code() == ErrorCode::Conflict {
                " (reloaded; try again)"
            } else {
                ""
            };
            model.error(format!("{e}{reloaded}"));
            vec![Effect::Reload]
        }
        Msg::EditInvalid { id, draft, error } => {
            model.mode = Mode::Reedit { id, draft, error };
            Vec::new()
        }
        Msg::Warning(text) => {
            model.error(format!("warning: {text}"));
            Vec::new()
        }
        Msg::Info(text) => {
            model.info(text);
            Vec::new()
        }
    }
}

/// One `match` per mode, so a key added to `Normal` can't silently act in `Filtering`.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "KeyCode is crossterm's input enum, not a domain enum: unbound keys do nothing"
)]
fn key_press(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return vec![Effect::Quit];
    }
    model.notice = None;
    match std::mem::replace(&mut model.mode, Mode::Normal) {
        Mode::Normal => normal_key(model, key),
        Mode::Help => Vec::new(),
        Mode::Detail { id, scroll } => detail_key(model, key, id, scroll),
        Mode::Filtering(input) => filter_key(model, key, input),
        Mode::Prompt(prompt, input) => prompt_key(model, key, prompt, input),
        Mode::Confirm { question, command } => match key.code {
            KeyCode::Char('y') => vec![Effect::Run(command)],
            KeyCode::Char('n') | KeyCode::Esc => Vec::new(),
            _ => {
                model.mode = Mode::Confirm { question, command };
                Vec::new()
            }
        },
        Mode::Reedit { id, draft, error } => match key.code {
            KeyCode::Char('y') => vec![Effect::Edit {
                id,
                draft: Some(draft),
            }],
            KeyCode::Char('n') | KeyCode::Esc => {
                model.info("edit discarded");
                Vec::new()
            }
            _ => {
                model.mode = Mode::Reedit { id, draft, error };
                Vec::new()
            }
        },
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "KeyCode is crossterm's input enum, not a domain enum: unbound keys do nothing"
)]
fn normal_key(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
    match key.code {
        KeyCode::Char('q') => return vec![Effect::Quit],
        KeyCode::Char('j') | KeyCode::Down => step(model, 1),
        KeyCode::Char('k') | KeyCode::Up => step(model, -1),
        KeyCode::Char('h') | KeyCode::Left => shift_column(model, -1),
        KeyCode::Char('l') | KeyCode::Right => shift_column(model, 1),
        KeyCode::Tab => switch_view(model, 1),
        KeyCode::BackTab => switch_view(model, -1),
        KeyCode::Char(c @ '1'..='4') => {
            let index = usize::from(u8::try_from(c).unwrap_or(b'1') - b'1');
            if let Some(&view) = View::ALL.get(index) {
                model.view = view;
                model.settle();
            }
        }
        KeyCode::Enter => {
            if let Some(id) = model.selected.clone() {
                model.mode = Mode::Detail { id, scroll: 0 };
            }
        }
        KeyCode::Char('/') => model.mode = Mode::Filtering(Input::new(model.filter.clone())),
        KeyCode::Esc => {
            model.filter.clear();
            model.settle();
        }
        KeyCode::Char('g') => {
            model.scope = match (model.scope, &model.current) {
                (Scope::Current, _) | (Scope::All, None) => Scope::All,
                (Scope::All, Some(_)) => Scope::Current,
            };
            model.settle();
        }
        KeyCode::Char('o') => {
            model.sort = match model.sort {
                Sort::Priority => Sort::Created,
                Sort::Created => Sort::Title,
                Sort::Title => Sort::State,
                Sort::State => Sort::Priority,
            };
        }
        KeyCode::Char('?') => model.mode = Mode::Help,
        KeyCode::Char('n') => {
            let project = model
                .current
                .clone()
                .or_else(|| model.selected.as_ref().map(|id| id.project().clone()));
            match project {
                Some(project) => {
                    model.mode = Mode::Prompt(Prompt::NewTitle { project }, Input::default());
                }
                None => model.error("no project here; run `tk init --key <key>`"),
            }
        }
        _ => return ticket_key(model, key),
    }
    Vec::new()
}

/// Keys that act on the selected ticket, in the list views and in Detail.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "KeyCode is crossterm's input enum, not a domain enum: unbound keys do nothing"
)]
fn ticket_key(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
    let Some(t) = model.selected_ticket() else {
        return Vec::new();
    };
    let id = t.id.clone();
    let version = VersionHex::from(t.version);
    let if_version = Some(version);
    let priority = t.priority;
    let title = t.title.clone();
    let command = match key.code {
        KeyCode::Char('e') => return vec![Effect::Edit { id, draft: None }],
        KeyCode::Char('s') => Command::Start { id, if_version },
        KeyCode::Char('u') => Command::Unblock { id, if_version },
        KeyCode::Char('x') => Command::Release { id, if_version },
        KeyCode::Char('r') => Command::Reopen { id, if_version },
        KeyCode::Char('p') => {
            let next = match priority {
                Priority::P0 => Priority::P1,
                Priority::P1 => Priority::P2,
                Priority::P2 => Priority::P3,
                Priority::P3 => Priority::P0,
            };
            Command::Edit {
                id,
                if_version,
                changes: vec![FieldChange::Priority(next)],
            }
        }
        KeyCode::Char('b') => {
            model.mode = Mode::Prompt(
                Prompt::BlockReason {
                    id,
                    if_version: version,
                },
                Input::default(),
            );
            return Vec::new();
        }
        KeyCode::Char('d') => {
            model.mode = Mode::Confirm {
                question: format!("Finish {id} \u{201c}{title}\u{201d}?"),
                command: Command::Finish {
                    id,
                    if_version,
                    resolution: Resolution::Completed,
                },
            };
            return Vec::new();
        }
        _ => return Vec::new(),
    };
    vec![Effect::Run(command)]
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "KeyCode is crossterm's input enum, not a domain enum: unbound keys do nothing"
)]
fn detail_key(model: &mut Model, key: KeyEvent, id: TicketId, scroll: u16) -> Vec<Effect> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => Vec::new(),
        KeyCode::Char('j') | KeyCode::Down => {
            model.mode = Mode::Detail {
                id,
                scroll: scroll.saturating_add(1),
            };
            Vec::new()
        }
        KeyCode::Char('k') | KeyCode::Up => {
            model.mode = Mode::Detail {
                id,
                scroll: scroll.saturating_sub(1),
            };
            Vec::new()
        }
        _ => {
            model.selected = Some(id.clone());
            model.mode = Mode::Detail { id, scroll };
            // An action that opens a prompt or confirm replaces Detail; others stay in it.
            ticket_key(model, key)
        }
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "KeyCode is crossterm's input enum, not a domain enum: unbound keys do nothing"
)]
fn filter_key(model: &mut Model, key: KeyEvent, mut input: Input) -> Vec<Effect> {
    match key.code {
        KeyCode::Enter => {}
        KeyCode::Esc => model.filter.clear(),
        _ => {
            input.handle_event(&Event::Key(key));
            input.value().clone_into(&mut model.filter);
            model.mode = Mode::Filtering(input);
        }
    }
    model.settle();
    Vec::new()
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "KeyCode is crossterm's input enum, not a domain enum: unbound keys do nothing"
)]
fn prompt_key(model: &mut Model, key: KeyEvent, prompt: Prompt, mut input: Input) -> Vec<Effect> {
    match key.code {
        KeyCode::Esc => Vec::new(),
        KeyCode::Enter => match prompt {
            Prompt::NewTitle { project } => match Title::try_new(input.value().to_owned()) {
                Ok(title) => {
                    model.edit_created = true;
                    vec![Effect::Run(Command::New {
                        project: Some(project),
                        title,
                        kind: tk_core::Kind::default(),
                        priority: Priority::default(),
                        labels: tk_core::Labels::new(),
                        depends_on: std::collections::BTreeSet::new(),
                        body: tk_core::Body::default(),
                        refs: Vec::new(),
                    })]
                }
                Err(e) => {
                    model.error(e.explain());
                    model.mode = Mode::Prompt(Prompt::NewTitle { project }, input);
                    Vec::new()
                }
            },
            Prompt::BlockReason { id, if_version } => {
                if let Ok(reason) = NonEmpty::try_new(input.value().to_owned()) {
                    vec![Effect::Run(Command::Block {
                        id,
                        if_version: Some(if_version),
                        reason,
                    })]
                } else {
                    model.error("a block reason is one non-blank line");
                    model.mode = Mode::Prompt(Prompt::BlockReason { id, if_version }, input);
                    Vec::new()
                }
            }
        },
        _ => {
            input.handle_event(&Event::Key(key));
            model.mode = Mode::Prompt(prompt, input);
            Vec::new()
        }
    }
}

fn step(model: &mut Model, by: isize) {
    let Some(rows) = model.rows() else {
        return;
    };
    let at = rows
        .iter()
        .position(|t| Some(&t.id) == model.selected.as_ref());
    let next = match at {
        Some(i) => i
            .saturating_add_signed(by)
            .min(rows.len().saturating_sub(1)),
        None => 0,
    };
    model.selected = rows.get(next).map(|t| t.id.clone());
}

fn shift_column(model: &mut Model, by: isize) {
    if model.view != View::Board {
        return;
    }
    let at = COLUMNS.iter().position(|&c| c == model.column).unwrap_or(0);
    let next = at.saturating_add_signed(by).min(COLUMNS.len() - 1);
    if let Some(&column) = COLUMNS.get(next) {
        model.column = column;
        model.selected = model.column_rows(column).first().map(|t| t.id.clone());
    }
}

fn switch_view(model: &mut Model, by: isize) {
    let at = View::ALL.iter().position(|&v| v == model.view).unwrap_or(0);
    let len = View::ALL.len();
    let next = (at + len).saturating_add_signed(by) % len;
    if let Some(&view) = View::ALL.get(next) {
        model.view = view;
        model.settle();
    }
}
