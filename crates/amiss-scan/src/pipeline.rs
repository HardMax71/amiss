use amiss_git::{GitResources, ObjectKind, Repository, parse_commit};
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::{EngineProvenance, ErrorDetail};

use crate::Error;
use crate::correlate::{Observation, Side, correlate};
use crate::discovery::{DocumentStatus, SnapshotDiscovery, discover};
use crate::observe::occurrence_id;
use crate::report::{Built, Setup, SnapshotIdentity, construct, construct_incomplete};
use crate::resolve::resolve;
use crate::resolve::{GithubContext, TargetCache};
use crate::resources::{ScanLimits, ScanResources};

/// One side's full evaluation: discovery, then every scanned occurrence
/// resolved against this same snapshot.
struct Evaluated {
    identity: SnapshotIdentity,
    discovery: SnapshotDiscovery,
    side: Side,
}

const fn format_str(object_format: ObjectFormat) -> &'static str {
    match object_format {
        ObjectFormat::Sha1 => "sha1",
        ObjectFormat::Sha256 => "sha256",
    }
}

fn detail(error: &Error, path: Option<&str>) -> ErrorDetail {
    let resource = match error {
        Error::ResourceLimit {
            resource,
            configured_limit,
            observed_lower_bound,
        } => Some((*resource, *configured_limit, *observed_lower_bound)),
        Error::Parse(_) | Error::Git(_) | Error::UnrepresentablePath | Error::Internal => None,
    };
    ErrorDetail {
        code: error.code(),
        path: path.map(str::to_owned),
        resource,
    }
}

fn evaluate_side(
    repo: &Repository,
    git_resources: &mut GitResources,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    commit_oid: &Oid,
) -> Result<(Evaluated, Vec<ErrorDetail>), ErrorDetail> {
    let commit_object = repo
        .read_expected(git_resources, commit_oid, ObjectKind::Commit)
        .map_err(|defect| detail(&Error::from(defect), None))?;
    let commit = parse_commit(repo.object_format(), &commit_object.body)
        .map_err(|defect| detail(&Error::from(defect), None))?;
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let discovery = discover(repo, git_resources, &mut scan_resources, &commit.tree)
        .map_err(|defect| detail(&defect, None))?;

    let mut failures: Vec<ErrorDetail> = discovery
        .path_defects
        .iter()
        .map(|defect| detail(defect, None))
        .collect();
    let mut cache = TargetCache::default();
    let mut observations: Vec<Observation> = Vec::new();
    let mut documents = std::collections::BTreeMap::new();
    for record in &discovery.documents {
        if let Some(raw) = record.raw_digest {
            documents.insert(record.path.clone(), (record.mode, raw));
        }
        match &record.status {
            DocumentStatus::Failed(defect) => {
                failures.push(detail(defect, Some(&record.path)));
            }
            DocumentStatus::ExcludedBuiltIn | DocumentStatus::Unsupported(_) => {}
            DocumentStatus::Scanned(scanned) => {
                for occurrence in &scanned.occurrences {
                    let (intent, resolution) = resolve(
                        repo,
                        git_resources,
                        &mut scan_resources,
                        &mut cache,
                        &discovery,
                        github,
                        &record.path,
                        occurrence.occurrence.construct.is_image(),
                        &occurrence.occurrence.semantic_destination,
                    )
                    .map_err(|defect| detail(&defect, Some(&record.path)))?;
                    observations.push(Observation {
                        id: occurrence_id(
                            engine,
                            record.classification.adapter(),
                            &record.path,
                            occurrence,
                            &intent,
                        ),
                        document: record.path.clone(),
                        span: occurrence.occurrence.span,
                        display: occurrence.display,
                        block_kind: occurrence.occurrence.block_kind,
                        node_path: occurrence.occurrence.node_path.clone(),
                        adapter: record.classification.adapter(),
                        construct: occurrence.occurrence.construct,
                        intent,
                        raw_destination_digest: occurrence.raw_destination_digest,
                        projection_digest: occurrence.projection_digest,
                        resolution,
                    });
                }
            }
        }
    }
    let identity = SnapshotIdentity {
        object_format: format_str(repo.object_format()),
        commit_oid: commit_oid.as_str().to_owned(),
        tree_oid: commit.tree.as_str().to_owned(),
    };
    Ok((
        Evaluated {
            identity,
            discovery,
            side: Side {
                observations,
                documents,
            },
        },
        failures,
    ))
}

/// The complete commit-pair run: both sides, correlation, and construction.
/// Any accumulated typed error makes the run incomplete with every safely
/// established row retained; the report is emitted either way.
#[must_use]
pub fn commit_pair(
    repo: &Repository,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    setup_shell: &SetupShell,
    base_oid: &Oid,
    candidate_oid: &Oid,
) -> Built {
    let mut git_resources = GitResources::new(amiss_git::GitLimits::CONTRACT);
    let incomplete =
        |setup: &Setup, details: Vec<ErrorDetail>| construct_incomplete(setup, &details);
    let placeholder = |oid: &Oid| SnapshotIdentity {
        object_format: format_str(repo.object_format()),
        commit_oid: oid.as_str().to_owned(),
        tree_oid: oid.as_str().to_owned(),
    };

    let base = evaluate_side(repo, &mut git_resources, engine, github, base_oid);
    let candidate = evaluate_side(repo, &mut git_resources, engine, github, candidate_oid);
    match (base, candidate) {
        (Ok((base, base_failures)), Ok((candidate, candidate_failures))) => {
            let setup = setup_shell.with(base.identity.clone(), candidate.identity.clone());
            let mut failures = base_failures;
            failures.extend(candidate_failures);
            if !failures.is_empty() {
                return incomplete(&setup, failures);
            }
            match correlate(&base.side, &candidate.side) {
                Ok(comparisons) => {
                    construct(&setup, &base.discovery, &candidate.discovery, &comparisons)
                }
                Err(defect) => incomplete(&setup, vec![detail(&defect, None)]),
            }
        }
        (Err(defect), Ok((candidate, _))) => {
            let setup = setup_shell.with(placeholder(base_oid), candidate.identity.clone());
            incomplete(&setup, vec![defect])
        }
        (Ok((base, _)), Err(defect)) => {
            let setup = setup_shell.with(base.identity.clone(), placeholder(candidate_oid));
            incomplete(&setup, vec![defect])
        }
        (Err(base_defect), Err(candidate_defect)) => {
            let setup = setup_shell.with(placeholder(base_oid), placeholder(candidate_oid));
            incomplete(&setup, vec![base_defect, candidate_defect])
        }
    }
}

/// Everything of the run identity except the two snapshot identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupShell {
    pub engine: EngineProvenance,
    pub enforce: bool,
    pub repository: Option<(String, String)>,
    pub candidate_ref: Option<String>,
    pub default_branch_ref: Option<String>,
}

impl SetupShell {
    fn with(&self, base: SnapshotIdentity, candidate: SnapshotIdentity) -> Setup {
        Setup {
            engine: self.engine.clone(),
            enforce: self.enforce,
            repository: self.repository.clone(),
            candidate_ref: self.candidate_ref.clone(),
            default_branch_ref: self.default_branch_ref.clone(),
            base,
            candidate,
        }
    }
}
