//! Leases stop two agents from starting the same ticket, even in different
//! worktrees: they live in the git directory every worktree shares and are never committed.

use std::io::{self, Write as _};
use std::process;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use tk_core::{Actor, TicketId, Timestamp};

use crate::error::StoreError;

/// `git rev-parse --git-common-dir` for `repo`: the same for every worktree of
/// one repository. `None` outside git, or when git isn't installed.
///
/// # Errors
/// Running git failed for a reason other than "not a repository".
pub(crate) fn git_common_dir(repo: &Utf8Path) -> Result<Option<Utf8PathBuf>, StoreError> {
    let output = process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(repo)
        .output();
    let output = match output {
        Ok(output) => output,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if !output.status.success() {
        return Ok(None);
    }
    let common = String::from_utf8(output.stdout)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(Utf8PathBuf::from(common.trim())))
}

/// The lease directory of one repository.
#[derive(Clone, Debug)]
pub struct Leases {
    dir: Utf8PathBuf,
}

/// Proof that the ticket is held, and a guard: dropping it releases the lease,
/// so a start that fails before its save can't leave one behind. No public
/// constructor: only [`Leases::acquire`] returns one, and [`Lease::keep`] is
/// how a successful start commits it.
#[must_use]
#[derive(Debug)]
pub struct Lease {
    leases: Leases,
    id: TicketId,
    kept: bool,
}

/// Someone else holds a live lease.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[error("{id} is claimed by {by} until {until}")]
pub struct Claimed {
    pub id: TicketId,
    pub by: Actor,
    pub until: Timestamp,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaseFile {
    actor: Actor,
    until: Timestamp,
}

impl Leases {
    /// `<git common dir>/tk/leases`: the same directory for every worktree.
    pub(crate) fn in_git_dir(common: &Utf8Path) -> Self {
        Self::at(common.join("tk").join("leases"))
    }

    pub(crate) fn at(dir: Utf8PathBuf) -> Self {
        Self { dir }
    }

    /// Takes the lease for `id`, or renews it if `actor` already holds it.
    /// When two agents race, the filesystem lets exactly one create the file.
    ///
    /// # Errors
    /// [`Claimed`] when another actor holds a live lease, or an I/O error.
    pub fn acquire(
        &self,
        id: &TicketId,
        actor: &Actor,
        now: Timestamp,
        until: Timestamp,
    ) -> Result<Lease, AcquireError> {
        fs_err::create_dir_all(&self.dir)?;
        let file = LeaseFile {
            actor: actor.clone(),
            until,
        };
        if self.publish_new(id, &file)? {
            return Ok(self.granted(id));
        }
        // A lease exists. Decide under the lock every worktree shares; every
        // change to an existing lease (takeover, renew, release) happens under it.
        let _lock = self.lock()?;
        match self.read(id)? {
            Some(held) if held.until > now && &held.actor != actor => Err(claimed(id, held)),
            // Ours to renew, or expired or unreadable: replace it.
            Some(_) => {
                self.publish_over(id, &file)?;
                Ok(self.granted(id))
            }
            // Released meanwhile, and maybe being created right now without the
            // lock: only create, never overwrite.
            None if self.publish_new(id, &file)? => Ok(self.granted(id)),
            None => match self.read(id)? {
                Some(held) if &held.actor != actor => Err(claimed(id, held)),
                Some(_) | None => Ok(self.granted(id)),
            },
        }
    }

    /// The guard for a lease that is now ours. Made only once the lease is held:
    /// a guard dropped on a refusal would delete the holder's lease.
    fn granted(&self, id: &TicketId) -> Lease {
        Lease {
            leases: self.clone(),
            id: id.clone(),
            kept: false,
        }
    }

    /// Who holds a live lease on `id`, if anyone.
    ///
    /// # Errors
    /// An I/O error reading the lease.
    pub fn holder(&self, id: &TicketId, now: Timestamp) -> Result<Option<Actor>, StoreError> {
        Ok(self
            .read(id)?
            .filter(|held| held.until > now)
            .map(|held| held.actor))
    }

    /// Extends `actor`'s lease on `id`, if it holds one.
    ///
    /// # Errors
    /// An I/O error.
    pub fn renew(&self, id: &TicketId, actor: &Actor, until: Timestamp) -> Result<(), StoreError> {
        let _lock = self.lock()?;
        match self.read(id)? {
            Some(held) if &held.actor == actor => self.publish_over(
                id,
                &LeaseFile {
                    actor: actor.clone(),
                    until,
                },
            ),
            Some(_) | None => Ok(()),
        }
    }

