//! Transactions over one project's `.tickets/` directory.
//!
//! Four mistakes can't be expressed: writing outside a transaction, writing
//! without reading first, writing unvalidated data, and saving one edit twice.

use std::collections::BTreeMap;
use std::io::{self, Write as _};
use std::sync::OnceLock;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use tk_core::{ProjectKey, Ticket, TicketId};

use crate::error::{Conflict, InvalidFile, ParseError, StoreError, UpdateError};
use crate::format::{self, TicketDoc};
use crate::lease::{Leases, git_common_dir};
use crate::version::{Version, VersionHex};

const DIR: &str = ".tickets";
const CONFIG: &str = "project.toml";
const LOCK: &str = ".lock";
const FORMAT: u32 = 1;

/// One project: a repository with a `.tickets/` directory.
#[derive(Debug)]
pub struct Store {
    repo: Utf8PathBuf,
    dir: Utf8PathBuf,
    key: ProjectKey,
    name: String,
    /// `git rev-parse --git-common-dir`, asked once on first use: it costs a process.
    git_dir: OnceLock<Option<Utf8PathBuf>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    key: ProjectKey,
    name: String,
    format: u32,
}

/// The closest directory at or above `start` that holds `.tickets/project.toml`.
#[must_use]
pub fn find_repo(start: &Utf8Path) -> Option<Utf8PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(DIR).join(CONFIG).is_file())
        .map(Utf8Path::to_owned)
}

impl Store {
    /// # Errors
    /// [`StoreError::NoProject`] when `repo` has no `.tickets/project.toml`, or a config or format error.
    pub fn open(repo: &Utf8Path) -> Result<Self, StoreError> {
        let dir = repo.join(DIR);
        let path = dir.join(CONFIG);
        let text = match fs_err::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(StoreError::NoProject(repo.to_owned()));
            }
            Err(e) => return Err(e.into()),
        };
        let config: ProjectFile = toml::from_str(&text).map_err(|e| StoreError::Config {
            path: path.clone(),
            message: e.message().to_owned(),
        })?;
        if config.format != FORMAT {
            return Err(StoreError::Format {
                path,
                found: config.format,
            });
        }
        Ok(Self {
            repo: repo.to_owned(),
            dir,
            key: config.key,
            name: config.name,
            git_dir: OnceLock::new(),
        })
    }

    /// Creates `.tickets/` with its `project.toml`, and a `.gitignore` for the lock file.
    ///
    /// # Errors
    /// [`StoreError::ProjectExists`] if there already is one, or an I/O error.
    pub fn init(repo: &Utf8Path, key: ProjectKey, name: String) -> Result<Self, StoreError> {
        let dir = repo.join(DIR);
        let path = dir.join(CONFIG);
        if path.exists() {
            return Err(StoreError::ProjectExists(repo.to_owned()));
        }
        fs_err::create_dir_all(&dir)?;
        let config = ProjectFile {
            key,
            name,
            format: FORMAT,
        };
        let text = toml::to_string(&config).map_err(|e| StoreError::Config {
            path: path.clone(),
            message: e.to_string(),
        })?;
        fs_err::write(&path, text)?;
        fs_err::write(dir.join(".gitignore"), format!("{LOCK}\n"))?;
        route_merges(repo)?;
        let ProjectFile {
            key,
            name,
            format: _,
        } = config;
        Ok(Self {
            repo: repo.to_owned(),
            dir,
            key,
            name,
            git_dir: OnceLock::new(),
        })
    }

    #[must_use]
    pub fn key(&self) -> &ProjectKey {
        &self.key
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn repo(&self) -> &Utf8Path {
        &self.repo
    }

    /// Holds the project lock for the whole closure. `Tx` can't outlive it,
    /// so every read-check-write sequence runs under one lock.
    ///
    /// # Errors
    /// Whatever the closure returns, or a [`StoreError`] taking the lock.
    pub fn transaction<R, E: From<StoreError>>(
        &self,
        f: impl for<'t> FnOnce(&mut Tx<'t>) -> Result<R, E>,
    ) -> Result<R, E> {
        let lock = fs_err::File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join(LOCK))
            .map_err(StoreError::from)?;
        lock.file().lock().map_err(StoreError::from)?;
        // Released when `lock` drops, after the closure returns.
        f(&mut Tx {
            store: self,
            _lock: &lock,
        })
    }

    /// The git directory every worktree of this repository shares, or `None` outside git.
    /// Two checkouts with the same one are the same project.
    ///
    /// # Errors
    /// Running git failed.
    pub fn git_dir(&self) -> Result<Option<&Utf8Path>, StoreError> {
        if let Some(dir) = self.git_dir.get() {
            return Ok(dir.as_deref());
        }
        let found = git_common_dir(&self.repo)?;
        Ok(self.git_dir.get_or_init(|| found).as_deref())
    }

    /// The leases shared by every worktree of this repository, or `None` outside
    /// git, where one checkout and the store lock are enough.
    ///
    /// # Errors
    /// Running git failed.
    pub fn leases(&self) -> Result<Option<Leases>, StoreError> {
        Ok(self.git_dir()?.map(Leases::in_git_dir))
    }

    /// The `.tickets/` directory, for watching.
    #[must_use]
    pub fn tickets_dir(&self) -> &Utf8Path {
        &self.dir
    }

    /// The file a ticket lives in.
    #[must_use]
    pub fn path(&self, id: &TicketId) -> Utf8PathBuf {
        self.dir.join(format!("{id}.md"))
    }
}

