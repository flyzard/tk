//! `tk merge-driver`: git hands it the base, ours and theirs versions of one
//! ticket file. Frontmatter merges field by field, `labels` and `depends_on`
//! as sets, and `## Log` lines as a union ordered by timestamp, so two agents'
//! branches merge without hand-editing unless they changed the same field.

use std::collections::BTreeSet;

use tk_core::Timestamp;
use toml_edit::{Array, DocumentMut, Item, Value};

use crate::format;
use crate::log::{line_stamp, log_entries, with_log_entries, without_log};

/// The merged file. `Conflicted` text carries git-style markers around each
/// field both sides changed differently; each side alone is valid TOML.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Merged {
    Clean(String),
    Conflicted(String),
}

/// Fields whose values are sets, merged element by element.
const SET_FIELDS: [&str; 2] = ["labels", "depends_on"];

#[must_use]
pub fn merge(base: &str, ours: &str, theirs: &str) -> Merged {
    let (Some((b_doc, b_body)), Some((o_doc, o_body)), Some((t_doc, t_body))) =
        (parse(base), parse(ours), parse(theirs))
    else {
        return Merged::Conflicted(markers(ours, theirs));
    };
    let mut out = o_doc.clone();
    let mut conflicts: Vec<(String, Option<Value>)> = Vec::new();
    // Ours' keys in order, then keys only theirs has. TOML keys are unique per table.
    let keys: Vec<String> = o_doc
        .as_table()
        .iter()
        .map(|(k, _)| k)
        .chain(
            t_doc
                .as_table()
                .iter()
                .map(|(k, _)| k)
                .filter(|k| !o_doc.contains_key(k)),
        )
        .map(str::to_owned)
        .collect();
    for key in keys {
        let (b, o, t) = (
            value(&b_doc, &key),
            value(&o_doc, &key),
            value(&t_doc, &key),
        );
        match merge_value(&key, b, o, t) {
            Resolved::To(picked) if norm(picked.as_ref()) == norm(o) => {}
            Resolved::To(Some(v)) => format::set(out.as_table_mut(), &key, v),
            Resolved::To(None) => {
                out.as_table_mut().remove(&key);
            }
            Resolved::Conflict => conflicts.push((key, t.cloned())),
        }
    }
    let body = merge_body(&b_body, &o_body, &t_body).unwrap_or_else(|| {
        conflicts.push((String::from("body"), None));
        markers(&o_body, &t_body)
    });
    let mut front = out.to_string();
    for (key, theirs) in &conflicts {
        front = mark_line(&front, key, theirs.as_ref());
    }
    let gap = if body.is_empty() { "" } else { "\n" };
    let text = format!("+++\n{front}+++\n{gap}{body}");
    if conflicts.is_empty() {
        Merged::Clean(text)
    } else {
        Merged::Conflicted(text)
    }
}

/// How one field merged: to a value (`None` when the key is removed), or not at all.
enum Resolved {
    To(Option<Value>),
    Conflict,
}

/// Three-way merge of one field.
fn merge_value(key: &str, b: Option<&Value>, o: Option<&Value>, t: Option<&Value>) -> Resolved {
    let (nb, no, nt) = (norm(b), norm(o), norm(t));
    if no == nt || nt == nb {
        return Resolved::To(o.cloned());
    }
    if no == nb {
        return Resolved::To(t.cloned());
    }
    if SET_FIELDS.contains(&key) {
        let (Some(b), Some(o), Some(t)) = (strings(b), strings(o), strings(t)) else {
            return Resolved::Conflict;
        };
        let removed: BTreeSet<&String> = b
            .iter()
            .filter(|x| !o.contains(*x) || !t.contains(*x))
            .collect();
        let merged: Array = o
            .union(&t)
            .filter(|x| !removed.contains(x))
            .map(String::as_str)
            .collect();
        return Resolved::To(Some(Value::Array(merged)));
    }
    Resolved::Conflict
}

