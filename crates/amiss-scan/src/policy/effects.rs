use std::collections::{BTreeMap, BTreeSet};

use amiss_wire::controls::{
    Disposition as PolicyDisposition, DocumentInclude, FindingDisposition, IncludeKind,
    SCANNER_POLICY_PATH, ScannerPolicy,
};
use amiss_wire::digest::Digest;
use amiss_wire::model::{RepoPath, RepoPathText};
use amiss_wire::report::{Disposition, FindingKind};
use amiss_wire::requests::RequestTrust;

use super::acquire::PolicySide;

/// One control-plane finding the policy comparison produces, keyed by its
/// exact rule identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlSeed {
    pub kind: FindingKind,
    pub rule_id: String,
    pub control_path: Option<RepoPath>,
}

pub(super) fn disposition_rows(rows: &[FindingDisposition]) -> Vec<(FindingKind, Disposition)> {
    rows.iter()
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

fn raised(policy: Option<&ScannerPolicy>) -> Vec<(FindingKind, Disposition)> {
    policy.map_or_else(Vec::new, |policy| {
        disposition_rows(policy.finding_dispositions())
    })
}

fn include_weakening(
    base: &DocumentInclude,
    candidate: Option<&DocumentInclude>,
) -> Option<&'static str> {
    let Some(candidate) = candidate else {
        return Some(if base.suffix.is_some() {
            "policy/include-suffix-selector-removed"
        } else {
            match base.kind {
                IncludeKind::Document => "policy/include-document-removed",
                IncludeKind::Tree => "policy/include-tree-removed",
            }
        });
    };
    if base.suffix != candidate.suffix {
        return Some(if base.suffix.is_some() {
            "policy/include-suffix-removed"
        } else {
            "policy/include-tree-narrowed"
        });
    }
    base.adapter
        .filter(|adapter| candidate.adapter != Some(*adapter))
        .map(|_removed| "policy/include-binding-removed")
}

/// The verified debt snapshot as evaluation context: provenance plus the
/// items the finding projection matches by key and fact digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebtContext {
    pub digest: Digest,
    pub trust_source: RequestTrust,
    pub adoption_tree: amiss_wire::model::TreeIdentity,
    pub items: Vec<amiss_wire::controls::DebtItem>,
}

/// The verified waiver bundle as evaluation context: provenance, every item
/// for inventory, the current candidate tree that selects items, and the
/// floor's issuer and kind allow-lists selected-item semantics consult.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaiverContext {
    pub digest: Digest,
    pub trust_source: RequestTrust,
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
    pub floor: Option<(Digest, RequestTrust)>,
    pub debt: Option<DebtContext>,
    pub waiver: Option<WaiverContext>,
    pub time: Option<TimeContext>,
    pub constraint: Option<(
        amiss_wire::controls::ExecutionConstraintDescriptor,
        RequestTrust,
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
    let base_includes = base
        .policy
        .as_ref()
        .map_or(&[][..], ScannerPolicy::document_includes);
    let base_inventory = base
        .policy
        .as_ref()
        .map_or(&[][..], ScannerPolicy::protected_inventory);
    let candidate_includes: BTreeMap<(&str, IncludeKind), &DocumentInclude> = candidate
        .policy
        .as_ref()
        .map_or(&[][..], ScannerPolicy::document_includes)
        .iter()
        .map(|row| ((row.path.as_str(), row.kind), row))
        .collect();
    let candidate_inventory: BTreeSet<&str> = candidate
        .policy
        .as_ref()
        .map_or(&[][..], ScannerPolicy::protected_inventory)
        .iter()
        .map(RepoPathText::as_str)
        .collect();

    for include in base_includes {
        let candidate = candidate_includes
            .get(&(include.path.as_str(), include.kind))
            .copied();
        let rule = include_weakening(include, candidate);
        if let Some(rule) = rule {
            controls.push(ControlSeed {
                kind: FindingKind::PolicyWeakened,
                rule_id: rule.to_owned(),
                control_path: Some(RepoPath::from(&include.path)),
            });
        }
    }
    for member in base_inventory {
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
                rule_id: format!("policy/disposition/{}", kind.as_ref()),
                control_path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
            });
        }
    }

    let mut inventory: BTreeSet<&str> = BTreeSet::new();
    inventory.extend(base_inventory.iter().map(RepoPathText::as_str));
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
