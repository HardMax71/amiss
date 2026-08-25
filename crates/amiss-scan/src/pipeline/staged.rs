use amiss_git::{GitResources, Repository};
use amiss_wire::model::{Oid, RepoPath};
use amiss_wire::report::{AnalysisErrorCode, EngineProvenance, ErrorDetail};

use crate::Error;
use crate::correlate::Side;
use crate::discovery::SnapshotDiscovery;
use crate::report::{
    Built, CandidateBlock, Setup, SnapshotIdentity, construct_incomplete, synthetic_candidate,
};
use crate::resolve::ForgeContext;
use crate::resources::{ScanLimits, ScanResources};

use super::external::external_gate;
use super::{
    Evaluated, ExternalVerified, PipelineFailure, PipelineResult, ResolvedTree, SetupShell,
    binding_mismatch, conclude, controls_failure, detail, effective_limits, effective_shell,
    evaluate_tree, floor_gate, pair_effects, policy_unavailable_reason, resolve_tree,
    side_observations,
};

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
    forge: Option<&ForgeContext>,
    semantic: crate::semantic::View<'_>,
    setup_shell: &SetupShell,
    base_identity: &SnapshotIdentity,
    includes: &crate::policy::Includes,
    index: &amiss_git::LogicalIndex,
    base_failures: Vec<ErrorDetail>,
    claims: &mut Vec<crate::claim::ClaimOutcome>,
) -> PipelineResult<(SnapshotDiscovery, Option<Side>, Vec<ErrorDetail>)> {
    let discovery =
        crate::discovery::discover_index(repo, git_resources, candidate_scan, includes, index)
            .map_err(|defect| candidate_unavailable(setup_shell, base_identity.clone(), &defect))?;
    let mut failures = base_failures;
    match side_observations(
        repo,
        git_resources,
        candidate_scan,
        super::ObservationContext {
            engine,
            forge,
            semantic,
        },
        &discovery,
        Some(claims),
    ) {
        Ok((side, candidate_failures)) => {
            failures.extend(candidate_failures);
            Ok((discovery, Some(side), failures))
        }
        Err(defect_detail) => {
            failures.push(defect_detail);
            Ok((discovery, None, failures))
        }
    }
}

fn candidate_unavailable(
    setup_shell: &SetupShell,
    base: SnapshotIdentity,
    defect: &Error,
) -> PipelineFailure {
    let setup = setup_shell.with(
        base,
        CandidateBlock::Unavailable(vec![unavailable_reason(defect)]),
    );
    PipelineFailure::one(setup, detail(defect, None))
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

fn not_evaluated(
    setup_shell: &SetupShell,
    base: &SnapshotIdentity,
    detail: ErrorDetail,
) -> PipelineFailure {
    PipelineFailure::one(
        setup_shell.with(
            base.clone(),
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
        ),
        detail,
    )
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
) -> PipelineResult<(
    crate::policy::PolicySide,
    crate::policy::PolicySide,
    crate::policy::Includes,
)> {
    let bail = |details: Vec<ErrorDetail>| {
        let mut setup = setup_shell.with(
            base_placeholder.clone(),
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
        );
        setup.controls_unavailable = Some(policy_unavailable_reason(&details));
        PipelineFailure::new(setup, details)
    };
    let base = crate::policy::acquire(repo, git_resources, base_scan, base_tree).map_err(&bail)?;
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
        .map_err(bail)?,
    };
    let includes = crate::policy::Includes::union(&base, &candidate);
    Ok((base, candidate, includes))
}

/// The synthetic candidate identity claims `complete-logical-index`, so a
/// row this block cannot spell is a refusal of the whole identity, never a
/// silent omission behind a digest that says nothing is missing. Every such
/// row is disclosed, each with its bytes when they fit the report's frozen
/// hex field.
fn index_candidate_block(
    repo: &Repository,
    base_oid: &Oid,
    index: &amiss_git::LogicalIndex,
    skip_worktree_paths: u64,
) -> Result<CandidateBlock, Vec<ErrorDetail>> {
    let disclosure_cap = amiss_git::GitLimits::CONTRACT.raw_path_bytes;
    let mut entries: Vec<(RepoPath, amiss_wire::controls::GitMode, String, bool)> =
        Vec::with_capacity(index.entries.len());
    let mut failures = Vec::new();
    for entry in &index.entries {
        let Some(path) = RepoPath::from_bytes(entry.path.clone()) else {
            let fits = u64::try_from(entry.path.len()).unwrap_or(u64::MAX) <= disclosure_cap;
            failures.push(ErrorDetail {
                code: AnalysisErrorCode::UnrepresentablePath,
                path: None,
                path_bytes: fits.then(|| entry.path.clone()),
                resource: None,
            });
            continue;
        };
        entries.push((
            path,
            entry.mode,
            entry.oid.as_str().to_owned(),
            entry.skip_worktree,
        ));
    }
    if !failures.is_empty() {
        return Err(failures);
    }
    Ok(CandidateBlock::Index(synthetic_candidate(
        repo.object_format().into(),
        base_oid.as_str(),
        &entries,
        skip_worktree_paths,
    )))
}