/// Prose merges whole; log entries merge as a union, by timestamp when every entry has one.
fn merge_body(b: &str, o: &str, t: &str) -> Option<String> {
    // Trailing whitespace doesn't count: the first `tk log` adds a blank line before `## Log`.
    let prose = |body: &str| without_log(body).trim_end().to_owned();
    let (bp, op, tp) = (prose(b), prose(o), prose(t));
    let side = if op == tp || tp == bp {
        o
    } else if op == bp {
        t
    } else {
        return None;
    };
    let mut entries: Vec<String> = log_entries(o).into_iter().map(str::to_owned).collect();
    for e in log_entries(t) {
        if !entries.iter().any(|x| x == e) {
            entries.push(e.to_owned());
        }
    }
    // A stable sort: entries with the same timestamp keep their order.
    if entries.iter().all(|e| stamp(e).is_some()) {
        entries.sort_by_key(|e| stamp(e));
    }
    if log_entries(side)
        .into_iter()
        .eq(entries.iter().map(String::as_str))
    {
        return Some(side.to_owned());
    }
    Some(with_log_entries(side, &entries))
}

/// A ticket file's frontmatter document and body, or `None` if it isn't one.
fn parse(text: &str) -> Option<(DocumentMut, String)> {
    let (toml, body) = format::split(text).ok()?;
    Some((toml.parse().ok()?, body.to_owned()))
}

/// The timestamp a log line starts with, if it has one.
fn stamp(entry: &str) -> Option<Timestamp> {
    line_stamp(entry).map(|(at, _)| at)
}

fn value<'a>(doc: &'a DocumentMut, key: &str) -> Option<&'a Value> {
    doc.as_table().get(key).and_then(Item::as_value)
}

