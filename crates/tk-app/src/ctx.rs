use std::collections::BTreeMap;
use std::io;

use camino::Utf8Path;
use tk_core::{Actor, ProjectKey, Timestamp};
use tk_store::{Store, Workspace, find_repo};

use crate::error::AppError;

/// Everything `execute` needs from outside: the projects, who is acting, the
/// time and a source of random bytes. The binary builds it; tests can fake it.
pub struct Ctx {
    pub stores: BTreeMap<ProjectKey, Store>,
    /// The project of the current directory, if there is one.
    pub current: Option<ProjectKey>,
    pub actor: Actor,
    pub now: Timestamp,
    pub entropy: Box<dyn FnMut() -> io::Result<[u8; 4]> + Send>,
    /// Problems that didn't stop tk, such as a workspace entry that no longer exists.
    pub warnings: Vec<String>,
}

impl Ctx {
    /// Opens the project around `cwd` and every project in the workspace.
    ///
    /// # Errors
    /// A project that can't be opened, or two projects with the same key.
    pub fn open(
        workspace: &Workspace,
        cwd: &Utf8Path,
        actor: Actor,
        now: Timestamp,
        entropy: Box<dyn FnMut() -> io::Result<[u8; 4]> + Send>,
    ) -> Result<Self, AppError> {
        let mut stores: BTreeMap<ProjectKey, Store> = BTreeMap::new();
        let mut warnings = Vec::new();
        let here = find_repo(cwd);
        // The project you're in must open; it is read first, so it wins over
        // another checkout of the same project listed in the workspace.
        if let Some(repo) = &here {
            let store = Store::open(repo)?;
            stores.insert(store.key().clone(), store);
        }
        for repo in &workspace.projects {
            if here.as_ref() == Some(repo) {
                continue;
            }
            // A stale entry (a moved or deleted repo) mustn't stop work everywhere else.
            let store = match Store::open(repo) {
                Ok(store) => store,
                Err(e) => {
                    warnings.push(format!("skipping {repo} from {}: {e}", workspace.path));
                    continue;
                }
            };
            match stores.get(store.key()) {
                None => {
                    stores.insert(store.key().clone(), store);
                }
                // Another worktree of the same repository: one project, already open.
                Some(seen) if seen.repo() == store.repo() || same_repository(seen, &store)? => {}
                Some(seen) => {
                    return Err(AppError::Config(format!(
                        "{} and {} are different repositories that both use the key {}",
                        seen.repo(),
                        store.repo(),
                        store.key()
                    )));
                }
            }
        }
        let current = here.and_then(|repo| {
            stores
                .values()
                .find(|s| s.repo() == repo)
                .map(|s| s.key().clone())
        });
        Ok(Self {
            stores,
            current,
            actor,
            now,
            entropy,
            warnings,
        })
    }

    /// # Errors
    /// [`AppError::UnknownProject`] when no project has this key.
    pub fn store(&self, key: &ProjectKey) -> Result<&Store, AppError> {
        self.stores
            .get(key)
            .ok_or_else(|| AppError::UnknownProject(key.clone()))
    }

    /// The given project, or the current one.
    ///
    /// # Errors
    /// [`AppError::NoProject`] when neither exists.
    pub fn project(&self, key: Option<ProjectKey>) -> Result<ProjectKey, AppError> {
        key.or_else(|| self.current.clone())
            .ok_or(AppError::NoProject)
    }
}

/// Whether two checkouts are worktrees of one git repository.
fn same_repository(a: &Store, b: &Store) -> Result<bool, AppError> {
    Ok(match (a.git_dir()?, b.git_dir()?) {
        (Some(a), Some(b)) => a == b,
        (Some(_) | None, None) | (None, Some(_)) => false,
    })
}
