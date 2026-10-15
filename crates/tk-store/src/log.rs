//! Appending to the `## Log` section of a ticket body. The rest of the body is never touched.

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

/// Adds `entry` as the last line of the first `## Log` section, creating the
/// section at the end if there is none. A heading inside a code fence never matches.
#[must_use]
pub fn append_log(body: &str, entry: &str) -> String {
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

/// The body without its log section, for comparing everything else.
pub(crate) fn without_log(body: &str) -> String {
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
