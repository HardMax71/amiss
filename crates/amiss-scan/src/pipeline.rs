use amiss_git::{GitResources, ObjectKind, Repository, parse_commit};
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::{AnalysisErrorCode, EngineProvenance, ErrorDetail, adapter_contract};

use crate::Error;
use crate::correlate::{Observation, Side, correlate};
use crate::discovery::{DocumentStatus, SnapshotDiscovery, discover};
use crate::observe::{ObservationIdentity, observation_digest};
use crate::report::{
    Built, CandidateBlock, Setup, SnapshotIdentity, construct, construct_incomplete,
    synthetic_candidate,
};
use crate::resolve::{ForgeContext, Resolver, TargetCache};
use crate::resources::{ScanLimits, ScanResources};

/// Verification and packaging of wrapper-supplied external controls.
/// Commit and staged-index orchestration remain in this parent pipeline;
/// the child owns their shared binding gate and verified control bundle.
mod external;

use external::{ExternalVerified, external_gate, external_reason};

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

pub(crate) fn detail(error: &Error, path: Option<&RepoPath>) -> ErrorDetail {
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
        path: path.cloned(),
        path_bytes: None,
        resource,
    }
}

/// Builds one side's observations from its discovery: every scanned
/// occurrence resolved against this same snapshot, and every failed document
/// or path defect carried as a typed error detail.
pub(crate) fn side_observations(
    repo: &Repository,
    git_resources: &mut GitResources,
    scan_resources: &mut ScanResources,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    discovery: &SnapshotDiscovery,
    mut claims: Option<&mut Vec<crate::claim::ClaimOutcome>>,
) -> Result<(Side, Vec<ErrorDetail>), ErrorDetail> {
    let mut failures: Vec<ErrorDetail> = discovery
        .path_defects
        .iter()
        .map(|defect| ErrorDetail {
            path_bytes: defect.raw.clone(),
            ..detail(&defect.error, None)
        })
        .collect();
    let mut cache = TargetCache::default();
    let mut resolver = Resolver::new(repo, git_resources, scan_resources, &mut cache, discovery);
    let observation_count = discovery
        .documents
        .iter()
        .filter_map(|record| {
            let DocumentStatus::Scanned(scanned) = &record.status else {
                return None;
            };
            Some(scanned.occurrences.len())
        })
        .fold(0_usize, usize::saturating_add);
    let mut observations: Vec<Observation> = Vec::with_capacity(observation_count);
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
                let Some(adapter) = record.adapter else {
                    continue;
                };
                let (_descriptor, adapter_contract_digest) = adapter_contract(engine, adapter);
                for occurrence in &scanned.occurrences {
                    let (intent, resolution) = if occurrence.occurrence.construct
                        == amiss_wire::controls::SourceConstruct::RstRefRole
                    {
                        resolver.resolve_label(&occurrence.occurrence.semantic_destination)
                    } else {
                        resolver.resolve(
                            forge,
                            adapter,
                            &record.path,
                            occurrence.occurrence.construct.is_image(),
                            &occurrence.occurrence.semantic_destination,
                        )
                    }
                    .map_err(|defect| detail(&defect, Some(&record.path)))?;
                    let id = observation_digest(&ObservationIdentity {
                        adapter,
                        contract_digest: adapter_contract_digest,
                        document: &record.path,
                        construct: occurrence.occurrence.construct,
                        node_path: &occurrence.occurrence.node_path,
                        projection_digest: occurrence.projection_digest,
                        intent: &intent,
                        raw_destination_digest: occurrence.raw_destination_digest,
                    });
                    observations.push(Observation {
                        id,
                        adapter_contract_digest,
                        document: record.path.clone(),
                        span: occurrence.occurrence.span,
                        display: occurrence.display,
                        block_kind: occurrence.occurrence.block_kind,
                        node_path: occurrence.occurrence.node_path.clone(),
                        adapter,
                        construct: occurrence.occurrence.construct,
                        external_destination: matches!(
                            resolution,
                            amiss_wire::resolution::Resolution::External(_)
                        )
                        .then(|| occurrence.occurrence.semantic_destination.clone()),
                        intent,
                        raw_destination: occurrence.occurrence.raw_destination.clone(),
                        raw_destination_digest: occurrence.raw_destination_digest,
                        projection_digest: occurrence.projection_digest,
                        resolution,
                        fragment_span: occurrence.occurrence.fragment_span,
                        path_span: occurrence.occurrence.path_span,
                    });
                }
                if let Some(outcomes) = claims.as_deref_mut() {
                    document_claims(&mut resolver, (&record.path, scanned), outcomes)?;
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
            setup_shell.repository.as_ref(),
            setup_shell.target_ref.as_deref(),
            setup_shell.profile,
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

/// Evaluates one scanned document's value claims into outcomes; unknown
/// forms stay for the governed boundary.
fn document_claims(
    resolver: &mut Resolver<'_>,
    document: (&RepoPath, &crate::scan::Scanned),
    outcomes: &mut Vec<crate::claim::ClaimOutcome>,
) -> Result<(), ErrorDetail> {
    let (path, scanned) = document;
    for governed in &scanned.governed {
        let crate::claim::GovernedForm::Value(claim) = &governed.form else {
            continue;
        };
        let verdict = resolver
            .resolve_claim(claim)
            .map_err(|defect| detail(&defect, Some(path)))?;
        outcomes.push(crate::claim::ClaimOutcome {
            carrier: crate::claim::ClaimCarrier::of(scanned.adapter),
            document: path.clone(),
            name: claim.name.clone(),
            span: governed.span,
            display: governed.display,
            source_digest: governed.digest,
            path: claim.path.clone(),
            line: claim.line,
            expected_digest: amiss_wire::digest::hb(
                crate::resolve::RAW_EVIDENCE_DOMAIN,
                claim.expected.as_bytes(),
            ),
            verdict,
        });
    }
    Ok(())
}

/// The shell reissued with the floor-effective error ceiling, so every
/// fatal projection built downstream honors it.
fn effective_shell(shell: &SetupShell, limits: &ScanLimits) -> SetupShell {
    SetupShell {
        errors_retained: limits.errors_retained,
        ..shell.clone()
    }
}

struct PipelineFailure(Box<PipelineFailureContext>);

struct PipelineFailureContext {
    setup: Setup,
    details: Vec<ErrorDetail>,
}

impl PipelineFailure {
    fn new(setup: Setup, details: Vec<ErrorDetail>) -> Self {
        Self(Box::new(PipelineFailureContext { setup, details }))
    }

    fn one(setup: Setup, detail: ErrorDetail) -> Self {
        Self::new(setup, vec![detail])
    }

    fn into_built(self) -> Built {
        construct_incomplete(&self.0.setup, &self.0.details)
    }
}

type PipelineResult<T> = Result<T, PipelineFailure>;

fn controls_failure(
    setup_shell: &SetupShell,
    base: SnapshotIdentity,
    candidate: CandidateBlock,
    reason: &'static str,
    row: ErrorDetail,
) -> PipelineFailure {
    let mut setup = setup_shell.with(base, candidate);
    setup.controls_unavailable = Some(reason);
    PipelineFailure::one(setup, row)
}

fn binding_mismatch(
    setup_shell: &SetupShell,
    base: SnapshotIdentity,
    candidate: CandidateBlock,
    row: ErrorDetail,
) -> PipelineFailure {
    controls_failure(
        setup_shell,
        base,
        candidate,
        "control-binding-mismatch",
        row,
    )
}

/// The shared conclusion of a two-sided run: incomplete on any accumulated
/// failure, otherwise correlation and full construction.
fn conclude(
    setup: &Setup,
    base: (&SnapshotDiscovery, Side),
    candidate: (&SnapshotDiscovery, Side),
    claims: &[crate::claim::ClaimOutcome],
    failures: &[ErrorDetail],
) -> Built {
    if !failures.is_empty() {
        return construct_incomplete(setup, failures);
    }
    match correlate(base.1, candidate.1) {
        Ok(comparisons) => construct(setup, base.0, candidate.0, &comparisons, claims),
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
) -> PipelineResult<(ResolvedTree, ResolvedTree)> {
    let trees = resolve_tree(repo, git_resources, base_oid).and_then(|base_tree| {
        resolve_tree(repo, git_resources, candidate_oid)
            .map(|candidate_tree| (base_tree, candidate_tree))
    });
    let (base_tree, candidate_tree) = trees.map_err(|defect_detail| {
        PipelineFailure::one(
            oid_fallback(repo, setup_shell, base_oid, candidate_oid),
            defect_detail,
        )
    })?;
    if let Some(row) = floor_mismatch {
        return Err(binding_mismatch(
            setup_shell,
            base_tree.1,
            CandidateBlock::Commit(candidate_tree.1),
            row,
        ));
    }
    if let Some((reason, row)) = &setup_shell.external_defect {
        return Err(controls_failure(
            setup_shell,
            base_tree.1,
            CandidateBlock::Commit(candidate_tree.1),
            reason,
            row.clone(),
        ));
    }
    Ok((base_tree, candidate_tree))
}

/// The commit-pair external-control stage in the fatal order: trusted time,
/// debt binding with its adoption reproduction, then waiver binding.
#[expect(
    clippy::too_many_arguments,
    reason = "the external-control context is the contract's"
)]
fn commit_controls(
    repo: &Repository,
    git_resources: &mut GitResources,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    setup_shell: &SetupShell,
    verified_floor: Option<&crate::policy::FloorInput>,
    scan_limits: ScanLimits,
    base_tree: &ResolvedTree,
    candidate_tree: &ResolvedTree,
) -> PipelineResult<ExternalVerified> {
    let failure = |reason: &'static str, row: ErrorDetail| {
        controls_failure(
            setup_shell,
            base_tree.1.clone(),
            CandidateBlock::Commit(candidate_tree.1.clone()),
            reason,
            row,
        )
    };
    let provisional = setup_shell.with(
        base_tree.1.clone(),
        CandidateBlock::Commit(candidate_tree.1.clone()),
    );
    let Some(tree_identity) = amiss_wire::model::TreeIdentity::new(
        repo.object_format(),
        candidate_tree.0.as_str().to_owned(),
    ) else {
        return Err(failure("not-parsed", detail(&Error::Internal, None)));
    };
    let external = external_gate(
        setup_shell,
        verified_floor,
        scan_limits,
        &provisional,
        Some(tree_identity),
    )
    .map_err(|(reason, row)| failure(reason, row))?;
    if let Some(context) = external.debt() {
        crate::adoption::reproduce(repo, git_resources, engine, forge, scan_limits, context)
            .map_err(|row| failure(external_reason(&row), row))?;
    }
    Ok(external)
}

