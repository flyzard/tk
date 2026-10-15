//! Drawing the model. Reads it, never changes it.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize as _};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
    Tabs, Wrap,
};
use tk_app::{Scope, TicketJson};
use tk_core::{Priority, StateName, Status};

use crate::output::{deps, join, status};
use tui_input::Input;

use super::model::{COLUMNS, Mode, Model, Prompt, Sort, View};

const KEYS: &[(&str, &str)] = &[
    ("j / k", "move down / up"),
    ("h / l", "previous / next board column"),
    ("tab, 1-4", "switch view: board, list, ready, needs fixing"),
    ("enter", "open detail (j / k scroll, esc back)"),
    ("n", "new ticket: title, then your editor for the body"),
    ("e", "edit in your editor, validated on save"),
    ("s / b / d / r", "start / block / done / reopen"),
    ("u / x", "unblock / release (doing -> todo)"),
    ("p", "cycle priority"),
    ("/", "fuzzy filter (esc clears)"),
    ("g", "this project / all projects"),
    ("o", "cycle the list's sort"),
    ("?", "this help"),
    ("q", "quit"),
];

pub fn render(model: &Model, frame: &mut Frame<'_>) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    render_header(model, frame, header);
    match &model.mode {
        Mode::Detail { id, scroll } => match model.data.tickets.iter().find(|t| &t.id == id) {
            Some(t) => render_detail(t, *scroll, frame, body),
            None => frame.render_widget(Paragraph::new(format!("{id} is gone")), body),
        },
        Mode::Normal
        | Mode::Filtering(_)
        | Mode::Prompt(..)
        | Mode::Confirm { .. }
        | Mode::Reedit { .. }
        | Mode::Help => match model.view {
            View::Board => render_board(model, frame, body),
            View::List => render_list(model, frame, body),
            View::Ready => render_ready(model, frame, body),
            View::NeedsFixing => render_needs_fixing(model, frame, body),
        },
    }
    render_footer(model, frame, footer);
    render_overlay(model, frame);
}

fn view_name(view: View) -> &'static str {
    match view {
        View::Board => "Board",
        View::List => "List",
        View::Ready => "Ready",
        View::NeedsFixing => "Needs fixing",
    }
}

fn render_header(model: &Model, frame: &mut Frame<'_>, area: Rect) {
    let [tabs, info] = Layout::horizontal([Constraint::Min(0), Constraint::Length(48)]).areas(area);
    match &model.mode {
        // The tabs can't be switched from a ticket, so show where it was opened from instead.
        Mode::Detail { id, scroll: _ } => frame.render_widget(
            Line::from(vec![
                Span::raw(format!("{} › ", view_name(model.view))).dim(),
                Span::raw(id.to_string()).bold(),
            ]),
            tabs,
        ),
        Mode::Normal
        | Mode::Filtering(_)
        | Mode::Prompt(..)
        | Mode::Confirm { .. }
        | Mode::Reedit { .. }
        | Mode::Help => {
            let fixing = model.data.findings.len();
            let titles = View::ALL.map(|v| match (v, fixing) {
                (View::NeedsFixing, 1..) => format!("{} ({fixing})", view_name(v)),
                (View::Board | View::List | View::Ready | View::NeedsFixing, _) => {
                    String::from(view_name(v))
                }
            });
            let selected = View::ALL.iter().position(|&v| v == model.view);
            frame.render_widget(
                Tabs::new(titles)
                    .select(selected)
                    .highlight_style(Style::new().bold().reversed()),
                tabs,
            );
        }
    }
    let scope = match (model.scope, &model.current) {
        (Scope::Current, Some(key)) => format!("project {key}"),
        (Scope::Current, None) | (Scope::All, _) => String::from("all projects"),
    };
    let filter = if model.filter.is_empty() {
        String::new()
    } else {
        format!("  /{}", model.filter)
    };
    frame.render_widget(
        Line::from(format!("{scope}{filter}")).right_aligned().dim(),
        info,
    );
}

fn render_footer(model: &Model, frame: &mut Frame<'_>, area: Rect) {
    let line = match &model.notice {
        Some(n) if n.error => Line::from(n.text.as_str()).fg(Color::Red),
        Some(n) => Line::from(n.text.as_str()).fg(Color::Green),
        None => Line::from("j/k move  enter detail  n new  e edit  s start  b block  d done  / filter  g scope  ? help  q quit").dim(),
    };
    frame.render_widget(line, area);
}

