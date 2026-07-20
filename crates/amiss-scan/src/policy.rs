use std::collections::BTreeSet;

use amiss_git::{GitResources, ObjectKind, Repository, ValueCap, parse_tree};
use amiss_wire::controls::{
    Disposition as PolicyDisposition, GitMode, IncludeKind, ResourceName, SCANNER_POLICY_PATH,
    ScannerPolicy,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::Digest;
use amiss_wire::model::{Oid, RepoPath};
use amiss_wire::report::{AnalysisErrorCode, Disposition, ErrorDetail, FindingKind};

use crate::resources::ScanResources;
use crate::{Error, lfs};

/// One side's acquired repository policy: the digest is null exactly when the
/// path is absent, and absence has empty semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PolicySide {
    pub digest: Option<Digest>,
    pub policy: Option<ScannerPolicy>,
}

/// The union of both sides' includes, which fixes classification row five and
/// overrides built-in exclusion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Includes {
    pub documents: BTreeSet<RepoPath>,
    pub trees: BTreeSet<RepoPath>,
}

impl Includes {
    #[must_use]
    pub fn union(base: &PolicySide, candidate: &PolicySide) -> Self {
        let mut merged = Self::default();
        for side in [base, candidate] {
            let Some(policy) = &side.policy else {
                continue;
            };
            for include in &policy.document_includes {
                let path = RepoPath::from(&include.path);
                match include.kind {
                    IncludeKind::Document => {
                        merged.documents.insert(path);
                    }
                    IncludeKind::Tree => {
                        merged.trees.insert(path);
                    }
                }
            }
        }
        merged
    }

    /// A document include matches exactly its path; a tree include matches the
    /// root itself and paths beginning `root + "/"`, bytewise.
    #[must_use]
    pub fn matches(&self, path: &RepoPath) -> bool {
        if self.documents.contains(path) {
            return true;
        }
        let raw = path.as_bytes();
        self.trees.contains(raw)
            || raw
                .iter()
                .enumerate()
                .rev()
                .filter(|(_, byte)| **byte == b'/')
                .any(|(separator, _)| {
                    raw.get(..separator)
                        .is_some_and(|ancestor| self.trees.contains(ancestor))
                })
    }
}

fn specific_code(kind: &ErrorKind) -> AnalysisErrorCode {
    match kind {
        ErrorKind::Json(_) => AnalysisErrorCode::InvalidJson,
        ErrorKind::UnknownField => AnalysisErrorCode::UnknownField,
        ErrorKind::DigestMismatch => AnalysisErrorCode::DigestMismatch,
        ErrorKind::UnsortedSet | ErrorKind::DuplicateMember => AnalysisErrorCode::NoncanonicalArray,
        ErrorKind::MissingField
        | ErrorKind::WrongType
        | ErrorKind::InvalidValue
        | ErrorKind::LimitExceeded
        | ErrorKind::Inconsistent => AnalysisErrorCode::ConfigurationInvalid,
    }
}

fn invalid(details: Vec<AnalysisErrorCode>) -> Vec<ErrorDetail> {
    let mut rows = vec![ErrorDetail {
        code: AnalysisErrorCode::ConfigurationInvalid,
        path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
        path_bytes: None,
        resource: None,
    }];
    for code in details {
        if code != AnalysisErrorCode::ConfigurationInvalid {
            rows.push(ErrorDetail {
                code,
                path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
                path_bytes: None,
                resource: None,
            });
        }
    }
    rows
}

