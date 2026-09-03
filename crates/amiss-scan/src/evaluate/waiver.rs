use std::collections::BTreeMap;

use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::UtcInstant;
use amiss_wire::report::FindingKind;

use super::control::control_row;
use super::{Finding, candidate_digest_of, tree_value};

fn waiver_diagnostic(
    item: &amiss_wire::controls::WaiverItem,
    bundle_digest: Digest,
    current_fact_digest: Option<Digest>,
) -> Value {
    Value::object(vec![
        ("kind".to_owned(), Value::string("waiver".to_owned())),
        (
            "waiver_id".to_owned(),
            Value::string(item.waiver_id.as_str().to_owned()),
        ),
        (
            "waiver_bundle_digest".to_owned(),
            Value::string(bundle_digest.to_string()),
        ),
        (
            "candidate_tree".to_owned(),
            tree_value(&item.candidate_tree),
        ),
        (
            "finding_key".to_owned(),
            Value::string(item.finding_key.to_string()),
        ),
        (
            "authorized_fact_digest".to_owned(),
            Value::string(item.authorized_fact_digest.to_string()),
        ),
        (
            "current_fact_digest".to_owned(),
            current_fact_digest.map_or(Value::Null, |digest| Value::string(digest.to_string())),
        ),
        (
            "owner".to_owned(),
            Value::string(item.owner.as_str().to_owned()),
        ),
        (
            "issuer".to_owned(),
            Value::string(item.issuer.as_str().to_owned()),
        ),
        ("reason".to_owned(), Value::string(item.reason.clone())),
        (
            "created_at".to_owned(),
            Value::string(item.created_at.as_str().to_owned()),
        ),
        (
            "not_before".to_owned(),
            Value::string(item.not_before.as_str().to_owned()),
        ),
        (
            "expires_at".to_owned(),
            Value::string(item.expires_at.as_str().to_owned()),
        ),
        (
            "residual_disposition".to_owned(),
            Value::string("warn".to_owned()),
        ),
    ])
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
) -> BTreeMap<Digest, usize> {
    let mut waiver_valid: BTreeMap<Digest, usize> = BTreeMap::new();
    let Some(context) = &policy.waiver else {
        return waiver_valid;
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
                finding.kind().as_ref() != item.authorized_fact.finding_kind.as_ref()
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
                waiver_diagnostic(item, context.digest, current),
                profile,
            ));
        }
        if defects.is_empty() && target.is_some() {
            waiver_valid.insert(item.finding_key, index);
        }
    }
    waiver_valid
}
