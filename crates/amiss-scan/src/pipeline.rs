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

/// One resolved snapshot root: its tree OID plus the full identity block.
type ResolvedTree = (Oid, SnapshotIdentity);

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

/// Verifies a supplied floor's binding against the run identity. A floor
/// that fails its binding has no effect of any kind: the returned reference
/// is present only for a verified floor.
fn floor_gate(
    setup_shell: &SetupShell,
) -> (Option<&crate::policy::FloorInput>, Option<ErrorDetail>) {
    let mismatch = setup_shell.floor.as_ref().and_then(|floor| {
        crate::policy::verify_floor(
            floor,
            setup_shell
                .repository
                .as_ref()
                .map(|(owner, name)| (owner.as_str(), name.as_str())),
            setup_shell.candidate_ref.as_deref(),
            setup_shell.enforce,
        )
        .err()
    });
    let verified = if mismatch.is_none() {
        setup_shell.floor.as_ref()
    } else {
        None
    };
    (verified, mismatch)
}

/// The engine-fixed ceilings, tightened by a verified floor. A run without a
/// verified floor uses the built-in contract values unchanged.
fn effective_limits(
    floor: Option<&crate::policy::FloorInput>,
) -> (ScanLimits, amiss_git::GitLimits) {
    floor.map_or(
        (ScanLimits::CONTRACT, amiss_git::GitLimits::CONTRACT),
        |input| {
            crate::policy::tightened_limits(
                ScanLimits::CONTRACT,
                amiss_git::GitLimits::CONTRACT,
                &input.floor,
            )
        },
    )
}

fn binding_mismatch(
    setup_shell: &SetupShell,
    base: SnapshotIdentity,
    candidate: CandidateBlock,
    row: ErrorDetail,
) -> Built {
    let mut setup = setup_shell.with(base, candidate);
    setup.controls_unavailable = Some("control-binding-mismatch");
    construct_incomplete(&setup, &[row])
}

/// The shared conclusion of a two-sided run: incomplete on any accumulated
/// failure, otherwise correlation and full construction.
fn conclude(
    setup: &Setup,
    base: (&SnapshotDiscovery, &Side),
    candidate: (&SnapshotDiscovery, &Side),
    failures: &[ErrorDetail],
) -> Built {
    if !failures.is_empty() {
        return construct_incomplete(setup, failures);
    }
    match correlate(base.1, candidate.1) {
        Ok(comparisons) => construct(setup, base.0, candidate.0, &comparisons),
        Err(defect) => construct_incomplete(setup, &[detail(&defect, None)]),
    }
}

/// The fallback identity projection when a snapshot cannot be established:
/// each supplied commit OID stands in for both identity fields.
fn oid_fallback(
    repo: &Repository,
    setup_shell: &SetupShell,
    base_oid: &Oid,
    candidate_oid: &Oid,
) -> Setup {
    let placeholder = |oid: &Oid| SnapshotIdentity {
        object_format: format_str(repo.object_format()),
        commit_oid: oid.as_str().to_owned(),
        tree_oid: oid.as_str().to_owned(),
    };
    setup_shell.with(
        placeholder(base_oid),
        CandidateBlock::Commit(placeholder(candidate_oid)),
    )
}

