use amiss_git::{GitResources, ObjectKind, Repository, parse_commit};
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::{EngineProvenance, ErrorDetail};

use crate::Error;
use crate::correlate::{Observation, Side, correlate};
use crate::discovery::{DocumentStatus, SnapshotDiscovery, discover};
use crate::observe::occurrence_id;
use crate::report::{
    Built, CandidateBlock, Setup, SnapshotIdentity, construct, construct_incomplete,
    synthetic_candidate,
};
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

/// Builds one side's observations from its discovery: every scanned
/// occurrence resolved against this same snapshot, and every failed document
/// or path defect carried as a typed error detail.
fn side_observations(
    repo: &Repository,
    git_resources: &mut GitResources,
    scan_resources: &mut ScanResources,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    discovery: &SnapshotDiscovery,
) -> Result<(Side, Vec<ErrorDetail>), ErrorDetail> {
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
                let Some(adapter) = record.classification.adapter() else {
                    continue;
                };
                for occurrence in &scanned.occurrences {
                    let (intent, resolution) = resolve(
                        repo,
                        git_resources,
                        scan_resources,
                        &mut cache,
                        discovery,
                        github,
                        &record.path,
                        occurrence.occurrence.construct.is_image(),
                        &occurrence.occurrence.semantic_destination,
                    )
                    .map_err(|defect| detail(&defect, Some(&record.path)))?;
                    observations.push(Observation {
                        id: occurrence_id(engine, adapter, &record.path, occurrence, &intent),
                        document: record.path.clone(),
                        span: occurrence.occurrence.span,
                        display: occurrence.display,
                        block_kind: occurrence.occurrence.block_kind,
                        node_path: occurrence.occurrence.node_path.clone(),
                        adapter,
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
    Ok((
        Side {
            observations,
            documents,
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
    let placeholder = |oid: &Oid| SnapshotIdentity {
        object_format: format_str(repo.object_format()),
        commit_oid: oid.as_str().to_owned(),
        tree_oid: oid.as_str().to_owned(),
    };
    let fallback_setup = |shell: &SetupShell| {
        shell.with(
            placeholder(base_oid),
            CandidateBlock::Commit(placeholder(candidate_oid)),
        )
    };

    let trees = resolve_tree(repo, &mut git_resources, base_oid).and_then(|base_tree| {
        resolve_tree(repo, &mut git_resources, candidate_oid)
            .map(|candidate_tree| (base_tree, candidate_tree))
    });
    let (base_tree, candidate_tree) = match trees {
        Ok(pair) => pair,
        Err(defect_detail) => {
            return construct_incomplete(&fallback_setup(setup_shell), &[defect_detail]);
        }
    };

    let mut control_resources = ScanResources::new(ScanLimits::CONTRACT);
    let policies = acquire_policies(
        repo,
        &mut git_resources,
        &mut control_resources,
        &base_tree.0,
        &candidate_tree.0,
    );
    let (base_policy, candidate_policy) = match policies {
        Ok(pair) => pair,
        Err(details) => {
            let mut setup = fallback_setup(setup_shell);
            setup.controls_unavailable = Some("invalid-repository-policy");
            return construct_incomplete(&setup, &details);
        }
    };
    let includes = crate::policy::Includes::union(&base_policy, &candidate_policy);

    let base = evaluate_tree(
        repo,
        &mut git_resources,
        engine,
        github,
        &includes,
        base_tree,
    );
    let candidate = evaluate_tree(
        repo,
        &mut git_resources,
        engine,
        github,
        &includes,
        candidate_tree,
    );
    match (base, candidate) {
        (Ok((base, base_failures)), Ok((candidate, candidate_failures))) => {
            let effects = crate::policy::effects(
                &base_policy,
                &candidate_policy,
                &inventory_lookup(&candidate.discovery),
            );
            let mut setup = setup_shell.with(
                base.identity.clone(),
                CandidateBlock::Commit(candidate.identity.clone()),
            );
            setup.policy = effects;
            let mut failures = base_failures;
            failures.extend(candidate_failures);
            if !failures.is_empty() {
                return construct_incomplete(&setup, &failures);
            }
            match correlate(&base.side, &candidate.side) {
                Ok(comparisons) => {
                    construct(&setup, &base.discovery, &candidate.discovery, &comparisons)
                }
                Err(defect) => construct_incomplete(&setup, &[detail(&defect, None)]),
            }
        }
        (Err(defect), Ok(_)) | (Ok(_), Err(defect)) => {
            construct_incomplete(&fallback_setup(setup_shell), &[defect])
        }
        (Err(base_defect), Err(candidate_defect)) => construct_incomplete(
            &fallback_setup(setup_shell),
            &[base_defect, candidate_defect],
        ),
    }
}

/// The candidate state of one inventory path under the obligation test.
fn inventory_lookup(
    discovery: &SnapshotDiscovery,
) -> impl Fn(&str) -> crate::policy::InventoryState {
    move |path: &str| {
        if let Some(record) = discovery
            .documents
            .iter()
            .find(|record| record.path == path)
        {
            return match record.status {
                DocumentStatus::Scanned(_) => crate::policy::InventoryState::Scanned,
                DocumentStatus::ExcludedBuiltIn
                | DocumentStatus::Unsupported(_)
                | DocumentStatus::Failed(_) => crate::policy::InventoryState::Unsupported,
            };
        }
        if discovery.entries.contains_key(path) {
            return crate::policy::InventoryState::Outside;
        }
        crate::policy::InventoryState::Missing
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the staged pipeline context is the contract's"
)]
fn staged_result(
    repo: &Repository,
    git_resources: &mut GitResources,
    scan_resources: &mut ScanResources,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    setup: &Setup,
    base_evaluated: &Evaluated,
    base_failures: Vec<ErrorDetail>,
    candidate_discovery: &SnapshotDiscovery,
) -> Built {
    let candidate = side_observations(
        repo,
        git_resources,
        scan_resources,
        engine,
        github,
        candidate_discovery,
    );
    let (candidate_side, candidate_failures) = match candidate {
        Ok(result) => result,
        Err(defect_detail) => return construct_incomplete(setup, &[defect_detail]),
    };
    let mut failures = base_failures;
    failures.extend(candidate_failures);
    if !failures.is_empty() {
        return construct_incomplete(setup, &failures);
    }
    match correlate(&base_evaluated.side, &candidate_side) {
        Ok(comparisons) => construct(
            setup,
            &base_evaluated.discovery,
            candidate_discovery,
            &comparisons,
        ),
        Err(defect) => construct_incomplete(setup, &[detail(&defect, None)]),
    }
}

fn candidate_unavailable(
    setup_shell: &SetupShell,
    base: SnapshotIdentity,
    defect: &Error,
) -> Built {
    let setup = setup_shell.with(
        base,
        CandidateBlock::Unavailable(vec![unavailable_reason(defect)]),
    );
    construct_incomplete(&setup, &[detail(defect, None)])
}

/// One pinned read of the raw index, its parsed logical form, and the
/// skip-worktree count.
fn pinned_index(
    repo: &Repository,
    git_resources: &mut GitResources,
) -> Result<(Vec<u8>, amiss_git::LogicalIndex, u64), Error> {
    let initial = repo.read_index_bytes(git_resources).map_err(Error::from)?;
    let index = amiss_git::parse_index_file(repo.object_format(), &initial).map_err(Error::from)?;
    let skip_worktree_paths = u64::try_from(
        index
            .entries
            .iter()
            .filter(|entry| entry.skip_worktree)
            .count(),
    )
    .unwrap_or(u64::MAX);
    Ok((initial, index, skip_worktree_paths))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the staged pipeline context is the contract's"
)]
fn staged_base(
    repo: &Repository,
    git_resources: &mut GitResources,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    includes: &crate::policy::Includes,
    setup_shell: &SetupShell,
    base_placeholder: SnapshotIdentity,
    base_tree: (Oid, SnapshotIdentity),
) -> Result<(Evaluated, Vec<ErrorDetail>), Box<Built>> {
    evaluate_tree(repo, git_resources, engine, github, includes, base_tree).map_err(
        |defect_detail| {
            let setup = setup_shell.with(
                base_placeholder,
                CandidateBlock::Unavailable(vec!["not-evaluated"]),
            );
            Box::new(construct_incomplete(&setup, &[defect_detail]))
        },
    )
}

fn staged_policy(
    repo: &Repository,
    git_resources: &mut GitResources,
    setup_shell: &SetupShell,
    base_placeholder: &SnapshotIdentity,
    base_tree: &Oid,
    index: &amiss_git::LogicalIndex,
) -> Result<
    (
        crate::policy::PolicySide,
        crate::policy::PolicySide,
        crate::policy::Includes,
    ),
    Box<Built>,
> {
    let mut control_resources = ScanResources::new(ScanLimits::CONTRACT);
    let bail = |details: &[ErrorDetail]| {
        let mut setup = setup_shell.with(
            base_placeholder.clone(),
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
        );
        setup.controls_unavailable = Some("invalid-repository-policy");
        Box::new(construct_incomplete(&setup, details))
    };
    let base = crate::policy::acquire(repo, git_resources, &mut control_resources, base_tree)
        .map_err(|details| bail(&details))?;
    let staged = index
        .entries
        .iter()
        .find(|entry| entry.path == amiss_wire::controls::SCANNER_POLICY_PATH.as_bytes());
    let candidate = match staged {
        None => crate::policy::PolicySide::default(),
        Some(entry) => crate::policy::acquire_entry(
            repo,
            git_resources,
            &mut control_resources,
            entry.mode,
            &entry.oid,
        )
        .map_err(|details| bail(&details))?,
    };
    let includes = crate::policy::Includes::union(&base, &candidate);
    Ok((base, candidate, includes))
}

fn resolve_tree(
    repo: &Repository,
    git_resources: &mut GitResources,
    commit_oid: &Oid,
) -> Result<(Oid, SnapshotIdentity), ErrorDetail> {
    let commit_object = repo
        .read_expected(git_resources, commit_oid, ObjectKind::Commit)
        .map_err(|defect| detail(&Error::from(defect), None))?;
    let commit = parse_commit(repo.object_format(), &commit_object.body)
        .map_err(|defect| detail(&Error::from(defect), None))?;
    Ok((
        commit.tree.clone(),
        SnapshotIdentity {
            object_format: format_str(repo.object_format()),
            commit_oid: commit_oid.as_str().to_owned(),
            tree_oid: commit.tree.as_str().to_owned(),
        },
    ))
}

fn acquire_policies(
    repo: &Repository,
    git_resources: &mut GitResources,
    control_resources: &mut ScanResources,
    base_tree: &Oid,
    candidate_tree: &Oid,
) -> Result<(crate::policy::PolicySide, crate::policy::PolicySide), Vec<ErrorDetail>> {
    let base = crate::policy::acquire(repo, git_resources, control_resources, base_tree)?;
    let candidate = crate::policy::acquire(repo, git_resources, control_resources, candidate_tree)?;
    Ok((base, candidate))
}

fn evaluate_tree(
    repo: &Repository,
    git_resources: &mut GitResources,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    includes: &crate::policy::Includes,
    tree: (Oid, SnapshotIdentity),
) -> Result<(Evaluated, Vec<ErrorDetail>), ErrorDetail> {
    let (tree_oid, identity) = tree;
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let discovery = discover(
        repo,
        git_resources,
        &mut scan_resources,
        includes,
        &tree_oid,
    )
    .map_err(|defect| detail(&defect, None))?;
    let (side, failures) = side_observations(
        repo,
        git_resources,
        &mut scan_resources,
        engine,
        github,
        &discovery,
    )?;
    Ok((
        Evaluated {
            identity,
            discovery,
            side,
        },
        failures,
    ))
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
    fn with(&self, base: SnapshotIdentity, candidate: CandidateBlock) -> Setup {
        Setup {
            engine: self.engine.clone(),
            enforce: self.enforce,
            repository: self.repository.clone(),
            candidate_ref: self.candidate_ref.clone(),
            default_branch_ref: self.default_branch_ref.clone(),
            base,
            candidate,
            policy: crate::policy::Effects::default(),
            controls_unavailable: None,
        }
    }
}

fn index_candidate_block(
    repo: &Repository,
    base_oid: &Oid,
    index: &amiss_git::LogicalIndex,
    skip_worktree_paths: u64,
) -> CandidateBlock {
    let entries: Vec<(String, amiss_wire::controls::GitMode, String, bool)> = index
        .entries
        .iter()
        .filter_map(|entry| {
            str::from_utf8(&entry.path).ok().map(|path| {
                (
                    path.to_owned(),
                    entry.mode,
                    entry.oid.as_str().to_owned(),
                    entry.skip_worktree,
                )
            })
        })
        .collect();
    CandidateBlock::Index(synthetic_candidate(
        format_str(repo.object_format()),
        base_oid.as_str(),
        &entries,
        skip_worktree_paths,
    ))
}

const fn unavailable_reason(defect: &Error) -> &'static str {
    match defect {
        Error::Git(crate::GitDefect::RepositoryUnavailable) => "repository-unavailable",
        Error::Git(crate::GitDefect::ObjectMissing) => "missing-object",
        Error::Git(crate::GitDefect::ObjectWrongKind) => "wrong-object-kind",
        Error::Git(crate::GitDefect::ObjectUnreadable) => "unreadable-object",
        Error::Git(crate::GitDefect::IndexInvalid) => "index-invalid",
        Error::Git(crate::GitDefect::IndexUnmerged) => "index-unmerged",
        Error::Git(crate::GitDefect::IntentToAdd) => "intent-to-add",
        Error::Git(crate::GitDefect::SnapshotChanged) => "snapshot-changed",
        Error::UnrepresentablePath => "unrepresentable-path",
        Error::ResourceLimit { .. } => "resource-limit",
        Error::Parse(_) | Error::Internal => "not-evaluated",
    }
}

/// The staged-index run: the explicit base commit plus the synthetic
/// candidate built from one pinned read of the complete logical index. After
/// the scan, the current index is reread and compared; a change is solely a
/// snapshot change.
#[must_use]
pub fn staged_index(
    repo: &Repository,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    setup_shell: &SetupShell,
    base_oid: &Oid,
) -> Built {
    let mut git_resources = GitResources::new(amiss_git::GitLimits::CONTRACT);
    let base_placeholder = SnapshotIdentity {
        object_format: format_str(repo.object_format()),
        commit_oid: base_oid.as_str().to_owned(),
        tree_oid: base_oid.as_str().to_owned(),
    };

    let (initial, index, skip_worktree_paths) = match pinned_index(repo, &mut git_resources) {
        Ok(pinned) => pinned,
        Err(defect) => {
            return candidate_unavailable(setup_shell, base_placeholder, &defect);
        }
    };

    let base_tree = match resolve_tree(repo, &mut git_resources, base_oid) {
        Ok(tree) => tree,
        Err(defect_detail) => {
            let setup = setup_shell.with(
                base_placeholder,
                CandidateBlock::Unavailable(vec!["not-evaluated"]),
            );
            return construct_incomplete(&setup, &[defect_detail]);
        }
    };
    let (base_policy, candidate_policy, includes) = match staged_policy(
        repo,
        &mut git_resources,
        setup_shell,
        &base_placeholder,
        &base_tree.0,
        &index,
    ) {
        Ok(acquired) => acquired,
        Err(built) => return *built,
    };
    let (base_evaluated, base_failures) = match staged_base(
        repo,
        &mut git_resources,
        engine,
        github,
        &includes,
        setup_shell,
        base_placeholder,
        base_tree,
    ) {
        Ok(evaluated) => evaluated,
        Err(built) => return *built,
    };

    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let candidate_discovery = match crate::discovery::discover_index(
        repo,
        &mut git_resources,
        &mut scan_resources,
        &includes,
        &index,
    ) {
        Ok(discovery) => discovery,
        Err(defect) => {
            return candidate_unavailable(setup_shell, base_evaluated.identity, &defect);
        }
    };
    let candidate_block = index_candidate_block(repo, base_oid, &index, skip_worktree_paths);
    let mut setup = setup_shell.with(base_evaluated.identity.clone(), candidate_block);
    setup.policy = crate::policy::effects(
        &base_policy,
        &candidate_policy,
        &inventory_lookup(&candidate_discovery),
    );

    let built = staged_result(
        repo,
        &mut git_resources,
        &mut scan_resources,
        engine,
        github,
        &setup,
        &base_evaluated,
        base_failures,
        &candidate_discovery,
    );

    if let Err(defect) = repo.verify_index_unchanged(&mut git_resources, &initial) {
        let defect = Error::from(defect);
        let changed_setup = setup_shell.with(
            base_evaluated.identity,
            CandidateBlock::Unavailable(vec![unavailable_reason(&defect)]),
        );
        return construct_incomplete(&changed_setup, &[detail(&defect, None)]);
    }
    built
}
