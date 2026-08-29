use std::collections::BTreeMap;

use amiss_git::{GitResources, ObjectKind, Repository, parse_commit};
use amiss_wire::model::{ArtifactId, Oid, RepoPath};
use amiss_wire::report::{AnalysisErrorCode, EngineProvenance, ErrorDetail, adapter_contract};
use amiss_wire::resolution::{Missing, Resolution};

use crate::Error;
use crate::correlate::{Observation, Side, correlate, unique_path_pairs};
use crate::discovery::{DocumentStatus, SnapshotDiscovery, discover};
use crate::observe::{ObservationIdentity, observation_digest};
use crate::report::{Built, CandidateBlock, Setup, SnapshotIdentity, construct_incomplete};
use crate::resolve::{ForgeContext, Resolver, TargetCache};
use crate::resources::{ScanLimits, ScanResources};
use crate::semantic::RecordSet;

mod commit;
/// Verification and packaging of wrapper-supplied external controls shared
/// by both orchestration modes.
mod external;
mod staged;

pub use commit::commit_pair;
pub use staged::staged_index;

use external::ExternalVerified;

/// One side's full evaluation: discovery, then every scanned occurrence
/// resolved against this same snapshot.
struct Evaluated {
    identity: SnapshotIdentity,
    discovery: SnapshotDiscovery,
    side: Side,
}

#[derive(Default)]
pub(crate) struct CandidateOutcomes {
    claims: Vec<crate::claim::ClaimOutcome>,
    projections: Vec<crate::projection::Outcome>,
}

pub(crate) struct CandidateEvaluation<'a> {
    policy: Option<&'a amiss_wire::controls::ScannerPolicy>,
    record_sets: &'a BTreeMap<ArtifactId, RecordSet>,
    outcomes: &'a mut CandidateOutcomes,
}

/// One resolved snapshot root: its tree OID plus the full identity block.
type ResolvedTree = (Oid, SnapshotIdentity);

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

#[derive(Clone, Copy)]
pub(crate) struct ObservationContext<'a> {
    pub(crate) engine: &'a EngineProvenance,
    pub(crate) forge: Option<&'a ForgeContext>,
    pub(crate) semantic: crate::semantic::View<'a>,
}

/// Builds one side's observations from its discovery: every scanned
/// occurrence resolved against this same snapshot, and every failed document
/// or path defect carried as a typed error detail.
pub(crate) fn side_observations(
    repo: &Repository,
    git_resources: &mut GitResources,
    scan_resources: &mut ScanResources,
    context: ObservationContext<'_>,
    discovery: &SnapshotDiscovery,
    mut candidate: Option<CandidateEvaluation<'_>>,
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
        .filter_map(|record| match &record.status {
            DocumentStatus::Scanned(scanned) => Some(scanned.occurrences.len()),
            DocumentStatus::Failed(_)
            | DocumentStatus::ExcludedBuiltIn
            | DocumentStatus::Unsupported(_) => None,
        })
        .fold(0_usize, usize::saturating_add);
    let mut observations: Vec<Observation> = Vec::with_capacity(observation_count);
    let mut documents = BTreeMap::new();
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
                let (_descriptor, adapter_contract_digest) =
                    adapter_contract(context.engine, adapter);
                for occurrence in &scanned.occurrences {
                    let (intent, resolution, external_destination) = resolver
                        .resolve_scanned(
                            context.forge,
                            context.semantic,
                            adapter,
                            &record.path,
                            occurrence,
                        )
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
                        external_destination,
                        intent,
                        raw_destination: occurrence.occurrence.raw_destination.clone(),
                        raw_destination_digest: occurrence.raw_destination_digest,
                        projection_digest: occurrence.projection_digest,
                        resolution,
                        fragment_span: occurrence.occurrence.fragment_span,
                        path_span: occurrence.occurrence.path_span,
                    });
                }
                if let Some(candidate) = candidate.as_mut() {
                    document_claims(
                        &mut resolver,
                        (&record.path, scanned),
                        &mut candidate.outcomes.claims,
                    )?;
                }
            }
        }
    }
    if let Some(candidate) = candidate {
        evaluate_projections(&mut resolver, discovery, candidate)?;
    }
    Ok((
        Side {
            observations,
            documents,
        },
        failures,
    ))
}

fn evaluate_projections(
    resolver: &mut Resolver<'_>,
    discovery: &SnapshotDiscovery,
    candidate: CandidateEvaluation<'_>,
) -> Result<(), ErrorDetail> {
    let CandidateEvaluation {
        policy,
        record_sets,
        outcomes,
    } = candidate;
    let assertions = policy.map_or(
        &[][..],
        amiss_wire::controls::ScannerPolicy::projection_assertions,
    );
    for assertion in assertions {
        let document = RepoPath::from(&assertion.document);
        let outcome = crate::projection::evaluate(resolver, discovery, record_sets, assertion)
            .map_err(|defect| detail(&defect, Some(&document)))?;
        outcomes.projections.push(outcome);
    }
    Ok(())
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
    site: &crate::semantic::SiteEvaluation,
    outcomes: &CandidateOutcomes,
    failures: &[ErrorDetail],
) -> Built {
    if !failures.is_empty() {
        return construct_incomplete(setup, failures);
    }
    let mut candidate_side = candidate.1;
    let relocations = candidate_side
        .observations
        .iter()
        .find(|observation| {
            matches!(
                &observation.resolution,
                Resolution::Missing(Missing::PathNotFound { .. })
            )
        })
        .map_or_else(Default::default, |_| {
            unique_path_pairs(&base.0.entries, &candidate.0.entries)
        });
    for observation in &mut candidate_side.observations {
        if observation.intent.commit_oid.is_none()
            && let Resolution::Missing(Missing::PathNotFound {
                path,
                same_object_at,
                ..
            }) = &mut observation.resolution
        {
            *same_object_at = relocations.get(path).cloned();
        }
    }
    match correlate(base.1, candidate_side) {
        Ok(comparisons) => crate::report::construct_with_site(
            setup,
            base.0,
            candidate.0,
            comparisons,
            site,
            &outcomes.claims,
            &outcomes.projections,
        ),
        Err(defect) => construct_incomplete(setup, &[detail(&defect, None)]),
    }
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
) -> (crate::policy::Effects, crate::semantic::SiteEvaluation) {
    let mut effects = crate::policy::effects(
        base_policy,
        candidate_policy,
        &inventory_lookup(candidate.0),
    );
    let site = external.install(&mut effects);
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
    (effects, site)
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
    effects.floor = Some((floor.floor.digest(), floor.trust_source));
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
            object_format: repo.object_format().into(),
            commit_oid: commit_oid.as_str().to_owned(),
            tree_oid: commit.tree.as_str().to_owned(),
        },
    ))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the snapshot evaluation context is the contract's"
)]
fn evaluate_tree(
    repo: &Repository,
    git_resources: &mut GitResources,
    scan_resources: &mut ScanResources,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    semantic: crate::semantic::View<'_>,
    includes: &crate::policy::Includes,
    tree: (Oid, SnapshotIdentity),
    candidate: Option<CandidateEvaluation<'_>>,
) -> Result<(Evaluated, Vec<ErrorDetail>), ErrorDetail> {
    let (tree_oid, identity) = tree;
    let discovery = discover(repo, git_resources, scan_resources, includes, &tree_oid)
        .map_err(|defect| detail(&defect, None))?;
    let (side, failures) = side_observations(
        repo,
        git_resources,
        scan_resources,
        ObservationContext {
            engine,
            forge,
            semantic,
        },
        &discovery,
        candidate,
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
    pub semantic: crate::semantic::Input,
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
