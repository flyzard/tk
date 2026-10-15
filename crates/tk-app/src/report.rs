//! Errors in ticket files, pointing at the line, the same way for `tk lint` and every other command.

use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};
use serde::Serialize;
use tk_core::GraphProblem;
use tk_store::InvalidFile;

/// One problem as JSON: where it is and what is wrong.
#[derive(Serialize, Debug)]
pub struct Finding {
    pub path: Option<String>,
    /// 1-based line in the file, when known.
    pub line: Option<usize>,
    pub message: String,
}

impl From<&InvalidFile> for Finding {
    fn from(f: &InvalidFile) -> Self {
        let line = f
            .text
            .as_deref()
            .zip(f.error.span())
            .and_then(|(text, span)| text.get(..span.start))
            .map(|before| before.matches('\n').count() + 1);
        Self {
            path: Some(f.path.to_string()),
            line,
            message: f.error.to_string(),
        }
    }
}

impl From<&GraphProblem> for Finding {
    fn from(p: &GraphProblem) -> Self {
        Self {
            path: None,
            line: None,
            message: p.to_string(),
        }
    }
}

/// Renders a file error with the offending line underlined, when the file and position are known.
#[must_use]
pub fn render_invalid(f: &InvalidFile, styled: bool) -> String {
    let renderer = if styled {
        Renderer::styled()
    } else {
        Renderer::plain()
    };
    let title = f.error.to_string();
    let path = f.path.as_str();
    let snippet = match (f.text.as_deref(), f.error.span()) {
        (Some(text), Some(span)) => Snippet::source(text)
            .path(path)
            .annotation(AnnotationKind::Primary.span(span)),
        (Some(_) | None, None) | (None, Some(_)) => Snippet::source("").path(path),
    };
    renderer.render(&[Level::ERROR.primary_title(&title).element(snippet)])
}
