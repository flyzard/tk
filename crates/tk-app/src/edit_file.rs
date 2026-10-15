//! Editing a ticket file in your own editor. The edited text is parsed, then
//! turned into an ordinary `Command::Edit` with the version it was based on, so
//! it goes through the same `execute` and conflict check as every other write.

use tk_core::{TicketId, TicketView};
use tk_store::{Version, VersionHex, format};

use crate::command::{Command, FieldChange};
use crate::ctx::Ctx;
use crate::error::AppError;

/// A ticket file's text and version, read under the project lock.
///
/// # Errors
/// [`AppError::NotFound`] or a store error.
pub fn read_file(ctx: &Ctx, id: &TicketId) -> Result<(String, Version), AppError> {
    ctx.store(id.project())?
        .transaction(|tx| -> Result<_, AppError> { Ok(tx.read_text(id)?) })
}

/// The command that turns `original` into `edited`, or `None` when nothing changed.
///
/// # Errors
/// Why the edited text can't be saved: it doesn't parse, or it changes a field
/// that only moves through its own command (status, dependencies, created).
pub fn edit_command(
    id: &TicketId,
    base: Version,
    original: &str,
    edited: &str,
) -> Result<Option<Command>, String> {
    let (before, _) = format::parse(id.clone(), original)
        .map_err(|e| format!("the file as read doesn't parse: {e}"))?;
    let (after, _) = format::parse(id.clone(), edited).map_err(|e| e.to_string())?;
    let old = before.view();
    // No `..`: a new Ticket field fails to compile here until editing decides about it.
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
    if status != old.status {
        return Err(String::from(
            "status can't change in the editor; use s, b, u, x, d or r",
        ));
    }
    if depends_on != old.depends_on {
        return Err(String::from(
            "dependencies can't change in the editor; use `tk dep add` or `tk dep rm`",
        ));
    }
    if created != old.created {
        return Err(String::from("`created` can't change"));
    }
    let mut changes = Vec::new();
    if title != old.title {
        changes.push(FieldChange::Title(title.clone()));
    }
    if kind != old.kind {
        changes.push(FieldChange::Kind(kind));
    }
    if priority != old.priority {
        changes.push(FieldChange::Priority(priority));
    }
    changes.extend(
        old.labels
            .difference(labels)
            .cloned()
            .map(FieldChange::RemoveLabel),
    );
    changes.extend(
        labels
            .difference(old.labels)
            .cloned()
            .map(FieldChange::AddLabel),
    );
    if body != old.body {
        changes.push(FieldChange::Body(body.clone()));
    }
    Ok((!changes.is_empty()).then(|| Command::Edit {
        id: id.clone(),
        if_version: Some(VersionHex::from(base)),
        changes,
    }))
}
