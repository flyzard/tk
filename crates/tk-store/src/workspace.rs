//! `~/.config/tk/workspace.toml`: every project tk knows about, the editor and the default actor.

use std::io;

use camino::{Utf8Path, Utf8PathBuf};
use etcetera::BaseStrategy as _;
use serde::Deserialize;
use tk_core::Actor;
use toml_edit::{Array, DocumentMut, Item, Value};

use crate::error::StoreError;

#[derive(Debug, Default)]
pub struct Workspace {
    pub path: Utf8PathBuf,
    pub projects: Vec<Utf8PathBuf>,
    /// A program and its arguments, run without a shell, like `["code-insiders", "--wait"]`.
    pub editor: Vec<String>,
    pub actor: Option<Actor>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct WorkspaceFile {
    projects: Vec<String>,
    editor: Vec<String>,
    actor: Option<Actor>,
}

impl Workspace {
    /// Found via etcetera, so `XDG_CONFIG_HOME` is respected. A missing file is an empty workspace.
    ///
    /// # Errors
    /// A config error naming the file, or an I/O error.
    pub fn load() -> Result<Self, StoreError> {
        let path = config_path()?;
        let text = match fs_err::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    ..Self::default()
                });
            }
            Err(e) => return Err(e.into()),
        };
        let file: WorkspaceFile = toml::from_str(&text).map_err(|e| StoreError::Config {
            path: path.clone(),
            message: e.message().to_owned(),
        })?;
        let WorkspaceFile {
            projects,
            editor,
            actor,
        } = file;
        let home = home()?;
        let projects = projects.iter().map(|p| expand(p, &home)).collect();
        Ok(Self {
            path,
            projects,
            editor,
            actor,
        })
    }

    /// Adds a repository to `projects`, keeping the rest of the file as written.
    ///
    /// # Errors
    /// A config error naming the file, or an I/O error.
    pub fn add_project(&mut self, repo: &Utf8Path) -> Result<(), StoreError> {
        if self.projects.iter().any(|p| p == repo) {
            return Ok(());
        }
        let text = match fs_err::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        let config = |message: String| StoreError::Config {
            path: self.path.clone(),
            message,
        };
        let mut doc: DocumentMut = text
            .parse()
            .map_err(|e: toml_edit::TomlError| config(e.message().to_owned()))?;
        let home = home()?;
        let entry = repo
            .strip_prefix(&home)
            .map_or_else(|_| repo.to_string(), |rel| format!("~/{rel}"));
        let item = doc
            .entry("projects")
            .or_insert(Item::Value(Value::Array(Array::new())));
        let Some(array) = item.as_array_mut() else {
            return Err(config(String::from("`projects` must be an array of paths")));
        };
        array.push(entry);
        if let Some(parent) = self.path.parent() {
            fs_err::create_dir_all(parent)?;
        }
        fs_err::write(&self.path, doc.to_string())?;
        self.projects.push(repo.to_owned());
        Ok(())
    }
}

fn config_path() -> Result<Utf8PathBuf, StoreError> {
    let strategy = etcetera::choose_base_strategy()
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, e.to_string()))?;
    utf8(strategy.config_dir().join("tk").join("workspace.toml"))
}

fn home() -> Result<Utf8PathBuf, StoreError> {
    utf8(etcetera::home_dir().map_err(|e| io::Error::new(io::ErrorKind::NotFound, e.to_string()))?)
}

pub(crate) fn utf8(path: std::path::PathBuf) -> Result<Utf8PathBuf, StoreError> {
    Utf8PathBuf::from_path_buf(path).map_err(|p| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} isn't a UTF-8 path", p.display()),
        )
        .into()
    })
}

fn expand(path: &str, home: &Utf8Path) -> Utf8PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => Utf8PathBuf::from(path),
    }
}
