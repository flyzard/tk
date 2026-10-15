//! Printing results as text for people or JSON for agents.

use std::io::{self, Write as _};
use std::process::ExitCode;

use serde::Serialize;
use tk_app::{
    AppError, Brief, Capped, Item, LastHandoff, LintReport, Listing, Outcome, TicketJson, Waiting,
    render_invalid,
};
use tk_core::{Actor, Resolution, Status};
use tk_store::{Store, Workspace};

use crate::cli::Field;

pub struct Out {
    pub json: bool,
    /// Whether stderr is a terminal, for colored file errors.
    pub styled: bool,
}

impl Out {
    fn either(
        &self,
        value: &impl Serialize,
        text: impl FnOnce() -> String,
    ) -> Result<(), AppError> {
        if self.json {
            json_line(value)
        } else {
            print(&text())
        }
    }

    pub fn outcome(&self, outcome: &Outcome) -> Result<(), AppError> {
        self.either(outcome, || match outcome {
            Outcome::Created { id, version: _ } => id.to_string(),
            Outcome::Updated {
                id,
                state,
                version: _,
            } => format!("{id} {state}"),
            Outcome::Next {
                id,
                title,
                priority,
            } => format!("{id} {priority} {title}"),
            Outcome::NothingReady => String::from("nothing ready"),
        })
    }

