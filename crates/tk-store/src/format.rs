//! A ticket file: TOML frontmatter between `+++` lines, then a free Markdown body.
//!
//! New files are rendered from scratch. Changes to existing files go through
//! `toml_edit`, so comments, spacing and key order survive and a status change
//! is a one-line diff.

use std::ops::Range;

use tk_core::{Body, Frontmatter, Resolution, Status, Ticket, TicketId, TicketView, Timestamp};
use toml_edit::{Array, Datetime, DocumentMut, InlineTable, Item, Table, Value};

use crate::error::ParseError;

const FENCE: &str = "+++";

/// The parsed frontmatter of a file that was read, kept for lossless edits.
#[derive(Clone, Debug)]
pub struct TicketDoc(DocumentMut);

/// Parses a whole file. The ID comes from the filename.
///
/// # Errors
/// [`ParseError`] saying what is wrong and, when it can, where.
pub fn parse(id: TicketId, text: &str) -> Result<(Ticket, TicketDoc), ParseError> {
    let (toml, body) = split(text)?;
    let front: DocumentMut = toml
        .parse()
        .map_err(|e: toml_edit::TomlError| ParseError::Toml {
            message: e.message().to_owned(),
            span: e.span().map(|r| shift(r, FENCE.len() + 1)),
        })?;
    // TOML datetimes reach serde as maps, which jiff can't read. Quoting them
    // keeps every line in place, so error lines still point into the original.
    let mut plain = front.clone();
    quote_datetimes(plain.as_table_mut());
    let plain = plain.to_string();
    let fields: Frontmatter = toml::from_str(&plain).map_err(|e| ParseError::Fields {
        message: e.message().to_owned(),
        span: e
            .span()
            .and_then(|r| line_of(&plain, r.start))
            .and_then(|n| line_span(text, n + 1)),
    })?;
    Ok((
        Ticket::from_stored(id, fields, Body::new(body.to_owned())),
        TicketDoc(front),
    ))
}

/// Renders a ticket as a new file.
#[must_use]
pub fn render(ticket: &Ticket) -> String {
    let TicketView {
        id: _,
        title,
        kind,
        priority,
        labels,
        depends_on,
        created,
        status,
        body,
    } = ticket.view();
    let mut front = DocumentMut::new();
    let table = front.as_table_mut();
    table.insert("title", Item::Value(Value::from(title.as_ref())));
    table.insert("kind", Item::Value(Value::from(<&str>::from(kind))));
    table.insert("priority", Item::Value(Value::from(<&str>::from(priority))));
    table.insert(
        "labels",
        Item::Value(labels_value(labels.iter().map(AsRef::as_ref))),
    );
    table.insert("depends_on", Item::Value(ids_value(depends_on)));
    table.insert("created", Item::Value(timestamp_value(created)));
    table.insert("status", Item::Value(status_value(status)));
    file(&front, body)
}

/// Renders `after` into the document read for `before`, changing only the keys that differ.
pub(crate) fn render_update(doc: &mut TicketDoc, before: &Ticket, after: &Ticket) -> String {
    let old = before.view();
    // No `..`: a new Ticket field fails to compile here until saving decides about it.
    let TicketView {
        id: _,
        title,
        kind,
        priority,
        labels,
        depends_on,
        created,
        status,
        body,
    } = after.view();
    let table = doc.0.as_table_mut();
    if title != old.title {
        set(table, "title", Value::from(title.as_ref()));
    }
    if kind != old.kind {
        set(table, "kind", Value::from(<&str>::from(kind)));
    }
    if priority != old.priority {
        set(table, "priority", Value::from(<&str>::from(priority)));
    }
    if labels != old.labels {
        set(
            table,
            "labels",
            labels_value(labels.iter().map(AsRef::as_ref)),
        );
    }
    if depends_on != old.depends_on {
        set(table, "depends_on", ids_value(depends_on));
    }
    if created != old.created {
        set(table, "created", timestamp_value(created));
    }
    if status != old.status {
        set(table, "status", status_value(status));
    }
    file(&doc.0, body)
}

fn file(front: &DocumentMut, body: &Body) -> String {
    let body = body.as_ref();
    let gap = if body.is_empty() { "" } else { "\n" };
    format!("{FENCE}\n{front}{FENCE}\n{gap}{body}")
}