/// Resolves both commit trees, then settles a pending floor binding
/// mismatch against the real snapshot identities.
fn pair_trees(
    repo: &Repository,
    git_resources: &mut GitResources,
    setup_shell: &SetupShell,
    floor_mismatch: Option<ErrorDetail>,
    base_oid: &Oid,
    candidate_oid: &Oid,
) -> Result<(ResolvedTree, ResolvedTree), Box<Built>> {
    let trees = resolve_tree(repo, git_resources, base_oid).and_then(|base_tree| {
        resolve_tree(repo, git_resources, candidate_oid)
            .map(|candidate_tree| (base_tree, candidate_tree))
    });
    let (base_tree, candidate_tree) = trees.map_err(|defect_detail| {
        Box::new(construct_incomplete(
            &oid_fallback(repo, setup_shell, base_oid, candidate_oid),
            &[defect_detail],
        ))
    })?;
    if let Some(row) = floor_mismatch {
        return Err(Box::new(binding_mismatch(
            setup_shell,
            base_tree.1,
            CandidateBlock::Commit(candidate_tree.1),
            row,
        )));
    }
    Ok((base_tree, candidate_tree))
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
    let (verified_floor, floor_mismatch) = floor_gate(setup_shell);
    let (scan_limits, git_limits) = effective_limits(verified_floor);
    let mut git_resources = GitResources::new(git_limits);
    let trees = pair_trees(
        repo,
        &mut git_resources,
        setup_shell,
        floor_mismatch,
        base_oid,
        candidate_oid,
    );
    let (base_tree, candidate_tree) = match trees {
        Ok(pair) => pair,
        Err(built) => return *built,
    };

    let mut base_scan = ScanResources::new(scan_limits);
    let mut candidate_scan = ScanResources::new(scan_limits);
    let policies = pair_policies(
        repo,
        &mut git_resources,
        setup_shell,
        (&base_tree, &mut base_scan),
        (&candidate_tree, &mut candidate_scan),
    );
    let (base_policy, candidate_policy) = match policies {
        Ok(pair) => pair,
        Err(built) => return *built,
    };
    let includes = crate::policy::Includes::union(&base_policy, &candidate_policy);

    let base = evaluate_tree(
        repo,
        &mut git_resources,
        &mut base_scan,
        engine,
        github,
        &includes,
        base_tree,
    );
    let candidate = evaluate_tree(
        repo,
        &mut git_resources,
        &mut candidate_scan,
        engine,
        github,
        &includes,
        candidate_tree,
    );
    match (base, candidate) {
        (Ok((base, base_failures)), Ok((candidate, candidate_failures))) => {
            let mut failures = base_failures;
            failures.extend(candidate_failures);
            let effects = pair_effects(
                repo,
                &mut git_resources,
                verified_floor,
                &base_policy,
                &candidate_policy,
                (&base.discovery, &mut base_scan),
                (&candidate.discovery, &mut candidate_scan),
                &mut failures,
            );
            let mut setup = setup_shell.with(
                base.identity.clone(),
                CandidateBlock::Commit(candidate.identity.clone()),
            );
            setup.policy = effects;
            conclude(
                &setup,
                (&base.discovery, &base.side),
                (&candidate.discovery, &candidate.side),
                &failures,
            )
        }
        (Err(defect), Ok(_)) | (Ok(_), Err(defect)) => construct_incomplete(
            &oid_fallback(repo, setup_shell, base_oid, candidate_oid),
            &[defect],
        ),
        (Err(base_defect), Err(candidate_defect)) => construct_incomplete(
            &oid_fallback(repo, setup_shell, base_oid, candidate_oid),
            &[base_defect, candidate_defect],
        ),
    }
}

/// Acquires both repository policies, each side on its own per-snapshot
/// ledger, producing the fatal projection on any defect.
fn pair_policies(
    repo: &Repository,
    git_resources: &mut GitResources,
    setup_shell: &SetupShell,
    base: (&(Oid, SnapshotIdentity), &mut ScanResources),
    candidate: (&(Oid, SnapshotIdentity), &mut ScanResources),
) -> Result<(crate::policy::PolicySide, crate::policy::PolicySide), Box<Built>> {
    let (base_tree, base_scan) = base;
    let (candidate_tree, candidate_scan) = candidate;
    let fallback = |details: &[ErrorDetail]| {
        let mut setup = setup_shell.with(
            base_tree.1.clone(),
            CandidateBlock::Commit(candidate_tree.1.clone()),
        );
        setup.controls_unavailable = Some(policy_unavailable_reason(details));
        Box::new(construct_incomplete(&setup, details))
    };
    let base_policy = crate::policy::acquire(repo, git_resources, base_scan, &base_tree.0)
        .map_err(|details| fallback(&details))?;
    let candidate_policy =
        crate::policy::acquire(repo, git_resources, candidate_scan, &candidate_tree.0)
            .map_err(|details| fallback(&details))?;
    Ok((base_policy, candidate_policy))
}

/// `invalid-repository-policy` requires its `CONFIGURATION_INVALID` anchor;
/// any other acquisition failure leaves the controls merely not parsed.
fn policy_unavailable_reason(details: &[ErrorDetail]) -> &'static str {
    if details
        .iter()
        .any(|row| row.code == amiss_wire::report::AnalysisErrorCode::ConfigurationInvalid)
    {
        "invalid-repository-policy"
    } else {
        "not-parsed"
    }
}

