use std::collections::BTreeMap;

use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::model::UtcInstant;
use amiss_wire::report::FindingKind;
use amiss_wire::report::model::{DebtExceptionDiagnosticKind, ExceptionDiagnostic};

use super::control::control_row;
use super::{Finding, candidate_digest_of};

fn debt_diagnostic(
    item: &amiss_wire::controls::DebtItem,
    context: &crate::policy::DebtContext,
    current_fact_digest: Digest,
) -> ExceptionDiagnostic {
    ExceptionDiagnostic::Debt {
        kind: DebtExceptionDiagnosticKind::Debt,
        debt_id: item.debt_id.clone(),
        debt_snapshot_digest: context.digest,
        adoption_tree: context.adoption_tree.clone(),
        accepted_fact_digest: item.accepted_fact_digest,
        current_fact_digest,
        owner: item.owner.clone(),
        reason: item.reason.clone(),
        created_at: item.created_at.clone(),
        expires_at: item.expires_at.clone(),
    }
}

/// The debt item pass: expiry before fact inequality, both defect rows able
/// to coexist, and a finding absent from the snapshot receiving no
/// treatment.
pub(super) fn debt_pass(
    findings: &[Finding],
    targets: &BTreeMap<Digest, usize>,
    policy: &crate::policy::Effects,
    profile: Profile,
    instant: &UtcInstant,
    extra: &mut Vec<Finding>,
) -> Result<BTreeMap<Digest, usize>, crate::Error> {
    let mut debt_valid: BTreeMap<Digest, usize> = BTreeMap::new();
    let Some(context) = &policy.debt else {
        return Ok(debt_valid);
    };
    for (index, item) in context.items.iter().enumerate() {
        let Some(target) = targets.get(&item.finding_key).copied() else {
            continue;
        };
        let Some(current) = findings.get(target).and_then(candidate_digest_of) else {
            continue;
        };
        let expired = *instant >= item.expires_at;
        let equal = current == item.accepted_fact_digest;
        if expired {
            extra.push(control_row(
                FindingKind::DebtExpired,
                format!("debt/{}/expired", item.debt_id.as_str()),
                None,
                (None, Some(context.digest)),
                Some(debt_diagnostic(item, context, current)),
                profile,
            )?);
        }
        if !equal {
            extra.push(control_row(
                FindingKind::DebtWorsened,
                format!("debt/{}/fact", item.debt_id.as_str()),
                None,
                (None, Some(context.digest)),
                Some(debt_diagnostic(item, context, current)),
                profile,
            )?);
        }
        if !expired && equal {
            debt_valid.insert(item.finding_key, index);
        }
    }
    Ok(debt_valid)
}