/// Routes ticket files to `tk merge-driver` in the repository's `.gitattributes`.
/// Registering the driver itself is per clone: `git config merge.tk.driver ...`.
pub(crate) fn route_merges(repo: &Utf8Path) -> Result<(), StoreError> {
    // eol=lf: tk writes LF, and a CRLF checkout would turn every line into a change.
    const ROUTE: &str = ".tickets/*.md merge=tk eol=lf";
    let path = repo.join(".gitattributes");
    let text = match fs_err::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    if text.lines().any(|line| line.trim() == ROUTE) {
        return Ok(());
    }
    let gap = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    fs_err::write(&path, format!("{text}{gap}{ROUTE}\n"))?;
    Ok(())
}

/// Access to one project's files while its lock is held.
pub struct Tx<'t> {
    store: &'t Store,
    _lock: &'t fs_err::File,
}

/// Every ticket in the project, plus the files that failed to parse.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub tickets: BTreeMap<TicketId, Loaded>,
    pub invalid: Vec<InvalidFile>,
}

impl Snapshot {
    pub fn tickets(&self) -> impl Iterator<Item = &Ticket> {
        self.tickets.values().map(Loaded::ticket)
    }
}

/// A ticket as read, with the version of the bytes it came from.
#[derive(Clone, Debug)]
pub struct Loaded {
    ticket: Ticket,
    doc: TicketDoc,
    version: Version,
}

/// A change to a loaded ticket, ready to save. `save` consumes it, so it can't be saved twice.
#[derive(Debug)]
pub struct Edited {
    before: Ticket,
    after: Ticket,
    doc: TicketDoc,
    base: Version,
}

#[must_use]
#[derive(Clone, Debug)]
pub struct Saved {
    pub id: TicketId,
    pub version: Version,
}

impl Loaded {
    #[must_use]
    pub fn ticket(&self) -> &Ticket {
        &self.ticket
    }

    #[must_use]
    pub fn version(&self) -> Version {
        self.version
    }

    /// # Errors
    /// [`Conflict`] when the caller expected a different version than the one read.
    pub fn check_version(&self, expected: Option<&VersionHex>) -> Result<(), Conflict> {
        match expected {
            Some(hex) if !self.version.matches(hex) => Err(Conflict {
                id: self.ticket.id().clone(),
                expected: hex.to_string(),
                found: self.version.to_string(),
            }),
            Some(_) | None => Ok(()),
        }
    }

    /// The only way to get an [`Edited`]. The closure owns the ticket, so
    /// tk-core's consuming methods (`transition`, `ready` + `start`, `add_dep`) fit directly.
    ///
    /// # Errors
    /// The closure's error, or [`UpdateError::IdChanged`] if it returned a different ticket.
    pub fn update<E>(
        self,
        f: impl FnOnce(Ticket) -> Result<Ticket, E>,
    ) -> Result<Edited, UpdateError<E>> {
        let Self {
            ticket,
            doc,
            version,
        } = self;
        let after = f(ticket.clone()).map_err(UpdateError::Rejected)?;
        if after.id() != ticket.id() {
            return Err(UpdateError::IdChanged);
        }
        Ok(Edited {
            before: ticket,
            after,
            doc,
            base: version,
        })
    }
}

impl Edited {
    #[must_use]
    pub fn after(&self) -> &Ticket {
        &self.after
    }
}

