use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid};
use serde::{Deserialize, Serialize};

use crate::file_ledger::FileLedgerError;
use crate::{
    OidPair, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, RunIdentity, RunRefs,
};

use super::delivery::StoredChange;
use super::{MaterializeResult, checked};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredProviderRun {
    run_id: String,
    attempt: u64,
    object_format: StoredObjectFormat,
    candidate_commit: String,
}

impl StoredProviderRun {
    pub(in crate::file_ledger::format) fn new(run: &ProviderRunIdentity) -> Self {
        let run_id = run.run_id.as_str().to_owned();
        let attempt = run.attempt.get();
        let object_format = StoredObjectFormat::new(run.object_format);
        let candidate_commit = run.candidate_commit.as_str().to_owned();
        Self {
            run_id,
            attempt,
            object_format,
            candidate_commit,
        }
    }

    pub(in crate::file_ledger::format) fn materialize(
        &self,
    ) -> MaterializeResult<ProviderRunIdentity> {
        let object_format = self.object_format.materialize();
        ProviderRunId::new(self.run_id.clone())
            .zip(ProviderRunAttempt::new(self.attempt))
            .zip(Oid::new(object_format, self.candidate_commit.clone()))
            .and_then(|((run_id, attempt), candidate_commit)| {
                ProviderRunIdentity::new(run_id, attempt, object_format, candidate_commit)
            })
            .ok_or(FileLedgerError::Corrupt)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredRun {
    change: StoredChange,
    refs: StoredRefs,
    object_format: StoredObjectFormat,
    commits: StoredOidPair,
    trees: StoredOidPair,
}

impl StoredRun {
    pub(in crate::file_ledger::format) fn new(run: &RunIdentity) -> Self {
        let (commits, trees) = Self::store_oid_pairs(run);
        Self {
            change: StoredChange::new(&run.change),
            refs: StoredRefs::new(&run.refs),
            object_format: StoredObjectFormat::new(run.object_format),
            commits,
            trees,
        }
    }

    pub(in crate::file_ledger::format) fn materialize(&self) -> MaterializeResult<RunIdentity> {
        let object_format = self.object_format.materialize();
        let (change, refs) = (self.change.materialize()?, self.refs.materialize()?);
        let commits = self.commits.materialize(object_format)?;
        let trees = self.trees.materialize(object_format)?;
        checked(RunIdentity::new(
            change,
            refs,
            object_format,
            commits,
            trees,
        ))
    }

    fn store_oid_pairs(run: &RunIdentity) -> (StoredOidPair, StoredOidPair) {
        (
            StoredOidPair::new(&run.commits),
            StoredOidPair::new(&run.trees),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRefs {
    forge: StoredForge,
    candidate: String,
    target: String,
    default_branch: String,
}

impl StoredRefs {
    fn new(refs: &RunRefs) -> Self {
        Self {
            forge: StoredForge::new(refs.forge),
            candidate: refs.candidate.as_str().to_owned(),
            target: refs.target.as_str().to_owned(),
            default_branch: refs.default_branch.as_str().to_owned(),
        }
    }

    fn materialize(&self) -> MaterializeResult<RunRefs> {
        Ok(RunRefs {
            forge: self.forge.materialize(),
            candidate: branch(&self.candidate)?,
            target: branch(&self.target)?,
            default_branch: branch(&self.default_branch)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredOidPair {
    base: String,
    candidate: String,
}

impl StoredOidPair {
    fn new(pair: &OidPair) -> Self {
        Self {
            base: pair.base.as_str().to_owned(),
            candidate: pair.candidate.as_str().to_owned(),
        }
    }

    fn materialize(&self, object_format: ObjectFormat) -> MaterializeResult<OidPair> {
        Ok(OidPair {
            base: checked(Oid::new(object_format, self.base.clone()))?,
            candidate: checked(Oid::new(object_format, self.candidate.clone()))?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredObjectFormat {
    Sha1,
    Sha256,
}

impl StoredObjectFormat {
    const fn new(object_format: ObjectFormat) -> Self {
        match object_format {
            ObjectFormat::Sha1 => Self::Sha1,
            ObjectFormat::Sha256 => Self::Sha256,
        }
    }

    const fn materialize(self) -> ObjectFormat {
        match self {
            Self::Sha1 => ObjectFormat::Sha1,
            Self::Sha256 => ObjectFormat::Sha256,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredForge {
    Github,
    Gitlab,
    Gitea,
}

impl StoredForge {
    const fn new(forge: ForgeDialect) -> Self {
        match forge {
            ForgeDialect::Github => Self::Github,
            ForgeDialect::Gitlab => Self::Gitlab,
            ForgeDialect::Gitea => Self::Gitea,
        }
    }

    const fn materialize(self) -> ForgeDialect {
        match self {
            Self::Github => ForgeDialect::Github,
            Self::Gitlab => ForgeDialect::Gitlab,
            Self::Gitea => ForgeDialect::Gitea,
        }
    }
}

fn branch(raw: &str) -> MaterializeResult<BranchRef> {
    checked(BranchRef::new(raw.to_owned()))
}