/// Finds the exact policy path in a snapshot tree without discovering the
/// snapshot, so policy validation can precede discovery as the fatal order
/// requires.
fn locate(
    repo: &Repository,
    git: &mut GitResources,
    root_tree: &Oid,
) -> Result<Option<(GitMode, Oid)>, Error> {
    let mut components = SCANNER_POLICY_PATH.split('/').peekable();
    let mut tree_oid = root_tree.clone();
    while let Some(component) = components.next() {
        let object = repo
            .read_expected(git, &tree_oid, ObjectKind::Tree)
            .map_err(Error::from)?;
        let entries = parse_tree(repo.object_format(), &object.body).map_err(Error::from)?;
        let Some(entry) = entries
            .iter()
            .find(|entry| entry.name == component.as_bytes())
        else {
            return Ok(None);
        };
        if components.peek().is_none() {
            return Ok(Some((entry.mode, entry.oid.clone())));
        }
        if entry.mode != GitMode::Tree {
            return Ok(None);
        }
        tree_oid = entry.oid.clone();
    }
    Ok(None)
}

/// Acquires one side's policy under the exact object-form law: an ordinary
/// non-LFS regular blob with mode `100644`, read under the configuration
/// `control-input-bytes` cap, strictly parsed. Every other present form is
/// configuration-invalid at the policy path.
///
/// # Errors
///
/// The complete typed error rows for an invalid policy; acquisition defects
/// below the policy itself propagate as their own codes.
pub fn acquire(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    root_tree: &Oid,
) -> Result<PolicySide, Vec<ErrorDetail>> {
    let located = locate(repo, git, root_tree).map_err(|defect| {
        vec![ErrorDetail {
            code: defect.code(),
            path: None,
            path_bytes: None,
            resource: None,
        }]
    })?;
    let Some((mode, oid)) = located else {
        return Ok(PolicySide::default());
    };
    acquire_entry(repo, git, scan, mode, &oid)
}

/// Acquires a located policy entry under the same object-form law, for a
/// snapshot whose entries are already enumerated.
///
/// # Errors
///
/// Exactly as `acquire`.
pub fn acquire_entry(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    mode: GitMode,
    oid: &Oid,
) -> Result<PolicySide, Vec<ErrorDetail>> {
    if mode != GitMode::RegularFile {
        return Err(invalid(Vec::new()));
    }
    let cap = ValueCap {
        resource: ResourceName::ControlInputBytes,
        limit: scan.limits().control_input_bytes,
    };
    let object = repo
        .read_expected_capped(git, oid, ObjectKind::Blob, cap)
        .map_err(|defect| {
            let defect = Error::from(defect);
            vec![match defect {
                Error::ResourceLimit {
                    resource,
                    configured_limit,
                    observed_lower_bound,
                } => ErrorDetail {
                    code: AnalysisErrorCode::ResourceLimitExceeded,
                    path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
                    path_bytes: None,
                    resource: Some((resource, configured_limit, observed_lower_bound)),
                },
                Error::Parse(_) | Error::Git(_) | Error::UnrepresentablePath | Error::Internal => {
                    ErrorDetail {
                        code: defect.code(),
                        path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
                        path_bytes: None,
                        resource: None,
                    }
                }
            }]
        })?;
    if lfs::is_pointer(&object.body) {
        return Err(invalid(Vec::new()));
    }
    match ScannerPolicy::parse(&object.body) {
        Ok(policy) => {
            let entries = [
                policy.document_includes.len(),
                policy.protected_inventory.len(),
                policy.finding_dispositions.len(),
            ]
            .iter()
            .map(|&len| u64::try_from(len).unwrap_or(u64::MAX))
            .fold(0_u64, u64::saturating_add);
            let limit = scan.limits().repository_policy_entries;
            if entries > limit {
                return Err(vec![ErrorDetail {
                    code: AnalysisErrorCode::ResourceLimitExceeded,
                    path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
                    path_bytes: None,
                    resource: Some((
                        ResourceName::RepositoryPolicyEntries,
                        limit,
                        limit.saturating_add(1),
                    )),
                }]);
            }
            Ok(PolicySide {
                digest: Some(policy.digest),
                policy: Some(policy),
            })
        }
        Err(defect) => Err(invalid(vec![specific_code(&defect.kind)])),
    }
}

/// One control-plane finding the policy comparison produces, keyed by its
/// exact rule identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlSeed {
    pub kind: FindingKind,
    pub rule_id: String,
    pub control_path: Option<RepoPath>,
}

