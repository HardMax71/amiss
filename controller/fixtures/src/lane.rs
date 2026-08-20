use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use amiss_controller::{Acquisition, AcquisitionTarget, OidPair, RunRequest};
use amiss_fixtures::{CommitPair, commit_pair};
use amiss_wire::controls::{ExecutionConstraintDescriptor, ExecutionConstraintInput};
use amiss_wire::digest::Digest;
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};

/// The checked repository and the action repository a provider lane acquires,
/// each with a base and a candidate commit.
pub struct Repositories {
    repository: CommitPair,
    action: CommitPair,
    pub commits: OidPair,
    pub trees: OidPair,
    pub action_commit: Oid,
    pub action_tree: Oid,
}

impl Repositories {
    /// # Errors
    ///
    /// Any git or filesystem failure, as plain I/O errors.
    pub fn new() -> io::Result<Self> {
        let repository = commit_pair(&[("README.md", "base\n")], &[("README.md", "candidate\n")])?;
        let action = commit_pair(
            &[("release/engine", "first\n")],
            &[("release/engine", "second\n")],
        )?;
        let commits = OidPair {
            base: oid(&repository.base)?,
            candidate: oid(&repository.candidate)?,
        };
        let trees = OidPair {
            base: oid(&repository.base_tree)?,
            candidate: oid(&repository.candidate_tree)?,
        };
        let action_commit = oid(&action.candidate)?;
        let action_tree = oid(&action.candidate_tree)?;
        Ok(Self {
            repository,
            action,
            commits,
            trees,
            action_commit,
            action_tree,
        })
    }

    pub fn acquisition(&self) -> CopyAcquisition {
        CopyAcquisition {
            repository: self.repository.root().to_path_buf(),
            action: self.action.root().to_path_buf(),
        }
    }
}

/// Builds the execution constraint shared by the provider lane fixtures.
///
/// # Errors
///
/// The published template or a generated fixture identity is invalid.
pub fn execution_constraint(
    repositories: &Repositories,
    action_repository: RepositoryIdentity,
    required_status_name: &str,
    bootstrap_digest: Digest,
) -> io::Result<ExecutionConstraintDescriptor> {
    let template = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../../spec/examples/scanner-execution-constraint.json"
    ))
    .map_err(io::Error::other)?;
    ExecutionConstraintDescriptor::new(ExecutionConstraintInput {
        action_repository,
        action_object_format: ObjectFormat::Sha1,
        action_commit_oid: repositories.action_commit.clone(),
        action_tree_oid: repositories.action_tree.clone(),
        manifest_path: template.manifest_path().clone(),
        release_manifest_digest: template.release_manifest_digest(),
        selected_platform: template.selected_platform(),
        required_status_name: required_status_name.to_owned(),
        bootstrap_digest,
    })
    .map_err(io::Error::other)
}

/// Places both fixture trees by copy, so a lane runs without a network.
#[derive(Clone)]
pub struct CopyAcquisition {
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

fn oid(raw: &str) -> io::Result<Oid> {
    Oid::new(ObjectFormat::Sha1, raw.to_owned())
        .ok_or_else(|| io::Error::other("fixture object name is not a SHA-1 oid"))
}