/// Splits a file into its frontmatter and body. One blank line after the
/// closing fence belongs to the file layout, not the body.
pub(crate) fn split(text: &str) -> Result<(&str, &str), ParseError> {
    // CRLF files (a checkout without `eol=lf`) parse too; the TOML parser accepts either.
    let rest = text
        .strip_prefix("+++\n")
        .or_else(|| text.strip_prefix("+++\r\n"))
        .ok_or(ParseError::NoFrontmatter)?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == FENCE {
            let (toml, after) = rest.split_at_checked(offset).ok_or(ParseError::Unclosed)?;
            let body = after.get(line.len()..).unwrap_or_default();
            let body = body
                .strip_prefix("\r\n")
                .or_else(|| body.strip_prefix('\n'))
                .unwrap_or(body);
            return Ok((toml, body));
        }
        offset += line.len();
    }
    Err(ParseError::Unclosed)
}

/// Replaces a value in place, keeping its key, position and surrounding whitespace and comments.
pub(crate) fn set(table: &mut Table, key: &str, mut value: Value) {
    match table.get_mut(key) {
        Some(Item::Value(old)) => {
            *value.decor_mut() = old.decor().clone();
            *old = value;
        }
        Some(slot @ (Item::None | Item::Table(_) | Item::ArrayOfTables(_))) => {
            *slot = Item::Value(value);
        }
        None => {
            table.insert(key, Item::Value(value));
        }
    }
}

fn labels_value<'a>(labels: impl Iterator<Item = &'a str>) -> Value {
    Value::Array(labels.collect::<Array>())
}

fn ids_value<'a>(ids: impl IntoIterator<Item = &'a TicketId>) -> Value {
    Value::Array(ids.into_iter().map(ToString::to_string).collect::<Array>())
}

/// A native TOML datetime, or a string in the rare case TOML can't hold the instant.
fn timestamp_value(t: Timestamp) -> Value {
    let text = t.to_string();
    match text.parse::<Datetime>() {
        Ok(datetime) => Value::from(datetime),
        Err(_) => Value::from(text),
    }
}

/// Status on one line, as an inline table: when two branches change it,
/// git shows one conflicting line and either side is valid TOML.
fn status_value(status: &Status) -> Value {
    let mut table = InlineTable::new();
    table.insert("state", Value::from(<&str>::from(status.name())));
    match status {
        Status::Todo => {}
        Status::Doing { by, since } => {
            table.insert("by", Value::from(by.to_string()));
            table.insert("since", timestamp_value(*since));
        }
        Status::Blocked { reason, since } => {
            table.insert("reason", Value::from(reason.as_ref()));
            table.insert("since", timestamp_value(*since));
        }
        Status::Done { at, resolution } => {
            table.insert("at", timestamp_value(*at));
            table.insert("resolution", resolution_value(resolution));
        }
    }
    Value::InlineTable(table)
}

fn resolution_value(resolution: &Resolution) -> Value {
    match resolution {
        Resolution::Completed => Value::from("completed"),
        Resolution::WontDo => Value::from("wont_do"),
        Resolution::Duplicate(of) => {
            let mut table = InlineTable::new();
            table.insert("duplicate", Value::from(of.to_string()));
            Value::InlineTable(table)
        }
    }
}

fn quote_datetimes(table: &mut Table) {
    for (_, item) in table.iter_mut() {
        quote_item(item);
    }
}

fn quote_item(item: &mut Item) {
    match item {
        Item::Value(value) => quote_value(value),
        Item::Table(table) => quote_datetimes(table),
        Item::ArrayOfTables(tables) => tables.iter_mut().for_each(quote_datetimes),
        Item::None => {}
    }
}

fn quote_value(value: &mut Value) {
    let quoted = match value {
        Value::Datetime(datetime) => Some(datetime.value().to_string()),
        Value::Array(array) => {
            array.iter_mut().for_each(quote_value);
            None
        }
        Value::InlineTable(table) => {
            table.iter_mut().for_each(|(_, v)| quote_value(v));
            None
        }
        Value::String(_) | Value::Integer(_) | Value::Float(_) | Value::Boolean(_) => None,
    };
    if let Some(text) = quoted {
        *value = Value::from(text);
    }
}

fn shift(r: Range<usize>, by: usize) -> Range<usize> {
    r.start + by..r.end + by
}

/// The 0-based line containing byte `at`.
fn line_of(text: &str, at: usize) -> Option<usize> {
    Some(text.get(..at)?.matches('\n').count())
}

/// The byte range of 0-based line `n`, without its newline.
fn line_span(text: &str, n: usize) -> Option<Range<usize>> {
    let start: usize = text.split_inclusive('\n').take(n).map(str::len).sum();
    let line = text.get(start..)?.split('\n').next()?;
    Some(start..start + line.len())
}
