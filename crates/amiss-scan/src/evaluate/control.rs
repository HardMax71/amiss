use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::RepoPath;
use amiss_wire::report::FindingKind;
use amiss_wire::report::model::ControlFindingKeyScopeKind;

use crate::scan::SpanDisplay;

use super::claims::sources_value;
use super::finding::{candidate_fact_finding, nullable_path};
use super::{Finding, FindingKeyScope, Location, LocationSide};

/// One candidate document's reserved governed definitions: the exact node
/// count and the distinct source digests with their multiplicities, plus the
/// least location as the representative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedSeed {
    pub document: RepoPath,
    pub member_count: u64,
    pub sources: Vec<(Digest, u64)>,
    pub representative_span: Option<(usize, usize)>,
    pub representative_display: Option<SpanDisplay>,
}

/// One control-scoped, candidate-only finding: the shared shell of the
/// governed boundary and the claim kinds, differing only in evidence.
pub(super) fn control_fact_finding(
    kind: FindingKind,
    document: &RepoPath,
    rule_id: &str,
    evidence: Value,
    member_count: u64,
    representative: (Option<(usize, usize)>, Option<SpanDisplay>),
    profile: Profile,
) -> Finding {
    candidate_fact_finding(
        kind,
        FindingKeyScope::Control {
            control_path: Some(document.clone()),
            kind: ControlFindingKeyScopeKind::Control,
            rule_id: rule_id.to_owned(),
        },
        evidence,
        member_count,
        Location {
            side: LocationSide::Candidate,
            path: Some(document.clone()),
            span: representative.0,
            display: representative.1,
        },
        profile,
    )
}

/// The reserved governed declaration boundary: control-scoped at the affected
/// document under the one closed rule, with null base state, candidate
/// `unsupported`, exact node multiplicity, and the sorted distinct source
/// digests.
pub(super) fn governed_finding(seed: &GovernedSeed, profile: Profile) -> Finding {
    let rule_id = "unsupported/governed-claim";
    let evidence = Value::object(vec![
        ("kind".to_owned(), Value::string("control".to_owned())),
        ("control_path".to_owned(), seed.document.to_value()),
        ("rule_id".to_owned(), Value::string(rule_id.to_owned())),
        ("base_control_state".to_owned(), Value::Null),
        ("base_control_digest".to_owned(), Value::Null),
        (
            "candidate_control_state".to_owned(),
            Value::object(vec![
                (
                    "schema".to_owned(),
                    Value::string("amiss/scanner-control-state".to_owned()),
                ),
                ("rule_id".to_owned(), Value::string(rule_id.to_owned())),
                (
                    "path".to_owned(),
                    seed.document
                        .as_str()
                        .map_or(Value::Null, |path| Value::string(path.to_owned())),
                ),
                ("sources".to_owned(), sources_value(&seed.sources)),
                ("state".to_owned(), Value::string("unsupported".to_owned())),
            ]),
        ),
        ("candidate_control_digest".to_owned(), Value::Null),
        ("exception".to_owned(), Value::Null),
    ]);
    control_fact_finding(
        FindingKind::UnsupportedCapability,
        &seed.document,
        rule_id,
        evidence,
        seed.member_count,
        (seed.representative_span, seed.representative_display),
        profile,
    )
}

pub(super) fn control_finding(
    seed: &crate::policy::ControlSeed,
    policy: &crate::policy::Effects,
    profile: Profile,
) -> Finding {
    control_row(
        seed.kind,
        seed.rule_id.clone(),
        seed.control_path.clone(),
        (policy.base_digest, policy.candidate_digest),
        Value::Null,
        profile,
    )
}

/// One control-scoped finding under an exact rule: the fact embeds the
/// governing control's digests and, for exception defects, the complete
/// typed diagnostic.
pub(super) fn control_row(
    kind: FindingKind,
    rule_id: String,
    control_path: Option<RepoPath>,
    control_digests: (Option<Digest>, Option<Digest>),
    exception: Value,
    profile: Profile,
) -> Finding {
    let nullable_digest = |value: Option<Digest>| {
        value.map_or(Value::Null, |digest| Value::string(digest.to_string()))
    };
    let scope = FindingKeyScope::Control {
        control_path: control_path.clone(),
        kind: ControlFindingKeyScopeKind::Control,
        rule_id: rule_id.clone(),
    };
    let evidence = Value::object(vec![
        ("kind".to_owned(), Value::string("control".to_owned())),
        (
            "control_path".to_owned(),
            nullable_path(control_path.as_ref()),
        ),
        ("rule_id".to_owned(), Value::string(rule_id)),
        ("base_control_state".to_owned(), Value::Null),
        (
            "base_control_digest".to_owned(),
            nullable_digest(control_digests.0),
        ),
        ("candidate_control_state".to_owned(), Value::Null),
        (
            "candidate_control_digest".to_owned(),
            nullable_digest(control_digests.1),
        ),
        ("exception".to_owned(), exception),
    ]);
    let side = if control_path.is_some() {
        LocationSide::Control
    } else {
        LocationSide::Global
    };
    candidate_fact_finding(
        kind,
        scope,
        evidence,
        1,
        Location {
            side,
            path: control_path,
            span: None,
            display: None,
        },
        profile,
    )
}