fn render_board(model: &Model, frame: &mut Frame<'_>, area: Rect) {
    let areas = Layout::horizontal([Constraint::Ratio(1, 4); 4]).split(area);
    // Filtered and sorted once per frame, then split into columns.
    let visible = model.visible();
    for (state, column) in COLUMNS.iter().zip(areas.iter()) {
        let rows: Vec<&TicketJson> = visible
            .iter()
            .copied()
            .filter(|t| t.status.name() == *state)
            .collect();
        let focused = model.column == *state;
        let items: Vec<ListItem<'_>> = rows
            .iter()
            .map(|t| {
                let mut spans = vec![
                    priority(t.priority),
                    Span::raw(" "),
                    Span::raw(t.title.as_ref()),
                ];
                if !t.waiting_on.is_empty() {
                    spans.push(Span::raw(" (waiting)").dim());
                }
                ListItem::new(vec![Line::from(spans), Line::from(t.id.to_string()).dim()])
            })
            .collect();
        let border = if focused {
            Style::new().fg(Color::Cyan)
        } else {
            Style::new().dim()
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(border)
            .title(format!(" {} ({}) ", state_title(*state), rows.len()));
        let selected = if focused {
            rows.iter()
                .position(|t| Some(&t.id) == model.selected.as_ref())
        } else {
            None
        };
        let list = List::new(items)
            .block(block)
            .highlight_style(Style::new().reversed());
        frame.render_stateful_widget(
            list,
            *column,
            &mut ListState::default().with_selected(selected),
        );
    }
}

fn render_list(model: &Model, frame: &mut Frame<'_>, area: Rect) {
    let rows = model.visible();
    let sort = match model.sort {
        Sort::Priority => "priority",
        Sort::Created => "created",
        Sort::Title => "title",
        Sort::State => "state",
    };
    let table = Table::new(
        rows.iter().map(|t| {
            Row::new(vec![
                Cell::from(t.id.to_string()),
                Cell::from(state(&t.status)),
                Cell::from(priority(t.priority)),
                Cell::from(t.kind.to_string()),
                Cell::from(t.title.as_ref().to_owned()),
                Cell::from(join(&t.labels)).dim(),
            ])
        }),
        [
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(24),
        ],
    )
    .header(Row::new(["ID", "State", "Pri", "Kind", "Title", "Labels"]).bold())
    .block(
        Block::new()
            .borders(Borders::TOP)
            .title(format!(" sorted by {sort} (o) "))
            .dim(),
    )
    .row_highlight_style(Style::new().reversed());
    let selected = rows
        .iter()
        .position(|t| Some(&t.id) == model.selected.as_ref());
    frame.render_stateful_widget(
        table,
        area,
        &mut TableState::default().with_selected(selected),
    );
}

fn render_ready(model: &Model, frame: &mut Frame<'_>, area: Rect) {
    let rows = model.visible();
    let table = Table::new(
        rows.iter().map(|t| {
            Row::new(vec![
                Cell::from(t.id.to_string()),
                Cell::from(priority(t.priority)),
                Cell::from(t.kind.to_string()),
                Cell::from(t.title.as_ref().to_owned()),
            ])
        }),
        [
            Constraint::Length(10),
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Min(20),
        ],
    )
    .header(Row::new(["ID", "Pri", "Kind", "Title"]).bold())
    .block(
        Block::new()
            .borders(Borders::TOP)
            .title(" what `tk next` would pick, across all projects ")
            .dim(),
    )
    .row_highlight_style(Style::new().reversed());
    let selected = rows
        .iter()
        .position(|t| Some(&t.id) == model.selected.as_ref());
    frame.render_stateful_widget(
        table,
        area,
        &mut TableState::default().with_selected(selected),
    );
}

fn render_needs_fixing(model: &Model, frame: &mut Frame<'_>, area: Rect) {
    let mut lines: Vec<Line<'_>> = Vec::new();
    for f in &model.data.findings {
        let at = match (&f.path, f.line) {
            (Some(path), Some(line)) => format!("{path}:{line}"),
            (Some(path), None) => path.clone(),
            (None, Some(_) | None) => String::new(),
        };
        if !at.is_empty() {
            lines.push(Line::from(at).bold());
        }
        lines.push(Line::from(format!("  {}", f.message)).fg(Color::Red));
    }
    if lines.is_empty() {
        lines.push(
            Line::from("Every ticket file parses and the dependency graph is valid.")
                .fg(Color::Green),
        );
    }
    let block = Block::new()
        .borders(Borders::TOP)
        .title(" files that don't parse, and dependency problems ")
        .dim();
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_detail(t: &TicketJson, scroll: u16, frame: &mut Frame<'_>, area: Rect) {
    let field = |name: &'static str, value: String| {
        Line::from(vec![
            Span::raw(format!("{name:<10}")).dim(),
            Span::raw(value),
        ])
    };
    let mut text = Text::from(vec![
        Line::from(vec![
            Span::raw(t.id.to_string()).dim(),
            Span::raw("  "),
            Span::raw(t.title.as_ref()).bold(),
        ]),
        Line::default(),
        field("status", status(&t.status)),
        Line::from(vec![
            Span::raw(format!("{:<10}", "priority")).dim(),
            priority(t.priority),
            Span::raw(format!("   kind {}", t.kind)),
        ]),
        field("labels", join(&t.labels)),
        field("depends", deps(t)),
        field("created", t.created.to_string()),
        field("version", t.version.to_string()),
        Line::default(),
    ]);
    text.extend(tui_markdown::from_str(&t.body));
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Cyan))
        .title(" detail · esc back ");
    frame.render_widget(
        Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

fn render_overlay(model: &Model, frame: &mut Frame<'_>) {
    let area = frame.area();
    match &model.mode {
        Mode::Normal | Mode::Detail { .. } => {}
        Mode::Filtering(input) => {
            render_input(frame, area, " filter · enter keep · esc clear ", input);
        }
        Mode::Prompt(Prompt::NewTitle { project }, input) => {
            render_input(
                frame,
                area,
                &format!(" new ticket in {project} · title "),
                input,
            );
        }
        Mode::Prompt(Prompt::BlockReason { id, .. }, input) => {
            render_input(frame, area, &format!(" block {id} · reason "), input);
        }
        Mode::Confirm { question, .. } => render_message(
            frame,
            area,
            " confirm ",
            &format!("{question}  y / n"),
            Color::Yellow,
        ),
        Mode::Reedit { id, error, .. } => render_message(
            frame,
            area,
            " can't save ",
            &format!("{id}: {error}\n\nReopen the editor with your text? y / n (n discards it)"),
            Color::Red,
        ),
        Mode::Help => {
            let lines: Vec<Line<'_>> = KEYS
                .iter()
                .map(|(key, what)| {
                    Line::from(vec![
                        Span::raw(format!("{key:<14}")).bold(),
                        Span::raw(*what),
                    ])
                })
                .collect();
            let popup = centered(area, 64, u16::try_from(KEYS.len()).unwrap_or(16) + 2);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new(lines).block(Block::bordered().title(" keys · any key closes ")),
                popup,
            );
        }
    }
}