/// The staged run's candidate identity, or its refusals folded into the
/// failure set, which keeps the run from concluding complete.
fn resolved_candidate_block(
    repo: &Repository,
    base_oid: &Oid,
    index: &amiss_git::LogicalIndex,
    skip_worktree_paths: u64,
    failures: &mut Vec<ErrorDetail>,
) -> CandidateBlock {
    index_candidate_block(repo, base_oid, index, skip_worktree_paths).unwrap_or_else(|rows| {
        failures.extend(rows);
        CandidateBlock::Unavailable(vec!["unrepresentable-path"])
    })
}

/// The staged external-control stage: trusted time verifies against the
/// synthetic candidate identity, and tree-bound debt or waiver values are
/// rejected outright.
#[expect(
    clippy::too_many_arguments,
    reason = "the staged pipeline context is the contract's"
)]
fn staged_gate(
    repo: &Repository,
    setup_shell: &SetupShell,
    verified_floor: Option<&crate::policy::FloorInput>,
    scan_limits: ScanLimits,
    base_oid: &Oid,
    base_tree: &ResolvedTree,
    index: &amiss_git::LogicalIndex,
    skip_worktree_paths: u64,
) -> PipelineResult<ExternalVerified> {
    let candidate_block = match index_candidate_block(repo, base_oid, index, skip_worktree_paths) {
        Ok(block) => block,
        Err(rows) => {
            let setup = setup_shell.with(
                base_tree.1.clone(),
                CandidateBlock::Unavailable(vec!["unrepresentable-path"]),
            );
            return Err(PipelineFailure::new(setup, rows));
        }
    };
    let provisional = setup_shell.with(base_tree.1.clone(), candidate_block);
    external_gate(setup_shell, verified_floor, scan_limits, &provisional, None).map_err(
        |(reason, row)| {
            controls_failure(
                setup_shell,
                base_tree.1.clone(),
                CandidateBlock::Unavailable(vec!["not-evaluated"]),
                reason,
                row,
            )
        },
    )
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

/// The staged run's opened inputs: the pinned raw index with its logical
/// projection and skip count, the base placeholder, the resolved base tree,
/// and the run's git ledger under the effective ceilings.
struct StagedOpen {
    git_resources: GitResources,
    scan_limits: ScanLimits,
    initial: Vec<u8>,
    index: amiss_git::LogicalIndex,
    skip_worktree_paths: u64,
    base_placeholder: SnapshotIdentity,
    base_tree: ResolvedTree,
}

/// The staged run's opening: the base placeholder identity, the pinned
/// index, the resolved base tree, and a pending floor mismatch settled
/// against them.
fn staged_open(
    repo: &Repository,
    setup_shell: &SetupShell,
    base_oid: &Oid,
    floor_mismatch: Option<ErrorDetail>,
    verified_floor: Option<&crate::policy::FloorInput>,
) -> PipelineResult<StagedOpen> {
    let (scan_limits, git_limits) = effective_limits(verified_floor);
    let mut git_resources = GitResources::new(git_limits);
    let base_placeholder = SnapshotIdentity {
        object_format: repo.object_format().into(),
        commit_oid: base_oid.as_str().to_owned(),
        tree_oid: base_oid.as_str().to_owned(),
    };
    let (initial, index, skip_worktree_paths) = pinned_index(repo, &mut git_resources)
        .map_err(|defect| candidate_unavailable(setup_shell, base_placeholder.clone(), &defect))?;
    let base_tree = resolve_tree(repo, &mut git_resources, base_oid)
        .map_err(|detail| not_evaluated(setup_shell, &base_placeholder, detail))?;
    if let Some(row) = floor_mismatch {
        return Err(binding_mismatch(
            setup_shell,
            base_tree.1,
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
            row,
        ));
    }
    if let Some((reason, row)) = &setup_shell.external_defect {
        return Err(controls_failure(
            setup_shell,
            base_tree.1,
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
            reason,
            row.clone(),
        ));
    }
    Ok(StagedOpen {
        git_resources,
        scan_limits,
        initial,
        index,
        skip_worktree_paths,
        base_placeholder,
        base_tree,
    })
}

/// The staged-index run: the explicit base commit plus the synthetic
/// candidate built from one pinned read of the complete logical index. After
/// the scan, the current index is reread and compared; a change is solely a
/// snapshot change.
#[must_use]
pub fn staged_index(
    repo: &Repository,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    setup_shell: &SetupShell,
    base_oid: &Oid,
) -> Built {
    staged_index_result(repo, engine, forge, setup_shell, base_oid)
        .unwrap_or_else(PipelineFailure::into_built)
}

fn staged_index_result(
    repo: &Repository,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    setup_shell: &SetupShell,
    base_oid: &Oid,
) -> PipelineResult<Built> {
    let (verified_floor, floor_mismatch) = floor_gate(setup_shell);
    let (effective_scan, _effective_git) = effective_limits(verified_floor);
    let setup_shell = &effective_shell(setup_shell, &effective_scan);
    let StagedOpen {
        mut git_resources,
        scan_limits,
        initial,
        index,
        skip_worktree_paths,
        base_placeholder,
        base_tree,
    } = staged_open(repo, setup_shell, base_oid, floor_mismatch, verified_floor)?;
    let external = staged_gate(
        repo,
        setup_shell,
        verified_floor,
        scan_limits,
        base_oid,
        &base_tree,
        &index,
        skip_worktree_paths,
    )?;
    let mut base_scan = ScanResources::new(scan_limits);
    let mut candidate_scan = ScanResources::new(scan_limits);
    let (base_policy, candidate_policy, includes) = staged_policy(
        repo,
        &mut git_resources,
        &mut base_scan,
        &mut candidate_scan,
        setup_shell,
        &base_placeholder,
        &base_tree.0,
        &index,
    )?;
    let (base_evaluated, base_failures) = evaluate_tree(
        repo,
        &mut git_resources,
        &mut base_scan,
        engine,
        forge,
        crate::semantic::View {
            labels: external.semantic.labels.as_ref(),
            routes: None,
        },
        &includes,
        base_tree,
        None,
    )
    .map_err(|detail| not_evaluated(setup_shell, &base_placeholder, detail))?;
    candidate_scan.scans = std::mem::take(&mut base_scan.scans);

    let mut claims: Vec<crate::claim::ClaimOutcome> = Vec::new();
    let (candidate_discovery, candidate_side, mut failures) = staged_candidate(
        repo,
        &mut git_resources,
        &mut candidate_scan,
        engine,
        forge,
        crate::semantic::View {
            labels: external.semantic.labels.as_ref(),
            routes: Some(external.semantic.routes.as_ref()),
        },
        setup_shell,
        &base_evaluated.identity,
        &includes,
        &index,
        base_failures,
        &mut claims,
    )?;
    let (effects, navigation) = pair_effects(
        repo,
        &mut git_resources,
        verified_floor,
        external,
        &base_policy,
        &candidate_policy,
        (&base_evaluated.discovery, &mut base_scan),
        (&candidate_discovery, &mut candidate_scan),
        &mut failures,
    );
    let candidate_block =
        resolved_candidate_block(repo, base_oid, &index, skip_worktree_paths, &mut failures);
    let mut setup = setup_shell.with(base_evaluated.identity.clone(), candidate_block);
    setup.policy = effects;
    setup.policy.errors_retained = setup_shell.errors_retained;
    setup.policy.complete_findings = scan_limits.complete_findings;
    Ok(staged_finish(
        repo,
        &mut git_resources,
        setup_shell,
        &setup,
        base_evaluated,
        (candidate_discovery, candidate_side),
        navigation.as_deref(),
        &claims,
        &failures,
        &initial,
    ))
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
    base_evaluated: Evaluated,
    candidate: (SnapshotDiscovery, Option<Side>),
    navigation: Option<&crate::semantic::SiteNavigation>,
    claims: &[crate::claim::ClaimOutcome],
    failures: &[ErrorDetail],
    initial: &[u8],
) -> Built {
    let base_identity = base_evaluated.identity.clone();
    let built = match (candidate.1, failures) {
        (Some(side), []) => conclude(
            setup,
            (&base_evaluated.discovery, base_evaluated.side),
            (&candidate.0, side),
            navigation,
            claims,
            &[],
        ),
        _ => construct_incomplete(setup, failures),
    };
    recheck_index(
        repo,
        git_resources,
        setup_shell,
        base_identity,
        initial,
        built,
    )
}