fn raised(policy: Option<&ScannerPolicy>) -> Vec<(FindingKind, Disposition)> {
    let Some(policy) = policy else {
        return Vec::new();
    };
    policy
        .finding_dispositions
        .iter()
        .map(|row| {
            let kind = match row.finding_kind {
                amiss_wire::controls::PromotableFindingKind::ExplicitTargetMissing => {
                    FindingKind::ExplicitTargetMissing
                }
                amiss_wire::controls::PromotableFindingKind::ExplicitTargetTypeMismatch => {
                    FindingKind::ExplicitTargetTypeMismatch
                }
                amiss_wire::controls::PromotableFindingKind::InvalidReference => {
                    FindingKind::InvalidReference
                }
            };
            let disposition = match row.disposition {
                PolicyDisposition::Warn => Disposition::Warn,
                PolicyDisposition::Fail => Disposition::Fail,
            };
            (kind, disposition)
        })
        .collect()
}

/// The verified debt snapshot as evaluation context: provenance plus the
/// items the finding projection matches by key and fact digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebtContext {
    pub digest: Digest,
    pub trust_source: &'static str,
    pub adoption_tree: amiss_wire::model::TreeIdentity,
    pub items: Vec<amiss_wire::controls::DebtItem>,
}

/// The verified waiver bundle as evaluation context: provenance, every item
/// for inventory, the current candidate tree that selects items, and the
/// floor's issuer and kind allow-lists selected-item semantics consult.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaiverContext {
    pub digest: Digest,
    pub trust_source: &'static str,
    pub candidate_tree: amiss_wire::model::TreeIdentity,
    pub items: Vec<amiss_wire::controls::WaiverItem>,
    pub authorized_issuers: Vec<amiss_wire::model::OwnerId>,
    pub waivable_kinds: Vec<amiss_wire::controls::EligibleFindingKind>,
}

/// The verified trusted-time statement: the report's evaluation instant is
/// exactly its `evaluation_instant`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeContext {
    pub statement: amiss_wire::controls::TrustedTimeStatement,
    pub digest: Digest,
}

/// The complete policy effects on one run: the candidate's raise-only
/// dispositions, the weakening and inventory-coverage control findings
/// derived from the base and candidate semantic sets, and the verified
/// external controls the wrapper supplied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effects {
    pub raised: Vec<(FindingKind, Disposition)>,
    pub floor_raised: Vec<(FindingKind, Disposition)>,
    pub controls: Vec<ControlSeed>,
    pub base_digest: Option<Digest>,
    pub candidate_digest: Option<Digest>,
    pub floor: Option<(Digest, &'static str)>,
    pub debt: Option<DebtContext>,
    pub waiver: Option<WaiverContext>,
    pub time: Option<TimeContext>,
    pub constraint: Option<(
        amiss_wire::controls::ExecutionConstraintDescriptor,
        &'static str,
    )>,
    /// The effective typed-analysis-errors-retained ceiling `E`:
    /// `min(64, verified floor limit)`, the built-in 64 without a floor.
    pub errors_retained: u64,
    /// The effective complete-findings ceiling: the built-in 100,000, which a
    /// verified floor may only tighten.
    pub complete_findings: u64,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            raised: Vec::new(),
            floor_raised: Vec::new(),
            controls: Vec::new(),
            base_digest: None,
            candidate_digest: None,
            floor: None,
            debt: None,
            waiver: None,
            time: None,
            constraint: None,
            errors_retained: 64,
            complete_findings: crate::resources::ScanLimits::CONTRACT.complete_findings,
        }
    }
}

