use std::collections::BTreeSet;

use amiss_git::{GitResources, ObjectKind, Repository, ValueCap, parse_tree};
use amiss_wire::controls::{
    Disposition as PolicyDisposition, GitMode, IncludeKind, ResourceName, SCANNER_POLICY_PATH,
    ScannerPolicy,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::Digest;
use amiss_wire::model::Oid;
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
    pub documents: BTreeSet<String>,
    pub trees: BTreeSet<String>,
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
                let path = include.path.as_str().to_owned();
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
    pub fn matches(&self, path: &str) -> bool {
        if self.documents.contains(path) {
            return true;
        }
        self.trees.iter().any(|root| {
            path == root
                || (path.len() > root.len()
                    && path.as_bytes().get(..root.len()) == Some(root.as_bytes())
                    && path.as_bytes().get(root.len()) == Some(&b'/'))
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
        path: Some(SCANNER_POLICY_PATH.to_owned()),
        resource: None,
    }];
    for code in details {
        if code != AnalysisErrorCode::ConfigurationInvalid {
            rows.push(ErrorDetail {
                code,
                path: Some(SCANNER_POLICY_PATH.to_owned()),
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
/// non-LFS regular blob with mode `100644`, read under the selected-control
/// caps, strictly parsed. Every other present form is configuration-invalid
/// at the policy path.
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
        resource: ResourceName::SelectedControlBlobBytes,
        limit: scan.limits().selected_control_blob_bytes,
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
                    path: Some(SCANNER_POLICY_PATH.to_owned()),
                    resource: Some((resource, configured_limit, observed_lower_bound)),
                },
                Error::Parse(_) | Error::Git(_) | Error::UnrepresentablePath | Error::Internal => {
                    ErrorDetail {
                        code: defect.code(),
                        path: Some(SCANNER_POLICY_PATH.to_owned()),
                        resource: None,
                    }
                }
            }]
        })?;
    scan.charge_control_bytes(u64::try_from(object.body.len()).unwrap_or(u64::MAX))
        .map_err(|defect| {
            vec![ErrorDetail {
                code: defect.code(),
                path: None,
                resource: match defect {
                    Error::ResourceLimit {
                        resource,
                        configured_limit,
                        observed_lower_bound,
                    } => Some((resource, configured_limit, observed_lower_bound)),
                    Error::Parse(_)
                    | Error::Git(_)
                    | Error::UnrepresentablePath
                    | Error::Internal => None,
                },
            }]
        })?;
    if lfs::is_pointer(&object.body) {
        return Err(invalid(Vec::new()));
    }
    match ScannerPolicy::parse(&object.body) {
        Ok(policy) => Ok(PolicySide {
            digest: Some(policy.digest),
            policy: Some(policy),
        }),
        Err(defect) => Err(invalid(vec![specific_code(&defect.kind)])),
    }
}

/// One control-plane finding the policy comparison produces, keyed by its
/// exact rule identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlSeed {
    pub kind: FindingKind,
    pub rule_id: String,
    pub control_path: Option<String>,
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

/// The complete policy effects on one run: the candidate's raise-only
/// dispositions, and the weakening and inventory-coverage control findings
/// derived from the base and candidate semantic sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub raised: Vec<(FindingKind, Disposition)>,
    pub controls: Vec<ControlSeed>,
    pub base_digest: Option<Digest>,
    pub candidate_digest: Option<Digest>,
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
        digest: amiss_wire::digest::hb("amiss/raw-evidence/v1", b""),
        document_includes: Vec::new(),
        protected_inventory: Vec::new(),
        finding_dispositions: Vec::new(),
    };
    let base_policy = base.policy.as_ref().unwrap_or(&empty);
    let candidate_policy = candidate.policy.as_ref().unwrap_or(&empty);

    for include in &base_policy.document_includes {
        let kept = candidate_policy
            .document_includes
            .iter()
            .any(|row| row.kind == include.kind && row.path.as_str() == include.path.as_str());
        if !kept {
            let rule = match include.kind {
                IncludeKind::Document => "policy/include-document-removed",
                IncludeKind::Tree => "policy/include-tree-removed",
            };
            controls.push(ControlSeed {
                kind: FindingKind::PolicyWeakened,
                rule_id: rule.to_owned(),
                control_path: Some(include.path.as_str().to_owned()),
            });
        }
    }
    for member in &base_policy.protected_inventory {
        let kept = candidate_policy
            .protected_inventory
            .iter()
            .any(|row| row.as_str() == member.as_str());
        if !kept {
            controls.push(ControlSeed {
                kind: FindingKind::PolicyWeakened,
                rule_id: "policy/inventory-removed".to_owned(),
                control_path: Some(member.as_str().to_owned()),
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
                control_path: Some(SCANNER_POLICY_PATH.to_owned()),
            });
        }
    }

    let mut inventory: BTreeSet<&str> = BTreeSet::new();
    inventory.extend(
        base_policy
            .protected_inventory
            .iter()
            .map(amiss_wire::model::RepoPath::as_str),
    );
    inventory.extend(
        candidate_policy
            .protected_inventory
            .iter()
            .map(amiss_wire::model::RepoPath::as_str),
    );
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
            control_path: Some(path.to_owned()),
        });
    }

    Effects {
        raised: candidate_raised,
        controls,
        base_digest: base.digest,
        candidate_digest: candidate.digest,
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