impl Tx<'_> {
    #[must_use]
    pub fn store(&self) -> &Store {
        self.store
    }

    /// # Errors
    /// An I/O error listing the directory. Files that don't parse go into `invalid`.
    pub fn snapshot(&self) -> Result<Snapshot, StoreError> {
        let mut snapshot = Snapshot::default();
        let mut paths: Vec<Utf8PathBuf> = Vec::new();
        for entry in fs_err::read_dir(&self.store.dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "md") {
                paths.push(crate::workspace::utf8(path)?);
            }
        }
        paths.sort();
        for path in paths {
            match self.read(&path) {
                Ok(loaded) => {
                    snapshot.tickets.insert(loaded.ticket.id().clone(), loaded);
                }
                Err(StoreError::Invalid(invalid)) => snapshot.invalid.push(*invalid),
                Err(e) => return Err(e),
            }
        }
        Ok(snapshot)
    }

    /// # Errors
    /// [`StoreError::NotFound`], [`StoreError::Invalid`] or an I/O error.
    pub fn load(&self, id: &TicketId) -> Result<Loaded, StoreError> {
        if id.project() != &self.store.key {
            return Err(StoreError::WrongProject {
                id: id.clone(),
                key: self.store.key.clone(),
            });
        }
        match self.read(&self.store.path(id)) {
            Err(StoreError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                Err(StoreError::NotFound(id.clone()))
            }
            other => other,
        }
    }

    /// A ticket file's exact text and version, for editing it outside tk.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] or an I/O error.
    pub fn read_text(&self, id: &TicketId) -> Result<(String, Version), StoreError> {
        let path = self.store.path(id);
        let bytes = match fs_err::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(StoreError::NotFound(id.clone()));
            }
            Err(e) => return Err(e.into()),
        };
        let version = Version::of(&bytes);
        let text =
            String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok((text, version))
    }

    /// Writes a new ticket. Fails rather than overwrite an existing file.
    ///
    /// # Errors
    /// [`StoreError::Exists`] when the ID is taken, [`StoreError::Unreadable`] or an I/O error.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "ownership as protocol: the ticket moves in, so one value can't be created twice"
    )]
    pub fn create(&mut self, ticket: Ticket) -> Result<Saved, StoreError> {
        let text = format::render(&ticket);
        check_reads_back(&ticket, &text)?;
        let path = self.store.path(ticket.id());
        // Dropping the error drops the temp file with it.
        self.temp(&text)?.persist_noclobber(&path).map_err(|e| {
            if e.error.kind() == io::ErrorKind::AlreadyExists {
                StoreError::Exists(ticket.id().clone())
            } else {
                e.error.into()
            }
        })?;
        self.sync_dir()?;
        Ok(Saved {
            id: ticket.id().clone(),
            version: Version::of(text.as_bytes()),
        })
    }

    /// Writes only the changed keys, after checking nobody else changed the file since it was read.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] if the file changed, [`StoreError::Unreadable`] or an I/O error.
    pub fn save(&mut self, edit: Edited) -> Result<Saved, StoreError> {
        let Edited {
            before,
            after,
            mut doc,
            base,
        } = edit;
        let path = self.store.path(after.id());
        let on_disk = Version::of(&fs_err::read(&path)?);
        if on_disk != base {
            return Err(StoreError::Conflict(Conflict {
                id: after.id().clone(),
                expected: base.to_string(),
                found: on_disk.to_string(),
            }));
        }
        let text = format::render_update(&mut doc, &before, &after);
        check_reads_back(&after, &text)?;
        self.temp(&text)?.persist(&path).map_err(|e| e.error)?;
        self.sync_dir()?;
        Ok(Saved {
            id: after.id().clone(),
            version: Version::of(text.as_bytes()),
        })
    }

    fn read(&self, path: &Utf8Path) -> Result<Loaded, StoreError> {
        let bytes = fs_err::read(path)?;
        let version = Version::of(&bytes);
        let invalid = |text: Option<String>, error| {
            StoreError::Invalid(Box::new(InvalidFile {
                path: path.to_owned(),
                text,
                error,
            }))
        };
        let id = path
            .file_stem()
            .and_then(|stem| stem.parse::<TicketId>().ok())
            .ok_or_else(|| invalid(None, ParseError::FileName))?;
        if id.project() != &self.store.key {
            return Err(invalid(
                None,
                ParseError::WrongProject {
                    id,
                    key: self.store.key.clone(),
                },
            ));
        }
        let text = String::from_utf8(bytes).map_err(|_| invalid(None, ParseError::NotUtf8))?;
        match format::parse(id, &text) {
            Ok((ticket, doc)) => Ok(Loaded {
                ticket,
                doc,
                version,
            }),
            Err(error) => Err(invalid(Some(text), error)),
        }
    }

    /// A synced temp file in the same directory, so the rename is atomic.
    fn temp(&self, text: &str) -> Result<tempfile::NamedTempFile, StoreError> {
        let mut tmp = tempfile::NamedTempFile::new_in(&self.store.dir)?;
        tmp.write_all(text.as_bytes())?;
        tmp.as_file().sync_all()?;
        Ok(tmp)
    }

    /// Makes the rename itself durable.
    fn sync_dir(&self) -> Result<(), StoreError> {
        fs_err::File::open(&self.store.dir)?.sync_all()?;
        Ok(())
    }
}

/// A file tk can't read back never reaches disk.
fn check_reads_back(ticket: &Ticket, text: &str) -> Result<(), StoreError> {
    match format::parse(ticket.id().clone(), text) {
        Ok((reparsed, _)) if &reparsed == ticket => Ok(()),
        Ok(_) | Err(_) => Err(StoreError::Unreadable(ticket.id().clone())),
    }
}

#[cfg(test)]
mod tests;