    /// Deletes the lease on `id`, whoever holds it: the ticket isn't being worked on any more.
    ///
    /// # Errors
    /// An I/O error other than the lease already being gone.
    pub fn release(&self, id: &TicketId) -> Result<(), StoreError> {
        let _lock = self.lock()?;
        match fs_err::remove_file(self.path(id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            Ok(()) | Err(_) => Ok(()),
        }
    }

    fn path(&self, id: &TicketId) -> Utf8PathBuf {
        self.dir.join(format!("{id}.toml"))
    }

    /// The lock every worktree shares, held until the returned file drops.
    fn lock(&self) -> Result<fs_err::File, StoreError> {
        fs_err::create_dir_all(&self.dir)?;
        let lock = fs_err::File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join(".lock"))?;
        lock.file().lock()?;
        Ok(lock)
    }

    /// A lease file that doesn't parse counts as no lease: files only appear
    /// whole, by rename, so it can't be one still being written.
    fn read(&self, id: &TicketId) -> Result<Option<LeaseFile>, StoreError> {
        match fs_err::read_to_string(self.path(id)) {
            Ok(text) => Ok(toml::from_str(&text).ok()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Publishes the lease only if none exists. Returns whether it did.
    fn publish_new(&self, id: &TicketId, file: &LeaseFile) -> Result<bool, StoreError> {
        match self.temp(file)?.persist_noclobber(self.path(id)) {
            Ok(_) => Ok(true),
            Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e.error.into()),
        }
    }

    fn publish_over(&self, id: &TicketId, file: &LeaseFile) -> Result<(), StoreError> {
        self.temp(file)?
            .persist(self.path(id))
            .map_err(|e| e.error)?;
        Ok(())
    }

    fn temp(&self, file: &LeaseFile) -> Result<tempfile::NamedTempFile, StoreError> {
        let text =
            toml::to_string(file).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut tmp = tempfile::NamedTempFile::new_in(&self.dir)?;
        tmp.write_all(text.as_bytes())?;
        tmp.as_file().sync_all()?;
        Ok(tmp)
    }
}

fn claimed(id: &TicketId, held: LeaseFile) -> AcquireError {
    AcquireError::Claimed(Claimed {
        id: id.clone(),
        by: held.actor,
        until: held.until,
    })
}

impl Lease {
    /// The start this lease was taken for is saved: keep the lease.
    pub fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if !self.kept && self.leases.release(&self.id).is_err() {
            // Nothing more to do here: an unreleased lease expires on its own.
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AcquireError {
    #[error(transparent)]
    Claimed(Claimed),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<io::Error> for AcquireError {
    fn from(e: io::Error) -> Self {
        Self::Store(e.into())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::*;
    use tk_core::Name;

    /// Needs the real OS: two threads race for one ticket and exactly one wins.
    #[test]
    fn two_threads_claim_one_ticket_and_exactly_one_gets_the_lease() {
        for _ in 0..50 {
            let dir = tempfile::tempdir().unwrap();
            let leases = Leases::at(Utf8Path::from_path(dir.path()).unwrap().to_owned());
            let id: TicketId = "app-k3x9".parse().unwrap();
            let now = Timestamp::UNIX_EPOCH;
            let until = Timestamp::from_second(7200).unwrap();
            let barrier = Arc::new(Barrier::new(2));
            let racers: Vec<_> = ["a", "b"]
                .into_iter()
                .map(|name| {
                    let (leases, id, barrier) = (leases.clone(), id.clone(), Arc::clone(&barrier));
                    thread::spawn(move || {
                        let actor = Actor::Agent(Name::try_new(name.to_owned()).unwrap());
                        barrier.wait();
                        leases
                            .acquire(&id, &actor, now, until)
                            .map(Lease::keep)
                            .is_ok()
                    })
                })
                .collect();
            let wins = racers
                .into_iter()
                .map(|r| r.join().unwrap())
                .filter(|&won| won)
                .count();
            assert_eq!(wins, 1);
        }
    }

    /// A refused acquire must not release the lease it was refused by.
    #[test]
    fn a_refused_claim_leaves_the_holders_lease_alone() {
        let dir = tempfile::tempdir().unwrap();
        let leases = Leases::at(Utf8Path::from_path(dir.path()).unwrap().to_owned());
        let id: TicketId = "app-k3x9".parse().unwrap();
        let (now, until) = (Timestamp::UNIX_EPOCH, Timestamp::from_second(7200).unwrap());
        let agent = |name: &str| Actor::Agent(Name::try_new(name.to_owned()).unwrap());
        leases.acquire(&id, &agent("a"), now, until).unwrap().keep();
        assert!(leases.acquire(&id, &agent("b"), now, until).is_err());
        assert_eq!(leases.holder(&id, now).unwrap(), Some(agent("a")));
    }
}