fn render_input(frame: &mut Frame<'_>, area: Rect, title: &str, input: &Input) {
    let popup = centered(area, 64, 3);
    let width = usize::from(popup.width.saturating_sub(2));
    let scroll = input.visual_scroll(width);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(input.value())
            .scroll((0, u16::try_from(scroll).unwrap_or(0)))
            .block(
                Block::bordered()
                    .border_style(Style::new().fg(Color::Cyan))
                    .title(title),
            ),
        popup,
    );
    let cursor = input.visual_cursor().saturating_sub(scroll);
    frame.set_cursor_position((
        popup.x + 1 + u16::try_from(cursor).unwrap_or(0),
        popup.y + 1,
    ));
}

fn render_message(frame: &mut Frame<'_>, area: Rect, title: &str, text: &str, color: Color) {
    let popup = centered(area, 72, 7);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .border_style(Style::new().fg(color))
                .title(title),
        ),
        popup,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 3,
        width,
        height,
    }
}

fn priority(p: Priority) -> Span<'static> {
    let style = match p {
        Priority::P0 => Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        Priority::P1 => Style::new().fg(Color::Yellow),
        Priority::P2 => Style::new(),
        Priority::P3 => Style::new().dim(),
    };
    Span::styled(p.to_string(), style)
}

fn state(s: &Status) -> Span<'static> {
    let name = s.name();
    let style = match name {
        StateName::Todo => Style::new(),
        StateName::Doing => Style::new().fg(Color::Cyan),
        StateName::Blocked => Style::new().fg(Color::Magenta),
        StateName::Done => Style::new().fg(Color::Green).dim(),
    };
    Span::styled(name.to_string(), style)
}

fn state_title(s: StateName) -> &'static str {
    match s {
        StateName::Todo => "Todo",
        StateName::Doing => "Doing",
        StateName::Blocked => "Blocked",
        StateName::Done => "Done",
    }
}