/// Compares the two sides and evaluates the inventory union against the
/// candidate document coverage.
#[must_use]
pub fn effects(
    base: &PolicySide,
    candidate: &PolicySide,
    candidate_documents: &dyn Fn(&str) -> InventoryState,
) -> Effects {
    let mut controls: Vec<ControlSeed> = Vec::new();
    let empty = ScannerPolicy {
        digest: amiss_wire::digest::hb("amiss/raw-evidence", b""),
        document_includes: Vec::new(),
        protected_inventory: Vec::new(),
        finding_dispositions: Vec::new(),
    };
    let base_policy = base.policy.as_ref().unwrap_or(&empty);
    let candidate_policy = candidate.policy.as_ref().unwrap_or(&empty);
    let candidate_includes: BTreeSet<(&str, IncludeKind)> = candidate_policy
        .document_includes
        .iter()
        .map(|row| (row.path.as_str(), row.kind))
        .collect();
    let candidate_inventory: BTreeSet<&str> = candidate_policy
        .protected_inventory
        .iter()
        .map(amiss_wire::model::RepoPathText::as_str)
        .collect();

    for include in &base_policy.document_includes {
        if !candidate_includes.contains(&(include.path.as_str(), include.kind)) {
            let rule = match include.kind {
                IncludeKind::Document => "policy/include-document-removed",
                IncludeKind::Tree => "policy/include-tree-removed",
            };
            controls.push(ControlSeed {
                kind: FindingKind::PolicyWeakened,
                rule_id: rule.to_owned(),
                control_path: Some(RepoPath::from(&include.path)),
            });
        }
    }
    for member in &base_policy.protected_inventory {
        if !candidate_inventory.contains(member.as_str()) {
            controls.push(ControlSeed {
                kind: FindingKind::PolicyWeakened,
                rule_id: "policy/inventory-removed".to_owned(),
                control_path: Some(RepoPath::from(member)),
            });
        }
    }
    let base_raised = raised(base.policy.as_ref());
    let candidate_raised = raised(candidate.policy.as_ref());
    for (kind, strength) in &base_raised {
        let now = candidate_raised
            .iter()
            .find(|(candidate_kind, _)| candidate_kind == kind)
            .map(|(_, disposition)| *disposition);
        if now.is_none_or(|disposition| disposition < *strength) {
            controls.push(ControlSeed {
                kind: FindingKind::PolicyWeakened,
                rule_id: format!("policy/disposition/{}", kind.as_str()),
                control_path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
            });
        }
    }

    let mut inventory: BTreeSet<&str> = BTreeSet::new();
    inventory.extend(
        base_policy
            .protected_inventory
            .iter()
            .map(amiss_wire::model::RepoPathText::as_str),
    );
    inventory.extend(candidate_inventory);
    for path in inventory {
        let rule = match candidate_documents(path) {
            InventoryState::Scanned => continue,
            InventoryState::Missing => "coverage/repository-inventory-missing",
            InventoryState::Unsupported => "coverage/repository-inventory-unsupported",
            InventoryState::Outside => "coverage/repository-inventory-outside",
        };
        controls.push(ControlSeed {
            kind: FindingKind::CoverageReduced,
            rule_id: rule.to_owned(),
            control_path: RepoPath::new(path.to_owned()),
        });
    }

    Effects {
        raised: candidate_raised,
        floor_raised: Vec::new(),
        controls,
        base_digest: base.digest,
        candidate_digest: candidate.digest,
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        errors_retained: 64,
        complete_findings: crate::resources::ScanLimits::CONTRACT.complete_findings,
    }
}

/// One inventory path's candidate state under the obligation test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryState {
    Scanned,
    Unsupported,
    Missing,
    Outside,
}

/// A verified organization floor as the wrapper supplies it: the parsed
/// value plus the external trust source that authorized it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FloorInput {
    pub floor: amiss_wire::controls::OrganizationFloor,
    pub trust_source: TrustSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustSource {
    ExternalRequiredCheck,
    OrganizationPolicy,
}

impl TrustSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExternalRequiredCheck => "external-required-check",
            Self::OrganizationPolicy => "organization-policy",
        }
    }
}