fn control_read_detail(defect: &Error, path: &str) -> ErrorDetail {
    match defect {
        Error::ResourceLimit {
            resource,
            configured_limit,
            observed_lower_bound,
        } => ErrorDetail {
            code: defect.code(),
            path: (*resource == amiss_wire::controls::ResourceName::SelectedControlBlobBytes)
                .then(|| path.to_owned()),
            resource: Some((*resource, *configured_limit, *observed_lower_bound)),
        },
        Error::Parse(_) | Error::Git(_) | Error::UnrepresentablePath | Error::Internal => {
            ErrorDetail {
                code: defect.code(),
                path: None,
                resource: None,
            }
        }
    }
}

/// The complete policy layer for a two-sided run: repository comparison
/// effects, then the verified floor applied over them. A floor defect row
/// joins the accumulated failures.
#[expect(
    clippy::too_many_arguments,
    reason = "the two-sided control context is the contract's"
)]
fn pair_effects(
    repo: &Repository,
    git_resources: &mut GitResources,
    verified_floor: Option<&crate::policy::FloorInput>,
    base_policy: &crate::policy::PolicySide,
    candidate_policy: &crate::policy::PolicySide,
    base: (&SnapshotDiscovery, &mut ScanResources),
    candidate: (&SnapshotDiscovery, &mut ScanResources),
    failures: &mut Vec<ErrorDetail>,
) -> crate::policy::Effects {
    let mut effects = crate::policy::effects(
        base_policy,
        candidate_policy,
        &inventory_lookup(candidate.0),
    );
    if let Some(row) = apply_floor(
        repo,
        git_resources,
        verified_floor,
        base,
        candidate,
        &mut effects,
        failures.is_empty(),
    ) {
        failures.push(row);
    }
    effects
}

/// Applies a verified floor to the run: the verified provenance and
/// raise-only dispositions always, floor inventory coverage from the
/// already-acquired candidate discovery, and protected control paths
/// compared across both sides only while no earlier stage has failed. The
/// first protected-path acquisition defect stops that comparison.
fn apply_floor(
    repo: &Repository,
    git_resources: &mut GitResources,
    floor: Option<&crate::policy::FloorInput>,
    base: (&SnapshotDiscovery, &mut ScanResources),
    candidate: (&SnapshotDiscovery, &mut ScanResources),
    effects: &mut crate::policy::Effects,
    acquire: bool,
) -> Option<ErrorDetail> {
    let floor = floor?;
    effects.floor = Some((floor.floor.digest, floor.trust_source.as_str()));
    effects.floor_raised = crate::policy::floor_raises(floor);
    effects.controls.extend(crate::policy::floor_inventory(
        floor,
        &inventory_lookup(candidate.0),
    ));
    if !acquire {
        return None;
    }
    let (base_discovery, base_scan) = base;
    let (candidate_discovery, candidate_scan) = candidate;
    let mut states: Vec<(
        &str,
        (crate::policy::ProtectedState, crate::policy::ProtectedState),
    )> = Vec::new();
    for path in &floor.floor.protected_control_paths {
        let read = crate::policy::protected_state(
            repo,
            git_resources,
            base_scan,
            &base_discovery.entries,
            path.as_str(),
        )
        .and_then(|base_state| {
            crate::policy::protected_state(
                repo,
                git_resources,
                candidate_scan,
                &candidate_discovery.entries,
                path.as_str(),
            )
            .map(|candidate_state| (base_state, candidate_state))
        });
        match read {
            Ok(pair) => states.push((path.as_str(), pair)),
            Err(defect) => return Some(control_read_detail(&defect, path.as_str())),
        }
    }
    let lookup = |path: &str| {
        states.iter().find(|(known, _)| *known == path).map_or(
            (
                crate::policy::ProtectedState::Absent,
                crate::policy::ProtectedState::Absent,
            ),
            |(_, pair)| *pair,
        )
    };
    effects
        .controls
        .extend(crate::policy::floor_protected(floor, &lookup));
    None
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
                DocumentStatus::ExcludedBuiltIn => crate::policy::InventoryState::Outside,
                DocumentStatus::Unsupported(_) | DocumentStatus::Failed(_) => {
                    crate::policy::InventoryState::Unsupported
                }
            };
        }
        if discovery.entries.contains_key(path) {
            return crate::policy::InventoryState::Outside;
        }
        crate::policy::InventoryState::Missing
    }
}