    pub fn listing(&self, listing: &Listing, fields: &[Field]) -> Result<(), AppError> {
        if listing.needs_fixing > 0 {
            eprintln!(
                "warning: {} ticket file(s) don't parse and aren't listed; run `tk lint`",
                listing.needs_fixing
            );
        }
        let tickets = &listing.tickets;
        if !fields.is_empty() {
            return self.fields(tickets, fields);
        }
        self.either(tickets, || {
            tickets
                .iter()
                .map(|t| {
                    let waiting = if t.waiting_on.is_empty() {
                        String::new()
                    } else {
                        format!("  (waiting on {})", join(&t.waiting_on))
                    };
                    format!(
                        "{}  {:<7}  {}  {:<7}  {}{waiting}",
                        t.id,
                        t.status.name(),
                        t.priority,
                        t.kind,
                        t.title
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    /// Only the chosen fields: JSON objects with just those keys, or aligned text columns.
    fn fields(&self, tickets: &[TicketJson], fields: &[Field]) -> Result<(), AppError> {
        if self.json {
            let rows: Vec<serde_json::Map<String, serde_json::Value>> = tickets
                .iter()
                .map(|t| {
                    fields
                        .iter()
                        .map(|&f| (String::from(<&str>::from(f)), field_json(t, f)))
                        .collect()
                })
                .collect();
            return json_line(&rows);
        }
        let cells: Vec<Vec<String>> = tickets
            .iter()
            .map(|t| fields.iter().map(|&f| field_text(t, f)).collect())
            .collect();
        let widths: Vec<usize> = (0..fields.len())
            .map(|i| {
                cells
                    .iter()
                    .filter_map(|row| row.get(i))
                    .map(|c| c.chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let lines: Vec<String> = cells
            .iter()
            .map(|row| {
                let last = row.len().saturating_sub(1);
                row.iter()
                    .zip(&widths)
                    .enumerate()
                    .map(|(i, (cell, &width))| {
                        if i == last {
                            cell.clone()
                        } else {
                            format!("{cell:<width$}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("  ")
            })
            .collect();
        print(&lines.join("\n"))
    }

    pub fn ticket(&self, t: &TicketJson) -> Result<(), AppError> {
        self.either(t, || {
            let labels = join(&t.labels);
            let deps = deps(t);
            let mut lines = vec![
                format!("{}  {}", t.id, t.title),
                format!("status    {}", status(&t.status)),
                format!("kind      {}   priority {}", t.kind, t.priority),
                format!("labels    {labels}"),
                format!("depends   {deps}"),
                format!("created   {}", t.created),
                format!("version   {}", t.version),
            ];
            if !t.body.trim().is_empty() {
                lines.push(format!("\n{}", t.body.trim_end()));
            }
            lines.join("\n")
        })
    }

    pub fn brief(&self, b: &Brief) -> Result<(), AppError> {
        self.either(b, || brief_text(b))
    }

    /// Several briefs: a JSON array, or the texts one after another.
    pub fn briefs(&self, briefs: &[Brief], actor: &Actor) -> Result<(), AppError> {
        self.either(&briefs, || {
            if briefs.is_empty() {
                return format!("nothing in doing by {actor} here; pass a ticket ID");
            }
            briefs
                .iter()
                .map(brief_text)
                .collect::<Vec<_>>()
                .join("\n\n---\n\n")
        })
    }

    pub fn lint(&self, report: &LintReport) -> Result<ExitCode, AppError> {
        let code = if report.is_clean() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
        if self.json {
            json_line(&report.findings())?;
        } else if report.is_clean() {
            print("ok")?;
        } else {
            for file in &report.invalid {
                eprintln!("{}\n", render_invalid(file, self.styled));
            }
            for problem in &report.problems {
                eprintln!("error: {problem}");
            }
        }
        Ok(code)
    }

    pub fn initialized(&self, store: &Store, workspace: &Workspace) -> Result<(), AppError> {
        #[derive(Serialize)]
        struct Init<'a> {
            key: String,
            repo: &'a str,
            workspace: &'a str,
        }
        let init = Init {
            key: store.key().to_string(),
            repo: store.repo().as_str(),
            workspace: workspace.path.as_str(),
        };
        self.either(&init, || {
            format!(
                "created .tickets/ for project {} and added it to {}
\
                 .gitattributes routes ticket files to tk's merge driver; register it once per clone:
  \
                 git config merge.tk.driver \"tk merge-driver %O %A %B %P\"",
                init.key, init.workspace
            )
        })
    }

    /// Prints the error and returns its exit status.
    pub fn error(&self, e: &AppError) -> ExitCode {
        if self.json {
            match serde_json::to_string(&e.to_json()) {
                Ok(line) => eprintln!("{line}"),
                Err(_) => eprintln!("{e}"),
            }
        } else if let AppError::InvalidFile(file) = e {
            eprintln!("{}", render_invalid(file, self.styled));
        } else {
            eprintln!("error: {e}");
            if let Some(hint) = e.hint() {
                eprintln!("hint: {hint}");
            }
        }
        ExitCode::from(e.code().exit())
    }
}

/// Prints one line to stdout.
pub fn print(text: &str) -> Result<(), AppError> {
    let mut stdout = io::stdout().lock();
    match writeln!(stdout, "{text}") {
        // `tk ls | head` closing the pipe early isn't an error.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

pub fn json_line(value: &impl Serialize) -> Result<(), AppError> {
    print(&serde_json::to_string(value).map_err(|e| AppError::Internal(e.to_string()))?)
}

fn field_json(t: &TicketJson, field: Field) -> serde_json::Value {
    match field {
        Field::Id => value(&t.id),
        Field::State => value(&t.status.name()),
        Field::Priority => value(&t.priority),
        Field::Kind => value(&t.kind),
        Field::Title => value(&t.title),
        Field::Labels => value(&t.labels),
        Field::DependsOn => value(&t.depends_on),
        Field::WaitingOn => value(&t.waiting_on),
        Field::Created => value(&t.created),
        Field::Status => value(&t.status),
        Field::Version => value(&t.version),
        Field::Body => value(&t.body),
    }
}

/// Every field type serializes; `Null` stands in if one ever fails.
fn value(v: &impl Serialize) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or(serde_json::Value::Null)
}

fn field_text(t: &TicketJson, field: Field) -> String {
    match field {
        Field::Id => t.id.to_string(),
        Field::State => t.status.name().to_string(),
        Field::Priority => t.priority.to_string(),
        Field::Kind => t.kind.to_string(),
        Field::Title => t.title.to_string(),
        Field::Labels => join(&t.labels),
        Field::DependsOn => join(&t.depends_on),
        Field::WaitingOn => join(&t.waiting_on),
        Field::Created => t.created.to_string(),
        Field::Status => status(&t.status),
        Field::Version => t.version.to_string(),
        Field::Body => t.body.replace('\n', " "),
    }
}

/// A status in words, for people.
pub fn status(s: &Status) -> String {
    match s {
        Status::Todo => String::from("todo"),
        Status::Doing { by, since } => format!("doing, by {by} since {since}"),
        Status::Blocked { reason, since } => format!("blocked since {since}: {reason}"),
        Status::Done { at, resolution } => match resolution {
            Resolution::Completed => format!("done at {at}"),
            Resolution::WontDo => format!("done at {at} (won't do)"),
            Resolution::Duplicate(of) => format!("done at {at} (duplicate of {of})"),
        },
    }
}

/// A ticket's dependencies, each marked waiting or done.
pub fn deps(t: &TicketJson) -> String {
    t.depends_on
        .iter()
        .map(|d| {
            if t.waiting_on.contains(d) {
                format!("{d} (waiting)")
            } else {
                format!("{d} (done)")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn join<T: std::fmt::Display>(items: impl IntoIterator<Item = T>) -> String {
    items
        .into_iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn brief_text(b: &Brief) -> String {
    // No `..`: a new field shows up here until the text decides about it.
    let Brief {
        id,
        title,
        kind,
        priority,
        status: state,
        version: _,
        waiting_on,
        prose,
        handoff,
        ruled_out,
        decided,
        notes,
    } = b;
    let mut out = vec![
        format!("{id}  {title}"),
        format!("{}   {kind} {priority}", status(state)),
    ];
    if !waiting_on.is_empty() {
        let waiting = waiting_on.iter().map(|Waiting { id, state }| match state {
            Some(state) => format!("{id} ({state})"),
            None => format!("{id} (unreadable)"),
        });
        out.push(format!("waiting on  {}", join(waiting)));
    }
    if !prose.is_empty() {
        out.extend([String::new(), prose.clone()]);
    }
    if let Some(LastHandoff {
        at: _,
        by,
        age_minutes,
        next,
        entries_since,
    }) = handoff
    {
        let since = match entries_since {
            0 => String::new(),
            n => format!(", {n} entries since"),
        };
        out.push(String::new());
        out.push(format!(
            "Next (handoff {} ago by {by}{since})",
            age(*age_minutes)
        ));
        out.extend(next.iter().map(|n| format!("- {n}")));
    }
    section(&mut out, "Ruled out", ruled_out);
    section(&mut out, "Decided", decided);
    section(&mut out, "Notes", notes);
    out.join("\n")
}

fn section(out: &mut Vec<String>, heading: &str, capped: &Capped<Item>) {
    let Capped { items, older } = capped;
    if items.is_empty() {
        return;
    }
    out.push(String::new());
    out.push(match older {
        0 => heading.to_owned(),
        n => format!("{heading} ({n} older: tk show)"),
    });
    out.extend(
        items
            .iter()
            .map(|Item { at: _, by: _, text }| format!("- {text}")),
    );
}

/// Minutes as a rough age, like `2h 5m` or `3d`.
fn age(minutes: i64) -> String {
    let m = minutes.max(0);
    match (m / 1440, m / 60, m % 60) {
        (0, 0, m) => format!("{m}m"),
        (0, h, 0) => format!("{h}h"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, _, _) => format!("{d}d"),
    }
}