/// The floor's binding: its repository and full ref must equal the run's,
/// and the selected profile must be at least the floor minimum under
/// `observe < enforce`. Any violation is a control-binding mismatch.
///
/// # Errors
///
/// One `CONTROL_BINDING_MISMATCH` detail.
pub fn verify_floor(
    input: &FloorInput,
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    target_ref: Option<&str>,
    enforce: bool,
) -> Result<(), ErrorDetail> {
    let mismatch = ErrorDetail {
        code: AnalysisErrorCode::ControlBindingMismatch,
        path: None,
        path_bytes: None,
        resource: None,
    };
    let Some(identity) = repository else {
        return Err(mismatch);
    };
    let floor = &input.floor;
    if floor.repository != *identity {
        return Err(mismatch);
    }
    if target_ref != Some(floor.ref_name.as_str()) {
        return Err(mismatch);
    }
    let minimum_enforce = matches!(
        floor.minimum_profile,
        amiss_wire::controls::Profile::Enforce
    );
    if minimum_enforce && !enforce {
        return Err(mismatch);
    }
    Ok(())
}

/// A verified debt snapshot as the wrapper supplies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebtInput {
    pub snapshot: amiss_wire::controls::DebtSnapshot,
    pub trust_source: TrustSource,
}

/// A verified waiver bundle as the wrapper supplies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaiverInput {
    pub bundle: amiss_wire::controls::WaiverBundle,
    pub trust_source: TrustSource,
}

/// The trusted-time statement plus the wrapper's provider-authenticated run
/// context the statement must identify.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeInput {
    pub statement: amiss_wire::controls::TrustedTimeStatement,
    pub provider: String,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
}

/// A verified execution constraint as the wrapper supplies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstraintInput {
    pub descriptor: amiss_wire::controls::ExecutionConstraintDescriptor,
    pub trust_source: TrustSource,
}

const fn binding_mismatch_row() -> ErrorDetail {
    ErrorDetail {
        code: AnalysisErrorCode::ControlBindingMismatch,
        path: None,
        path_bytes: None,
        resource: None,
    }
}

fn identity_matches(
    control: &amiss_wire::model::RepositoryIdentity,
    control_ref: &amiss_wire::model::BranchRef,
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    target_ref: Option<&str>,
) -> bool {
    repository == Some(control) && target_ref == Some(control_ref.as_str())
}

/// The statement's evaluation bindings: repository, ref, candidate identity,
/// and the provider run the wrapper authenticated. Shape and TTL were
/// established at parse.
///
/// # Errors
///
/// One `TRUSTED_TIME_INVALID` detail.
pub fn verify_time(
    input: &TimeInput,
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    target_ref: Option<&str>,
    candidate_identity: &Digest,
) -> Result<(), ErrorDetail> {
    let statement = &input.statement;
    let bound = identity_matches(
        &statement.repository,
        &statement.ref_name,
        repository,
        target_ref,
    ) && statement.candidate_identity_digest == *candidate_identity
        && statement.provider == input.provider
        && statement.provider_run_id == input.provider_run_id
        && statement.provider_run_attempt == input.provider_run_attempt;
    if bound {
        Ok(())
    } else {
        Err(ErrorDetail {
            code: AnalysisErrorCode::TrustedTimeInvalid,
            path: None,
            path_bytes: None,
            resource: None,
        })
    }
}