/// The staged candidate's discovery and observations plus every accumulated
/// failure row, or the fatal projection when the index side cannot be
/// discovered at all.
#[expect(
    clippy::too_many_arguments,
    reason = "the staged pipeline context is the contract's"
)]
fn staged_candidate(
    repo: &Repository,
    git_resources: &mut GitResources,
    candidate_scan: &mut ScanResources,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    setup_shell: &SetupShell,
    base_identity: &SnapshotIdentity,
    includes: &crate::policy::Includes,
    index: &amiss_git::LogicalIndex,
    base_failures: Vec<ErrorDetail>,
) -> Result<(SnapshotDiscovery, Option<Side>, Vec<ErrorDetail>), Box<Built>> {
    let discovery =
        crate::discovery::discover_index(repo, git_resources, candidate_scan, includes, index)
            .map_err(|defect| {
                Box::new(candidate_unavailable(
                    setup_shell,
                    base_identity.clone(),
                    &defect,
                ))
            })?;
    let (side, failures) = staged_sides(
        repo,
        git_resources,
        candidate_scan,
        engine,
        github,
        &discovery,
        base_failures,
    );
    Ok((discovery, side, failures))
}

/// The staged candidate's observations plus every accumulated failure row;
/// a side that cannot be built at all is `None` with its defect appended.
fn staged_sides(
    repo: &Repository,
    git_resources: &mut GitResources,
    scan_resources: &mut ScanResources,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    candidate_discovery: &SnapshotDiscovery,
    base_failures: Vec<ErrorDetail>,
) -> (Option<Side>, Vec<ErrorDetail>) {
    let mut failures = base_failures;
    match side_observations(
        repo,
        git_resources,
        scan_resources,
        engine,
        github,
        candidate_discovery,
    ) {
        Ok((side, candidate_failures)) => {
            failures.extend(candidate_failures);
            (Some(side), failures)
        }
        Err(defect_detail) => {
            failures.push(defect_detail);
            (None, failures)
        }
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
    scan_resources: &mut ScanResources,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    includes: &crate::policy::Includes,
    setup_shell: &SetupShell,
    base_placeholder: SnapshotIdentity,
    base_tree: (Oid, SnapshotIdentity),
) -> Result<(Evaluated, Vec<ErrorDetail>), Box<Built>> {
    evaluate_tree(
        repo,
        git_resources,
        scan_resources,
        engine,
        github,
        includes,
        base_tree,
    )
    .map_err(|defect_detail| {
        let setup = setup_shell.with(
            base_placeholder,
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
        );
        Box::new(construct_incomplete(&setup, &[defect_detail]))
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the staged pipeline context is the contract's"
)]
fn staged_policy(
    repo: &Repository,
    git_resources: &mut GitResources,
    base_scan: &mut ScanResources,
    candidate_scan: &mut ScanResources,
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
    let bail = |details: &[ErrorDetail]| {
        let mut setup = setup_shell.with(
            base_placeholder.clone(),
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
        );
        setup.controls_unavailable = Some(policy_unavailable_reason(details));
        Box::new(construct_incomplete(&setup, details))
    };
    let base = crate::policy::acquire(repo, git_resources, base_scan, base_tree)
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
            candidate_scan,
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

fn evaluate_tree(
    repo: &Repository,
    git_resources: &mut GitResources,
    scan_resources: &mut ScanResources,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    includes: &crate::policy::Includes,
    tree: (Oid, SnapshotIdentity),
) -> Result<(Evaluated, Vec<ErrorDetail>), ErrorDetail> {
    let (tree_oid, identity) = tree;
    let discovery = discover(repo, git_resources, scan_resources, includes, &tree_oid)
        .map_err(|defect| detail(&defect, None))?;
    let (side, failures) = side_observations(
        repo,
        git_resources,
        scan_resources,
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
    pub floor: Option<crate::policy::FloorInput>,
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

fn staged_tree(
    repo: &Repository,
    git_resources: &mut GitResources,
    setup_shell: &SetupShell,
    base_placeholder: &SnapshotIdentity,
    base_oid: &Oid,
) -> Result<(Oid, SnapshotIdentity), Box<Built>> {
    resolve_tree(repo, git_resources, base_oid).map_err(|defect_detail| {
        let setup = setup_shell.with(
            base_placeholder.clone(),
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
        );
        Box::new(construct_incomplete(&setup, &[defect_detail]))
    })
}

/// One pinned-snapshot recheck: the index is reread after the scan and any
/// change replaces the result with the snapshot-changed projection.
fn recheck_index(
    repo: &Repository,
    git_resources: &mut GitResources,
    setup_shell: &SetupShell,
    base_identity: SnapshotIdentity,
    initial: &[u8],
    built: Built,
) -> Built {
    if let Err(defect) = repo.verify_index_unchanged(git_resources, initial) {
        let defect = Error::from(defect);
        let changed_setup = setup_shell.with(
            base_identity,
            CandidateBlock::Unavailable(vec![unavailable_reason(&defect)]),
        );
        return construct_incomplete(&changed_setup, &[detail(&defect, None)]);
    }
    built
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
    let (verified_floor, floor_mismatch) = floor_gate(setup_shell);
    let (scan_limits, git_limits) = effective_limits(verified_floor);
    let mut git_resources = GitResources::new(git_limits);
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

    let base_tree = match staged_tree(
        repo,
        &mut git_resources,
        setup_shell,
        &base_placeholder,
        base_oid,
    ) {
        Ok(tree) => tree,
        Err(built) => return *built,
    };
    if let Some(row) = floor_mismatch {
        return binding_mismatch(
            setup_shell,
            base_tree.1,
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
            row,
        );
    }
    let mut base_scan = ScanResources::new(scan_limits);
    let mut candidate_scan = ScanResources::new(scan_limits);
    let (base_policy, candidate_policy, includes) = match staged_policy(
        repo,
        &mut git_resources,
        &mut base_scan,
        &mut candidate_scan,
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
        &mut base_scan,
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

    let staged = staged_candidate(
        repo,
        &mut git_resources,
        &mut candidate_scan,
        engine,
        github,
        setup_shell,
        &base_evaluated.identity,
        &includes,
        &index,
        base_failures,
    );
    let (candidate_discovery, candidate_side, mut failures) = match staged {
        Ok(parts) => parts,
        Err(built) => return *built,
    };
    let effects = pair_effects(
        repo,
        &mut git_resources,
        verified_floor,
        &base_policy,
        &candidate_policy,
        (&base_evaluated.discovery, &mut base_scan),
        (&candidate_discovery, &mut candidate_scan),
        &mut failures,
    );
    let candidate_block = index_candidate_block(repo, base_oid, &index, skip_worktree_paths);
    let mut setup = setup_shell.with(base_evaluated.identity.clone(), candidate_block);
    setup.policy = effects;
    staged_finish(
        repo,
        &mut git_resources,
        setup_shell,
        &setup,
        &base_evaluated,
        (&candidate_discovery, candidate_side.as_ref()),
        &failures,
        &initial,
    )
}

/// The staged conclusion plus the pinned-index recheck: a complete run
/// correlates and constructs; anything else is the incomplete projection.
#[expect(
    clippy::too_many_arguments,
    reason = "the staged pipeline context is the contract's"
)]
fn staged_finish(
    repo: &Repository,
    git_resources: &mut GitResources,
    setup_shell: &SetupShell,
    setup: &Setup,
    base_evaluated: &Evaluated,
    candidate: (&SnapshotDiscovery, Option<&Side>),
    failures: &[ErrorDetail],
    initial: &[u8],
) -> Built {
    let built = match (candidate.1, failures) {
        (Some(side), []) => conclude(
            setup,
            (&base_evaluated.discovery, &base_evaluated.side),
            (candidate.0, side),
            &[],
        ),
        _ => construct_incomplete(setup, failures),
    };
    recheck_index(
        repo,
        git_resources,
        setup_shell,
        base_evaluated.identity.clone(),
        initial,
        built,
    )
}
