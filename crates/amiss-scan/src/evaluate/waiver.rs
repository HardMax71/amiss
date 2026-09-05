use std::collections::BTreeMap;

use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::model::UtcInstant;
use amiss_wire::report::FindingKind;
use amiss_wire::report::model::{ExceptionDiagnostic, WaiverExceptionDiagnosticKind};

use super::control::control_row;
use super::{Finding, candidate_digest_of};

fn waiver_diagnostic(
    item: &amiss_wire::controls::WaiverItem,
    bundle_digest: Digest,
    current_fact_digest: Option<Digest>,
) -> ExceptionDiagnostic {
    ExceptionDiagnostic::Waiver {
        kind: WaiverExceptionDiagnosticKind::Waiver,
        waiver_id: item.waiver_id.clone(),
        waiver_bundle_digest: bundle_digest,
        candidate_tree: item.candidate_tree.clone(),
        finding_key: item.finding_key,
        authorized_fact_digest: item.authorized_fact_digest,
        issuer: item.issuer.clone(),
        not_before: item.not_before.clone(),
        residual_disposition: item.residual_disposition,
        current_fact_digest,
        owner: item.owner.clone(),
        reason: item.reason.clone(),
        created_at: item.created_at.clone(),
        expires_at: item.expires_at.clone(),
    }
}

/// The selected-waiver pass: the closed defect rows in construction order,
/// with the finding-bound rows applicable only when the key names a current
/// candidate finding.
pub(super) fn waiver_pass(
    findings: &[Finding],
    targets: &BTreeMap<Digest, usize>,
    policy: &crate::policy::Effects,
    profile: Profile,
    instant: &UtcInstant,
    extra: &mut Vec<Finding>,
) -> Result<BTreeMap<Digest, usize>, crate::Error> {
    let mut waiver_valid: BTreeMap<Digest, usize> = BTreeMap::new();
    let Some(context) = &policy.waiver else {
        return Ok(waiver_valid);
    };
    for (index, item) in context.items.iter().enumerate() {
        if item.candidate_tree != context.candidate_tree {
            continue;
        }
        let target = targets.get(&item.finding_key).copied();
        let current = target.and_then(|found| findings.get(found).and_then(candidate_digest_of));
        let mut defects: Vec<&'static str> = Vec::new();
        if *instant < item.not_before {
            defects.push("not-yet");
        }
        if *instant >= item.expires_at {
            defects.push("expired");
        }
        if !context.authorized_issuers.contains(&item.issuer) {
            defects.push("issuer");
        }
        if !context
            .waivable_kinds
            .contains(&item.authorized_fact.finding_kind)
        {
            defects.push("kind");
        }
        if item.owner == item.issuer {
            defects.push("same-owner");
        }
        if let Some(found) = target {
            if findings.get(found).is_some_and(|finding| {
                finding.key_input.finding_kind.as_ref()
                    != item.authorized_fact.finding_kind.as_ref()
            }) {
                defects.push("key");
            }
            if current != Some(item.authorized_fact_digest) {
                defects.push("fact");
            }
        }
        for suffix in &defects {
            extra.push(control_row(
                FindingKind::WaiverInvalid,
                format!("waiver/{}/{suffix}", item.waiver_id.as_str()),
                None,
                (None, Some(context.digest)),
                Some(waiver_diagnostic(item, context.digest, current)),
                profile,
            )?);
        }
        if defects.is_empty() && target.is_some() {
            waiver_valid.insert(item.finding_key, index);
        }
    }
    Ok(waiver_valid)
}