/// The snapshot-level debt binding: repository, ref, the verified floor's
/// digest, every owner on the floor's allow-list, causal time bounds against
/// the trusted instant, and the effective item ceiling.
///
/// # Errors
///
/// One `CONTROL_BINDING_MISMATCH` detail, or the `debt-items` crossing.
pub fn verify_debt(
    input: &DebtInput,
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    target_ref: Option<&str>,
    floor: Option<&FloorInput>,
    instant: &amiss_wire::model::UtcInstant,
    item_limit: u64,
) -> Result<(), ErrorDetail> {
    let snapshot = &input.snapshot;
    if u64::try_from(snapshot.items.len()).unwrap_or(u64::MAX) > item_limit {
        return Err(ErrorDetail {
            code: AnalysisErrorCode::ResourceLimitExceeded,
            path: None,
            path_bytes: None,
            resource: Some((
                ResourceName::DebtItems,
                item_limit,
                item_limit.saturating_add(1),
            )),
        });
    }
    let Some(floor) = floor else {
        return Err(binding_mismatch_row());
    };
    let bound = identity_matches(
        &snapshot.repository,
        &snapshot.ref_name,
        repository,
        target_ref,
    ) && snapshot.organization_floor_digest == floor.floor.digest
        && snapshot.created_at <= *instant
        && snapshot.items.iter().all(|item| {
            item.created_at <= *instant && floor.floor.authorized_debt_owners.contains(&item.owner)
        });
    if bound {
        Ok(())
    } else {
        Err(binding_mismatch_row())
    }
}

/// The bundle-level waiver binding: repository, ref, the verified floor's
/// digest, bundle creation not after the trusted instant, and the effective
/// item ceiling. Issuer, kind, owner distinction, activity, and body
/// agreement are selected-item semantics, not binding.
///
/// # Errors
///
/// One `CONTROL_BINDING_MISMATCH` detail, or the `waiver-items` crossing.
pub fn verify_waiver(
    input: &WaiverInput,
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    target_ref: Option<&str>,
    floor: Option<&FloorInput>,
    instant: &amiss_wire::model::UtcInstant,
    item_limit: u64,
) -> Result<(), ErrorDetail> {
    let bundle = &input.bundle;
    if u64::try_from(bundle.items.len()).unwrap_or(u64::MAX) > item_limit {
        return Err(ErrorDetail {
            code: AnalysisErrorCode::ResourceLimitExceeded,
            path: None,
            path_bytes: None,
            resource: Some((
                ResourceName::WaiverItems,
                item_limit,
                item_limit.saturating_add(1),
            )),
        });
    }
    let Some(floor) = floor else {
        return Err(binding_mismatch_row());
    };
    let bound = identity_matches(&bundle.repository, &bundle.ref_name, repository, target_ref)
        && bundle.organization_floor_digest == floor.floor.digest
        && bundle.created_at <= *instant;
    if bound {
        Ok(())
    } else {
        Err(binding_mismatch_row())
    }
}

/// One protected control path's state on one side: absent, present with its
/// exact protected-control-evidence digest, or unsupported because the entry
/// is a tree, symlink, gitlink, or recognized LFS pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtectedState {
    Absent,
    Unsupported,
    Present(Digest),
}

pub const PROTECTED_CONTROL_EVIDENCE_DOMAIN: &str = "amiss/scanner-protected-control-evidence";

/// Reads one protected control path's state from a snapshot: the evidence
/// digest binds path, mode, and raw digest; the blob is size-checked under
/// the selected-control resources and never parsed or executed.
///
/// # Errors
///
/// Snapshot-level acquisition defects and control byte crossings.
pub fn protected_state(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    entries: &std::collections::BTreeMap<RepoPath, (GitMode, Oid)>,
    path: &str,
) -> Result<ProtectedState, Error> {
    let Some((mode, oid)) = entries.get(path.as_bytes()) else {
        return Ok(ProtectedState::Absent);
    };
    match mode {
        GitMode::Tree | GitMode::Gitlink | GitMode::Symlink => {
            return Ok(ProtectedState::Unsupported);
        }
        GitMode::RegularFile | GitMode::ExecutableFile => {}
    }
    let cap = ValueCap {
        resource: ResourceName::SelectedControlBlobBytes,
        limit: scan.limits().selected_control_blob_bytes,
    };
    let object = repo
        .read_expected_capped(git, oid, ObjectKind::Blob, cap)
        .map_err(Error::from)?;
    scan.charge_control_bytes(u64::try_from(object.body.len()).unwrap_or(u64::MAX))?;
    if lfs::is_pointer(&object.body) {
        return Ok(ProtectedState::Unsupported);
    }
    let raw = amiss_wire::digest::hb(crate::resolve::RAW_EVIDENCE_DOMAIN, &object.body);
    let descriptor = amiss_wire::json::Value::Object(vec![
        (
            "git_mode".to_owned(),
            amiss_wire::json::Value::String(mode.as_str().to_owned()),
        ),
        (
            "path".to_owned(),
            amiss_wire::json::Value::String(path.to_owned()),
        ),
        (
            "raw_digest".to_owned(),
            amiss_wire::json::Value::String(raw.to_string()),
        ),
    ]);
    Ok(ProtectedState::Present(amiss_wire::digest::hj(
        PROTECTED_CONTROL_EVIDENCE_DOMAIN,
        &descriptor,
    )))
}

