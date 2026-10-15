//! The `## Log` section of a ticket body: its line format, and appending to it.
//! The rest of the body is never touched.

use std::fmt;

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use tk_core::{Actor, EntryKind, NonEmpty, Timestamp};

/// One `## Log` line: `- <at> <by>: <text>` for a note, `- <at> <by> <kind>: <text>`
/// otherwise. An actor's name can't contain `:`, so the two shapes can't be confused,
/// and the kind is never read from the text.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LogEntry {
    pub at: Timestamp,
    pub by: Actor,
    pub kind: EntryKind,
    pub text: NonEmpty,
}

impl LogEntry {
    /// The inverse of `Display`, or `None` for a line in any other shape.
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let (at, rest) = line_stamp(line)?;
        let (head, rest) = rest.split_once(' ')?;
        let (by, kind, text) = if let Some(by) = head.strip_suffix(':') {
            (by, EntryKind::Note, rest)
        } else {
            let (kind, text) = rest.split_once(' ')?;
            (head, kind.strip_suffix(':')?.parse().ok()?, text)
        };
        Some(Self {
            at,
            by: by.parse().ok()?,
            kind,
            text: NonEmpty::try_new(text.to_owned()).ok()?,
        })
    }
}

impl fmt::Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { at, by, kind, text } = self;
        match kind {
            EntryKind::Note => write!(f, "- {at} {by}: {text}"),
            EntryKind::RuledOut | EntryKind::Decided | EntryKind::Next => {
                write!(f, "- {at} {by} {kind}: {text}")
            }
        }
    }
}

/// The timestamp that starts a `- <at> ...` line, and the rest of the line.
/// Lines tk can't otherwise read still have one, which is what merging sorts by.
pub(crate) fn line_stamp(line: &str) -> Option<(Timestamp, &str)> {
    let (at, rest) = line.strip_prefix("- ")?.split_once(' ')?;
    Some((at.parse().ok()?, rest))
}

/// A log line as tk reads it. People write under `## Log` too, so a line in
/// another shape is kept, without its bullet, rather than refused.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LogLine<'a> {
    Entry(LogEntry),
    Raw(&'a str),
}

/// Every non-blank line of the log section, parsed where it can be.
#[must_use]
pub fn log_lines(body: &str) -> Vec<LogLine<'_>> {
    log_entries(body)
        .into_iter()
        .map(|line| {
            LogEntry::parse(line).map_or_else(
                || LogLine::Raw(line.strip_prefix("- ").unwrap_or(line)),
                LogLine::Entry,
            )
        })
        .collect()
}

/// `body` with `entries` added to the end of its log, in one pass. This and
/// `Display` are the only places a log line is written.
#[must_use]
pub fn append_entries(body: &str, entries: &[LogEntry]) -> String {
    if entries.is_empty() {
        return body.to_owned();
    }
    let lines: Vec<String> = entries.iter().map(ToString::to_string).collect();
    append_log(body, &lines.join("\n"))
}

/// Adds `entry` as the last line of the first `## Log` section, creating the
/// section at the end if there is none. A heading inside a code fence never matches.
#[must_use]
pub(crate) fn append_log(body: &str, entry: &str) -> String {
    let Some((head, tail)) = log_section(body).and_then(|s| body.split_at_checked(s.end)) else {
        let text = body.trim_end();
        let gap = if text.is_empty() { "" } else { "\n\n" };
        return format!("{text}{gap}## Log\n{entry}\n");
    };
    // Insert after the section's last non-blank line, keeping the blank lines that follow it.
    let kept = head.trim_end();
    let blank = head.get(kept.len()..).unwrap_or_default();
    format!("{kept}\n{entry}{blank}{tail}")
}

/// Where the first `## Log` section is: its heading starts at `start`, its
/// entries start at `content`, and it ends at the next heading of level 1 or 2
/// or the end of the body.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LogSection {
    pub start: usize,
    pub content: usize,
    pub end: usize,
}

pub(crate) fn log_section(body: &str) -> Option<LogSection> {
    let mut found: Option<usize> = None;
    let mut heading: Option<(usize, String)> = None;
    for (event, range) in Parser::new(body).into_offset_iter() {
        if let Event::Start(Tag::Heading { level, .. }) = &event {
            if let Some(start) = found
                && *level <= HeadingLevel::H2
            {
                return Some(LogSection {
                    start,
                    content: line_end(body, start),
                    end: range.start,
                });
            }
            heading = Some((range.start, String::new()));
        } else if let Event::Text(text) | Event::Code(text) = &event
            && let Some((_, h)) = heading.as_mut()
        {
            h.push_str(text);
        } else if let Event::End(TagEnd::Heading(level)) = event
            && let Some((start, h)) = heading.take()
            && found.is_none()
            && level == HeadingLevel::H2
            && h.trim() == "Log"
        {
            found = Some(start);
        }
    }
    found.map(|start| LogSection {
        start,
        content: line_end(body, start),
        end: body.len(),
    })
}

/// The offset just past the newline ending the line that contains `at`.
fn line_end(text: &str, at: usize) -> usize {
    text.get(at..)
        .and_then(|rest| rest.find('\n'))
        .map_or(text.len(), |i| at + i + 1)
}

/// The non-blank lines of the log section.
pub(crate) fn log_entries(body: &str) -> Vec<&str> {
    log_section(body)
        .and_then(|s| body.get(s.content..s.end))
        .map(|content| {
            content
                .lines()
                .map(str::trim_end)
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// The body without its log section: what people wrote about the ticket.
#[must_use]
pub fn without_log(body: &str) -> String {
    match log_section(body).and_then(|s| Some((body.get(..s.start)?, body.get(s.end..)?))) {
        Some((before, after)) => format!("{before}{after}"),
        None => body.to_owned(),
    }
}

/// `body` with its log entries replaced by `entries`, keeping everything else as written.
pub(crate) fn with_log_entries(body: &str, entries: &[String]) -> String {
    let Some((head, content, tail)) = log_section(body).and_then(|s| {
        Some((
            body.get(..s.content)?,
            body.get(s.content..s.end)?,
            body.get(s.end..)?,
        ))
    }) else {
        return entries
            .iter()
            .fold(body.to_owned(), |body, entry| append_log(&body, entry));
    };
    let blank = content.get(content.trim_end().len()..).unwrap_or_default();
    let blank = blank.strip_prefix('\n').unwrap_or(blank);
    let head = if head.ends_with('\n') {
        head.to_owned()
    } else {
        format!("{head}\n")
    };
    format!("{head}{}\n{blank}{tail}", entries.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::append_log;

    #[test]
    fn appends_inside_the_section_and_ignores_fenced_headings() {
        let body = "## Context\nx\n\n```\n## Log\n```\n\n## Log\n- a\n\n## After\ny\n";
        assert_eq!(
            append_log(body, "- b"),
            "## Context\nx\n\n```\n## Log\n```\n\n## Log\n- a\n- b\n\n## After\ny\n"
        );
        assert_eq!(append_log("", "- a"), "## Log\n- a\n");
        assert_eq!(append_log("text\n", "- a"), "text\n\n## Log\n- a\n");
    }
}
