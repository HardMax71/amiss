use std::fmt;
use std::path::Path;

use amiss_git::{GitLimits, GitResources, ObjectKind, Repository, parse_commit};
use amiss_wire::model::{ObjectFormat, Oid};

use crate::{RunRequest, check_binding};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcquireError {
    PlanBinding,
    RepositoryObjects,
    RepositoryTree,
    ActionObjects,
    ActionTree,
}

impl fmt::Display for AcquireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PlanBinding => "the check plan changed after authentication",
            Self::RepositoryObjects => "the acquired repository objects cannot be trusted",
            Self::RepositoryTree => "an acquired repository commit names another tree",
            Self::ActionObjects => "the acquired action objects cannot be trusted",
            Self::ActionTree => "the acquired action commit names another tree",
        })
    }
}

impl std::error::Error for AcquireError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcquiredRoots<'a> {
    pub repository: &'a Path,
    pub action: &'a Path,
}

/// Verifies that independently acquired object stores contain the authenticated
/// repository commits and the controller-pinned action commit at their exact trees.
///
/// # Errors
///
/// The check plan changed, an object store cannot prove an exact commit, or a
/// commit names a tree other than its authenticated or pinned tree.
pub fn verify_acquired(request: &RunRequest, roots: AcquiredRoots<'_>) -> Result<(), AcquireError> {
    check_binding(&request.plan)
        .map_err(|_defect| AcquireError::PlanBinding)
        .and_then(|binding| {
            (binding == request.check)
                .then_some(())
                .ok_or(AcquireError::PlanBinding)
        })?;

    verify_commits(
        roots.repository,
        request.run.object_format,
        [
            (&request.run.commits.base, &request.run.trees.base),
            (&request.run.commits.candidate, &request.run.trees.candidate),
        ],
        AcquireError::RepositoryObjects,
        AcquireError::RepositoryTree,
    )?;
    verify_commits(
        roots.action,
        request.plan.execution.action_object_format,
        [(
            &request.plan.execution.action_commit_oid,
            &request.plan.execution.action_tree_oid,
        )],
        AcquireError::ActionObjects,
        AcquireError::ActionTree,
    )
}

fn verify_commits<const N: usize>(
    root: &Path,
    object_format: ObjectFormat,
    expected: [(&Oid, &Oid); N],
    object_error: AcquireError,
    tree_error: AcquireError,
) -> Result<(), AcquireError> {
    let repository = Repository::open(root, object_format).map_err(|_defect| object_error)?;
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    expected.into_iter().try_for_each(|(commit_oid, tree_oid)| {
        repository
            .read_expected(&mut resources, commit_oid, ObjectKind::Commit)
            .map_err(|_defect| object_error)
            .and_then(|object| {
                parse_commit(object_format, &object.body).map_err(|_defect| object_error)
            })
            .and_then(|commit| (commit.tree == *tree_oid).then_some(()).ok_or(tree_error))
    })
}