/// The floor inventory obligation over the candidate: every protected
/// inventory path that is not a scanned candidate document emits its exact
/// floor coverage rule.
#[must_use]
pub fn floor_inventory(
    input: &FloorInput,
    candidate_documents: &dyn Fn(&str) -> InventoryState,
) -> Vec<ControlSeed> {
    let mut controls = Vec::new();
    for path in &input.floor.protected_inventory {
        let rule = match candidate_documents(path.as_str()) {
            InventoryState::Scanned => continue,
            InventoryState::Missing => "coverage/floor-inventory-missing",
            InventoryState::Unsupported => "coverage/floor-inventory-unsupported",
            InventoryState::Outside => "coverage/floor-inventory-outside",
        };
        controls.push(ControlSeed {
            kind: FindingKind::CoverageReduced,
            rule_id: rule.to_owned(),
            control_path: Some(RepoPath::from(path)),
        });
    }
    controls
}

/// The protected control paths compared across sides: emit when base and
/// candidate state or source differs, or whenever the candidate state is not
/// present; the same present descriptor on both sides emits nothing.
#[must_use]
pub fn floor_protected(
    input: &FloorInput,
    protected: &dyn Fn(&str) -> (ProtectedState, ProtectedState),
) -> Vec<ControlSeed> {
    let mut controls = Vec::new();
    for path in &input.floor.protected_control_paths {
        let (base, candidate) = protected(path.as_str());
        let unchanged = matches!(
            (base, candidate),
            (ProtectedState::Present(left), ProtectedState::Present(right)) if left == right
        );
        if !unchanged {
            controls.push(ControlSeed {
                kind: FindingKind::ControlPlaneChanged,
                rule_id: "control/protected-path".to_owned(),
                control_path: Some(RepoPath::from(path)),
            });
        }
    }
    controls
}

/// The floor's raise-only disposition rows, applied after the repository
/// steps in the policy trace.
#[must_use]
pub fn floor_raises(input: &FloorInput) -> Vec<(FindingKind, Disposition)> {
    input
        .floor
        .minimum_dispositions
        .iter()
        .map(|row| {
            let kind = match row.finding_kind {
                amiss_wire::controls::PromotableFindingKind::ExplicitTargetMissing => {
                    FindingKind::ExplicitTargetMissing
                }
                amiss_wire::controls::PromotableFindingKind::ExplicitTargetTypeMismatch => {
                    FindingKind::ExplicitTargetTypeMismatch
                }
                amiss_wire::controls::PromotableFindingKind::InvalidReference => {
                    FindingKind::InvalidReference
                }
            };
            let disposition = match row.disposition {
                PolicyDisposition::Warn => Disposition::Warn,
                PolicyDisposition::Fail => Disposition::Fail,
            };
            (kind, disposition)
        })
        .collect()
}