/// The complete commit-pair run: both sides, correlation, and construction.
/// Any accumulated typed error makes the run incomplete with every safely
/// established row retained; the report is emitted either way.
#[must_use]
pub fn commit_pair(
    repo: &Repository,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    setup_shell: &SetupShell,
    base_oid: &Oid,
    candidate_oid: &Oid,
) -> Built {
    commit_pair_result(repo, engine, forge, setup_shell, base_oid, candidate_oid)
        .unwrap_or_else(PipelineFailure::into_built)
}

fn commit_pair_result(
    repo: &Repository,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    setup_shell: &SetupShell,
    base_oid: &Oid,
    candidate_oid: &Oid,
) -> PipelineResult<Built> {
    let (verified_floor, floor_mismatch) = floor_gate(setup_shell);
    let (scan_limits, git_limits) = effective_limits(verified_floor);
    let setup_shell = &effective_shell(setup_shell, &scan_limits);
    let mut git_resources = GitResources::new(git_limits);
    let (base_tree, candidate_tree) = pair_trees(
        repo,
        &mut git_resources,
        setup_shell,
        floor_mismatch,
        base_oid,
        candidate_oid,
    )?;
    let external = commit_controls(
        repo,
        &mut git_resources,
        engine,
        forge,
        setup_shell,
        verified_floor,
        scan_limits,
        &base_tree,
        &candidate_tree,
    )?;

    let mut base_scan = ScanResources::new(scan_limits);
    let mut candidate_scan = ScanResources::new(scan_limits);
    let (base_policy, candidate_policy) = pair_policies(
        repo,
        &mut git_resources,
        setup_shell,
        (&base_tree, &mut base_scan),
        (&candidate_tree, &mut candidate_scan),
    )?;
    let includes = crate::policy::Includes::union(&base_policy, &candidate_policy);

    let (base, candidate, claims) = evaluated_pair(
        repo,
        &mut git_resources,
        (&mut base_scan, &mut candidate_scan),
        engine,
        forge,
        &includes,
        base_tree,
        candidate_tree,
    );
    Ok(match (base, candidate) {
        (Ok((base, base_failures)), Ok((candidate, candidate_failures))) => {
            let mut failures = base_failures;
            failures.extend(candidate_failures);
            let effects = pair_effects(
                repo,
                &mut git_resources,
                verified_floor,
                external,
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
            setup.policy.errors_retained = setup_shell.errors_retained;
            setup.policy.complete_findings = scan_limits.complete_findings;
            conclude(
                &setup,
                (&base.discovery, base.side),
                (&candidate.discovery, candidate.side),
                &claims,
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
    })
}

/// Acquires both repository policies, each side on its own per-snapshot
/// ledger, producing the fatal projection on any defect.
fn pair_policies(
    repo: &Repository,
    git_resources: &mut GitResources,
    setup_shell: &SetupShell,
    base: (&(Oid, SnapshotIdentity), &mut ScanResources),
    candidate: (&(Oid, SnapshotIdentity), &mut ScanResources),
) -> PipelineResult<(crate::policy::PolicySide, crate::policy::PolicySide)> {
    let (base_tree, base_scan) = base;
    let (candidate_tree, candidate_scan) = candidate;
    let fallback = |details: Vec<ErrorDetail>| {
        let mut setup = setup_shell.with(
            base_tree.1.clone(),
            CandidateBlock::Commit(candidate_tree.1.clone()),
        );
        setup.controls_unavailable = Some(policy_unavailable_reason(&details));
        PipelineFailure::new(setup, details)
    };
    let base_policy =
        crate::policy::acquire(repo, git_resources, base_scan, &base_tree.0).map_err(&fallback)?;
    let candidate_policy =
        crate::policy::acquire(repo, git_resources, candidate_scan, &candidate_tree.0)
            .map_err(fallback)?;
    Ok((base_policy, candidate_policy))
}

/// `invalid-repository-policy` requires its `CONFIGURATION_INVALID` anchor;
/// any other acquisition failure leaves the controls merely not parsed.
fn policy_unavailable_reason(details: &[ErrorDetail]) -> &'static str {
    if details
        .iter()
        .any(|row| row.code == AnalysisErrorCode::ConfigurationInvalid)
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
                .then(|| RepoPath::new(path.to_owned()))
                .flatten(),
            path_bytes: None,
            resource: Some((*resource, *configured_limit, *observed_lower_bound)),
        },
        Error::Parse(_) | Error::Git(_) | Error::UnrepresentablePath | Error::Internal => {
            ErrorDetail {
                code: defect.code(),
                path: None,
                path_bytes: None,
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
    external: ExternalVerified,
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
    external.install(&mut effects);
    if let Err(row) = apply_floor(
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
/// first protected-path acquisition defect discards every pending comparison.
fn apply_floor(
    repo: &Repository,
    git_resources: &mut GitResources,
    floor: Option<&crate::policy::FloorInput>,
    base: (&SnapshotDiscovery, &mut ScanResources),
    candidate: (&SnapshotDiscovery, &mut ScanResources),
    effects: &mut crate::policy::Effects,
    acquire: bool,
) -> Result<(), ErrorDetail> {
    let Some(floor) = floor else {
        return Ok(());
    };
    effects.floor = Some((floor.floor.digest(), floor.trust_source.as_str()));
    effects.floor_raised = crate::policy::floor_raises(floor);
    effects.controls.extend(crate::policy::floor_inventory(
        floor,
        &inventory_lookup(candidate.0),
    ));
    if !acquire {
        return Ok(());
    }
    let (base_discovery, base_scan) = base;
    let (candidate_discovery, candidate_scan) = candidate;
    let controls = floor.floor.protected_control_paths().iter().try_fold(
        Vec::new(),
        |mut controls, path| {
            let states = crate::policy::protected_state(
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
            })
            .map_err(|defect| control_read_detail(&defect, path.as_str()))?;
            controls.extend(crate::policy::protected_control(path, states));
            Ok::<_, ErrorDetail>(controls)
        },
    )?;
    effects.controls.extend(controls);
    Ok(())
}

/// The candidate state of one inventory path under the obligation test.
fn inventory_lookup(
    discovery: &SnapshotDiscovery,
) -> impl Fn(&str) -> crate::policy::InventoryState {
    move |path: &str| {
        if let Some(record) = discovery.document(path.as_bytes()) {
            return match record.status {
                DocumentStatus::Scanned(_) => crate::policy::InventoryState::Scanned,
                DocumentStatus::ExcludedBuiltIn => crate::policy::InventoryState::Outside,
                DocumentStatus::Unsupported(_) | DocumentStatus::Failed(_) => {
                    crate::policy::InventoryState::Unsupported
                }
            };
        }
        if discovery.entries.contains_key(path.as_bytes()) {
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
    forge: Option<&ForgeContext>,
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
    let (side, failures) = staged_sides(
        repo,
        git_resources,
        candidate_scan,
        engine,
        forge,
        &discovery,
        base_failures,
        claims,
    );
    Ok((discovery, side, failures))
}

/// The staged candidate's observations plus every accumulated failure row;
/// a side that cannot be built at all is `None` with its defect appended.
#[expect(
    clippy::too_many_arguments,
    reason = "the staged pipeline context is the contract's"
)]
fn staged_sides(
    repo: &Repository,
    git_resources: &mut GitResources,
    scan_resources: &mut ScanResources,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    candidate_discovery: &SnapshotDiscovery,
    base_failures: Vec<ErrorDetail>,
    claims: &mut Vec<crate::claim::ClaimOutcome>,
) -> (Option<Side>, Vec<ErrorDetail>) {
    let mut failures = base_failures;
    match side_observations(
        repo,
        git_resources,
        scan_resources,
        engine,
        forge,
        candidate_discovery,
        Some(claims),
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

type StagedAcquisition = (
    crate::policy::PolicySide,
    crate::policy::PolicySide,
    crate::policy::Includes,
    (Evaluated, Vec<ErrorDetail>),
);

/// Both staged policies and the evaluated base, or the fatal projection of
/// whichever stage refused first.
#[expect(
    clippy::too_many_arguments,
    reason = "the staged pipeline context is the contract's"
)]
fn staged_acquisition(
    repo: &Repository,
    git_resources: &mut GitResources,
    scans: (&mut ScanResources, &mut ScanResources),
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    setup_shell: &SetupShell,
    base_placeholder: SnapshotIdentity,
    base_tree: (Oid, SnapshotIdentity),
    index: &amiss_git::LogicalIndex,
) -> PipelineResult<StagedAcquisition> {
    let (base_scan, candidate_scan) = scans;
    let (base_policy, candidate_policy, includes) = staged_policy(
        repo,
        git_resources,
        base_scan,
        candidate_scan,
        setup_shell,
        &base_placeholder,
        &base_tree.0,
        index,
    )?;
    let evaluated = staged_base(
        repo,
        git_resources,
        base_scan,
        engine,
        forge,
        &includes,
        setup_shell,
        base_placeholder,
        base_tree,
    )?;
    Ok((base_policy, candidate_policy, includes, evaluated))
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
    forge: Option<&ForgeContext>,
    includes: &crate::policy::Includes,
    setup_shell: &SetupShell,
    base_placeholder: SnapshotIdentity,
    base_tree: (Oid, SnapshotIdentity),
) -> PipelineResult<(Evaluated, Vec<ErrorDetail>)> {
    evaluate_tree(
        repo,
        git_resources,
        scan_resources,
        engine,
        forge,
        includes,
        base_tree,
        None,
    )
    .map_err(|defect_detail| {
        let setup = setup_shell.with(
            base_placeholder,
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
        );
        PipelineFailure::one(setup, defect_detail)
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

type Evaluation = Result<(Evaluated, Vec<ErrorDetail>), ErrorDetail>;

/// Both snapshot evaluations, with claim outcomes gathered on the candidate
/// side alone: a claim speaks for what the candidate asserts today.
#[expect(
    clippy::too_many_arguments,
    reason = "the staged pipeline context is the contract's"
)]
fn evaluated_pair(
    repo: &Repository,
    git_resources: &mut GitResources,
    scans: (&mut ScanResources, &mut ScanResources),
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    includes: &crate::policy::Includes,
    base_tree: (Oid, SnapshotIdentity),
    candidate_tree: (Oid, SnapshotIdentity),
) -> (Evaluation, Evaluation, Vec<crate::claim::ClaimOutcome>) {
    let (base_scan, candidate_scan) = scans;
    let base = evaluate_tree(
        repo,
        git_resources,
        base_scan,
        engine,
        forge,
        includes,
        base_tree,
        None,
    );
    candidate_scan.scans = std::mem::take(&mut base_scan.scans);
    let mut claims: Vec<crate::claim::ClaimOutcome> = Vec::new();
    let candidate = evaluate_tree(
        repo,
        git_resources,
        candidate_scan,
        engine,
        forge,
        includes,
        candidate_tree,
        Some(&mut claims),
    );
    (base, candidate, claims)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the staged pipeline context is the contract's"
)]
fn evaluate_tree(
    repo: &Repository,
    git_resources: &mut GitResources,
    scan_resources: &mut ScanResources,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    includes: &crate::policy::Includes,
    tree: (Oid, SnapshotIdentity),
    claims: Option<&mut Vec<crate::claim::ClaimOutcome>>,
) -> Result<(Evaluated, Vec<ErrorDetail>), ErrorDetail> {
    let (tree_oid, identity) = tree;
    let discovery = discover(repo, git_resources, scan_resources, includes, &tree_oid)
        .map_err(|defect| detail(&defect, None))?;
    let (side, failures) = side_observations(
        repo,
        git_resources,
        scan_resources,
        engine,
        forge,
        &discovery,
        claims,
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

/// Everything of the run identity except the two snapshot identities. The
/// external controls are wrapper-supplied, already-authenticated values; the
/// disposable CLI always passes none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupShell {
    pub engine: EngineProvenance,
    pub profile: amiss_wire::controls::Profile,
    pub repository: Option<amiss_wire::model::RepositoryIdentity>,
    pub forge: Option<amiss_wire::model::ForgeDialect>,
    pub candidate_ref: Option<String>,
    pub target_ref: Option<String>,
    pub default_branch_ref: Option<String>,
    pub floor: Option<crate::policy::FloorInput>,
    pub debt: Option<crate::policy::DebtInput>,
    pub waiver: Option<crate::policy::WaiverInput>,
    pub time: Option<crate::policy::TimeInput>,
    pub constraint: Option<crate::policy::ConstraintInput>,
    /// The wrapper lane's diagnostic request digests; none for the CLI.
    pub requests: crate::report::RequestDigests,
    /// A wrapper-established external-control defect, settled against the
    /// resolved snapshot identities exactly like a binding mismatch.
    pub external_defect: Option<(&'static str, ErrorDetail)>,
    /// The effective typed-analysis-errors-retained ceiling `E`: the
    /// built-in 64 until a verified floor tightens it, at which point the
    /// pipeline re-shells with the effective value so every fatal
    /// projection honors it.
    pub errors_retained: u64,
}

impl SetupShell {
    fn with(&self, base: SnapshotIdentity, candidate: CandidateBlock) -> Setup {
        Setup {
            engine: self.engine.clone(),
            profile: self.profile,
            repository: self.repository.clone(),
            forge: self.forge,
            candidate_ref: self.candidate_ref.clone(),
            target_ref: self.target_ref.clone(),
            default_branch_ref: self.default_branch_ref.clone(),
            base,
            candidate,
            policy: crate::policy::Effects {
                errors_retained: self.errors_retained,
                ..crate::policy::Effects::default()
            },
            controls_unavailable: None,
            requests: self.requests,
        }
    }
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
        format_str(repo.object_format()),
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

fn staged_tree(
    repo: &Repository,
    git_resources: &mut GitResources,
    setup_shell: &SetupShell,
    base_placeholder: &SnapshotIdentity,
    base_oid: &Oid,
) -> PipelineResult<(Oid, SnapshotIdentity)> {
    resolve_tree(repo, git_resources, base_oid).map_err(|defect_detail| {
        let setup = setup_shell.with(
            base_placeholder.clone(),
            CandidateBlock::Unavailable(vec!["not-evaluated"]),
        );
        PipelineFailure::one(setup, defect_detail)
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
        object_format: format_str(repo.object_format()),
        commit_oid: base_oid.as_str().to_owned(),
        tree_oid: base_oid.as_str().to_owned(),
    };
    let (initial, index, skip_worktree_paths) = pinned_index(repo, &mut git_resources)
        .map_err(|defect| candidate_unavailable(setup_shell, base_placeholder.clone(), &defect))?;
    let base_tree = staged_tree(
        repo,
        &mut git_resources,
        setup_shell,
        &base_placeholder,
        base_oid,
    )?;
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
    let (base_policy, candidate_policy, includes, (base_evaluated, base_failures)) =
        staged_acquisition(
            repo,
            &mut git_resources,
            (&mut base_scan, &mut candidate_scan),
            engine,
            forge,
            setup_shell,
            base_placeholder,
            base_tree,
            &index,
        )?;
    candidate_scan.scans = std::mem::take(&mut base_scan.scans);

    let mut claims: Vec<crate::claim::ClaimOutcome> = Vec::new();
    let (candidate_discovery, candidate_side, mut failures) = staged_candidate(
        repo,
        &mut git_resources,
        &mut candidate_scan,
        engine,
        forge,
        setup_shell,
        &base_evaluated.identity,
        &includes,
        &index,
        base_failures,
        &mut claims,
    )?;
    let effects = pair_effects(
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
