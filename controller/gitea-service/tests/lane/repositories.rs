use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use amiss_controller::{Acquisition, AcquisitionTarget, OidPair, RunRequest};
use amiss_fixtures::{CommitPair, commit_pair, git};
use amiss_wire::model::{ObjectFormat, Oid};

pub(super) struct Repositories {
    repository: CommitPair,
    action: CommitPair,
}

impl Repositories {
    pub(super) fn new() -> Self {
        Self {
            repository: commit_pair(&[("README.md", "base\n")], &[("README.md", "candidate\n")])
                .unwrap(),
            action: commit_pair(
                &[("release/engine", "first\n")],
                &[("release/engine", "second\n")],
            )
            .unwrap(),
        }
    }

    pub(super) fn commits(&self) -> OidPair {
        OidPair {
            base: oid(&self.repository.base),
            candidate: oid(&self.repository.candidate),
        }
    }

    pub(super) fn trees(&self) -> OidPair {
        OidPair {
            base: tree(&self.repository, &self.repository.base),
            candidate: tree(&self.repository, &self.repository.candidate),
        }
    }

    pub(super) fn action_commit(&self) -> Oid {
        oid(&self.action.candidate)
    }

    pub(super) fn action_tree(&self) -> Oid {
        tree(&self.action, &self.action.candidate)
    }

    pub(super) fn acquisition(&self) -> CopyAcquisition {
        CopyAcquisition {
            repository: self.repository.root().to_path_buf(),
            action: self.action.root().to_path_buf(),
        }
    }
}

pub(super) struct CopyAcquisition {
    repository: PathBuf,
    action: PathBuf,
}

impl Acquisition for CopyAcquisition {
    type Error = io::Error;

    fn acquire(
        &mut self,
        _request: &RunRequest,
        target: AcquisitionTarget<'_>,
    ) -> Result<(), Self::Error> {
        copy_tree(&self.repository, target.repository, &target.cancelled)?;
        copy_tree(&self.action, target.action, &target.cancelled)
    }
}

fn copy_tree(
    source: &Path,
    destination: &Path,
    cancelled: &std::sync::atomic::AtomicBool,
) -> io::Result<()> {
    let mut pending = VecDeque::from([(source.to_path_buf(), destination.to_path_buf())]);
    while let Some((from, to)) = pending.pop_front() {
        if cancelled.load(Ordering::Acquire) {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let target = to.join(entry.file_name());
            if file_type.is_dir() {
                std::fs::create_dir(&target)?;
                pending.push_back((entry.path(), target));
            } else if file_type.is_file() {
                let _bytes = std::fs::copy(entry.path(), target)?;
            } else {
                return Err(io::Error::other("fixture repository contains a link"));
            }
        }
    }
    Ok(())
}

fn oid(raw: &str) -> Oid {
    Oid::new(ObjectFormat::Sha1, raw.to_owned()).unwrap()
}

fn tree(repository: &CommitPair, commit: &str) -> Oid {
    let revision = format!("{commit}^{{tree}}");
    oid(git(repository.root(), &["rev-parse", &revision])
        .unwrap()
        .trim())
}