/// A floor may only tighten built-in limits, never raise them; unmapped
/// resources belong to layers the local scanner does not own.
#[must_use]
pub fn tightened_limits(
    scan: crate::resources::ScanLimits,
    git: amiss_git::GitLimits,
    floor: &amiss_wire::controls::OrganizationFloor,
) -> (crate::resources::ScanLimits, amiss_git::GitLimits) {
    let mut scan = scan;
    let mut git = git;
    for row in &floor.resource_limits {
        let maximum = u64::try_from(row.maximum).unwrap_or(u64::MAX);
        let slot: Option<&mut u64> = match row.resource {
            ResourceName::DocumentsPerSnapshot => Some(&mut scan.documents_per_snapshot),
            ResourceName::DocumentBlobBytes => Some(&mut scan.document_blob_bytes),
            ResourceName::AggregateDocumentBytesPerSnapshot => {
                Some(&mut scan.aggregate_document_bytes_per_snapshot)
            }
            ResourceName::RawLinkDestinationBytes => Some(&mut scan.raw_link_destination_bytes),
            ResourceName::ParserNesting => Some(&mut scan.parser_nesting),
            ResourceName::ParserNodesPerDocument => Some(&mut scan.parser_nodes_per_document),
            ResourceName::ParserNodesPerSnapshot => Some(&mut scan.parser_nodes_per_snapshot),
            ResourceName::AggregateEmbeddedCodeEvaluationBytesPerSnapshot => {
                Some(&mut scan.aggregate_embedded_code_evaluation_bytes_per_snapshot)
            }
            ResourceName::ReferencesPerDocument => Some(&mut scan.references_per_document),
            ResourceName::ReferencesPerSnapshot => Some(&mut scan.references_per_snapshot),
            ResourceName::ReferencedTargetBlobBytes => Some(&mut scan.referenced_target_blob_bytes),
            ResourceName::AggregateReferencedTargetBytesPerSnapshot => {
                Some(&mut scan.aggregate_referenced_target_bytes_per_snapshot)
            }
            ResourceName::AggregateLineFragmentEvaluationBytesPerSnapshot => {
                Some(&mut scan.aggregate_line_fragment_evaluation_bytes_per_snapshot)
            }
            ResourceName::SelectedControlBlobBytes => Some(&mut scan.selected_control_blob_bytes),
            ResourceName::AggregateSelectedControlBytesPerSnapshot => {
                Some(&mut scan.aggregate_selected_control_bytes_per_snapshot)
            }
            ResourceName::GitObjectBytes => Some(&mut git.inflated_object_bytes),
            ResourceName::GitCompressedObjectBytes => Some(&mut git.compressed_stream_bytes),
            ResourceName::AggregateGitCompressedObjectBytesPerEvaluation => {
                Some(&mut git.aggregate_compressed_bytes)
            }
            ResourceName::GitPackDirectoryEntries => Some(&mut git.pack_directory_entries),
            ResourceName::GitPackFiles => Some(&mut git.pack_files),
            ResourceName::GitPackIndexBytes => Some(&mut git.pack_index_bytes),
            ResourceName::AggregateGitPackIndexBytes => Some(&mut git.aggregate_pack_index_bytes),
            ResourceName::GitDeltaDepth => Some(&mut git.delta_depth),
            ResourceName::GitIndexBytes => Some(&mut git.index_bytes),
            ResourceName::GitTreeEntriesPerSnapshot => Some(&mut git.tree_entries_per_snapshot),
            ResourceName::RawPathBytes => Some(&mut git.raw_path_bytes),
            ResourceName::ControlInputBytes => Some(&mut scan.control_input_bytes),
            ResourceName::RepositoryPolicyEntries => Some(&mut scan.repository_policy_entries),
            ResourceName::DebtItems => Some(&mut scan.debt_items),
            ResourceName::WaiverItems => Some(&mut scan.waiver_items),
            ResourceName::TypedAnalysisErrorsRetained => Some(&mut scan.errors_retained),
            ResourceName::CompleteFindings => Some(&mut scan.complete_findings),
            ResourceName::OrganizationPolicyEntries
            | ResourceName::MachineJsonBytes
            | ResourceName::PrivateTemporaryStorageBytes
            | ResourceName::EvaluatorManagedMemoryBytes => None,
        };
        if let Some(limit) = slot {
            *limit = (*limit).min(maximum);
        }
    }
    (scan, git)
}