/// A value without formatting, so spacing differences don't count as changes.
fn norm(v: Option<&Value>) -> Option<String> {
    let mut v = v?.clone();
    v.decor_mut().clear();
    match &mut v {
        Value::InlineTable(table) => table.fmt(),
        Value::Array(array) => array.fmt(),
        Value::String(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Boolean(_)
        | Value::Datetime(_) => {}
    }
    Some(v.to_string())
}

fn strings(v: Option<&Value>) -> Option<BTreeSet<String>> {
    match v {
        None => Some(BTreeSet::new()),
        Some(v) => v
            .as_array()?
            .iter()
            .map(|x| x.as_str().map(str::to_owned))
            .collect(),
    }
}

/// Wraps the line `key = ...` in conflict markers, with theirs underneath.
fn mark_line(front: &str, key: &str, theirs: Option<&Value>) -> String {
    let theirs = theirs
        .and_then(|v| norm(Some(v)))
        .map_or_else(String::new, |v| format!("{key} = {v}\n"));
    let block = |ours: &str| format!("<<<<<<< ours\n{ours}=======\n{theirs}>>>>>>> theirs\n");
    let mut marked = false;
    let mut out = String::new();
    for line in front.split_inclusive('\n') {
        if line
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
        {
            out.push_str(&block(line));
            marked = true;
        } else {
            out.push_str(line);
        }
    }
    // Ours removed the key: theirs still has to be seen, never silently dropped.
    if !marked {
        out.push_str(&block(""));
    }
    out
}

fn markers(ours: &str, theirs: &str) -> String {
    let nl = |s: &str| {
        if s.is_empty() || s.ends_with('\n') {
            String::new()
        } else {
            String::from("\n")
        }
    };
    format!(
        "<<<<<<< ours\n{ours}{}=======\n{theirs}{}>>>>>>> theirs\n",
        nl(ours),
        nl(theirs)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "+++
title = \"Export misses credit notes\"
kind = \"bug\"
priority = \"p1\"
labels = [\"export\"]
depends_on = []
created = 2026-10-14T09:00:00Z
status = { state = \"todo\" }
+++

## Context
Missing after the 1st.

## Log
- 2026-10-14T09:05:00Z human:muxa: filed
";

    fn parses(text: &str) -> bool {
        format::parse("app-k3x9".parse().unwrap(), text).is_ok()
    }

    /// Two branches change different fields and both append logs: everything is kept.
    #[test]
    fn different_fields_and_both_logs_merge_cleanly() {
        let ours = BASE
            .replace("status = { state = \"todo\" }", "status = { state = \"doing\", by = \"agent:a\", since = 2026-10-14T10:00:00Z }")
            .replace("- 2026-10-14T09:05:00Z human:muxa: filed\n", "- 2026-10-14T09:05:00Z human:muxa: filed\n- 2026-10-14T10:30:00Z agent:a: reproduced\n");
        let theirs = BASE
            .replace("priority = \"p1\"", "priority = \"p0\"")
            .replace("labels = [\"export\"]", "labels = [\"export\", \"urgent\"]")
            .replace("- 2026-10-14T09:05:00Z human:muxa: filed\n", "- 2026-10-14T09:05:00Z human:muxa: filed\n- 2026-10-14T10:10:00Z agent:b: bumped priority\n");
        let Merged::Clean(text) = merge(BASE, &ours, &theirs) else {
            panic!("expected a clean merge")
        };
        assert!(parses(&text), "{text}");
        assert!(text.contains("state = \"doing\""), "{text}");
        assert!(text.contains("priority = \"p0\""), "{text}");
        assert!(text.contains("labels = [\"export\", \"urgent\"]"), "{text}");
        assert!(text.ends_with("filed\n- 2026-10-14T10:10:00Z agent:b: bumped priority\n- 2026-10-14T10:30:00Z agent:a: reproduced\n"), "{text}");
    }

    /// A first log on one branch and a description fix on the other don't overlap.
    #[test]
    fn first_log_and_a_prose_edit_merge_cleanly() {
        let base = BASE.replace("\n## Log\n- 2026-10-14T09:05:00Z human:muxa: filed\n", "");
        let ours = crate::log::append_log(&base, "- 2026-10-14T10:00:00Z agent:a: first note");
        let theirs = base.replace(
            "Missing after the 1st.",
            "Missing after the 1st of each month.",
        );
        let Merged::Clean(text) = merge(&base, &ours, &theirs) else {
            panic!("expected a clean merge")
        };
        assert!(
            text.contains("of each month") && text.contains("first note"),
            "{text}"
        );
        assert!(parses(&text), "{text}");
    }

    /// Ours deleted a key that theirs changed: the conflict shows theirs instead of dropping it.
    #[test]
    fn conflict_on_a_key_ours_removed_still_shows_theirs() {
        let ours = BASE.replace("priority = \"p1\"\n", "");
        let theirs = BASE.replace("priority = \"p1\"", "priority = \"p0\"");
        let Merged::Conflicted(text) = merge(BASE, &ours, &theirs) else {
            panic!("expected a conflict")
        };
        assert!(
            text.contains("<<<<<<< ours\n=======\npriority = \"p0\"\n>>>>>>> theirs\n"),
            "{text}"
        );
    }

    /// Both change the status differently: one marked line, and either side is valid TOML.
    #[test]
    fn same_field_changed_differently_is_one_marked_line() {
        let ours = BASE.replace(
            "status = { state = \"todo\" }",
            "status = { state = \"done\", at = 2026-10-14T11:00:00Z, resolution = \"completed\" }",
        );
        let theirs = BASE.replace(
            "status = { state = \"todo\" }",
            "status = { state = \"blocked\", reason = \"waiting\", since = 2026-10-14T11:00:00Z }",
        );
        let Merged::Conflicted(text) = merge(BASE, &ours, &theirs) else {
            panic!("expected a conflict")
        };
        let keep = |drop_from: &str, drop_to: &str| -> String {
            let mut skipping = false;
            text.lines()
                .filter(|l| {
                    if l.starts_with(drop_from) {
                        skipping = true;
                    }
                    let keep = !skipping
                        && !l.starts_with("<<<<<<<")
                        && !l.starts_with(">>>>>>>")
                        && !l.starts_with("=======");
                    if l.starts_with(drop_to) {
                        skipping = false;
                    }
                    keep
                })
                .flat_map(|l| [l, "\n"])
                .collect()
        };
        assert!(parses(&keep("=======", ">>>>>>>")), "ours side: {text}");
        assert!(parses(&keep("<<<<<<<", "=======")), "theirs side: {text}");
    }
}
