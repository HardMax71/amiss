use amiss_wire::controls::{FindingKeyInputSchema, Profile};
use amiss_wire::digest::{Digest, hj_serde};
use amiss_wire::json::Value;
use amiss_wire::model::{RepoPath, RepoPathText};
use amiss_wire::report::model::{FindingKeyInput, PolicySource, RepositoryIntentPath};
use amiss_wire::report::{Disposition, FindingKind, FixKind};
use amiss_wire::resolution::{Missing, Resolution};

use crate::correlate::Observation;

use super::{
    Attribution, FINDING_KEY_DOMAIN, FINDING_KEY_SCHEMA, Finding, FindingFact, FindingFix,
    FindingKeyScope, Location, LocationSide, PolicyStep, resolution_value,
};

pub(super) fn nullable_path(path: Option<&RepoPath>) -> Value {
    path.map_or(Value::Null, RepoPath::to_value)
}

pub(crate) fn key_value(input: &FindingKeyInput<RepoPath>) -> Value {
    Value::object(vec![
        (
            "schema".to_owned(),
            Value::string(FINDING_KEY_SCHEMA.to_owned()),
        ),
        (
            "finding_kind".to_owned(),
            Value::string(input.finding_kind.as_ref().to_owned()),
        ),
        ("scope".to_owned(), scope_value(&input.scope)),
    ])
}

fn scope_value(scope: &FindingKeyScope) -> Value {
    match scope {
        FindingKeyScope::Document { document, .. } => Value::object(vec![
            ("kind".to_owned(), Value::string("document".to_owned())),
            ("document".to_owned(), document.to_value()),
        ]),
        FindingKeyScope::Observation { observation_id, .. } => Value::object(vec![
            ("kind".to_owned(), Value::string("observation".to_owned())),
            (
                "observation_id".to_owned(),
                Value::string(observation_id.to_string()),
            ),
        ]),
        FindingKeyScope::Reference {
            document,
            source_construct,
            normalized_target_intent: intent,
            occurrence,
            ..
        } => Value::object(vec![
            ("kind".to_owned(), Value::string("reference".to_owned())),
            ("document".to_owned(), document.to_value()),
            (
                "source_construct".to_owned(),
                Value::string(source_construct.as_ref().to_owned()),
            ),
            (
                "normalized_target_intent".to_owned(),
                Value::object(
                    intent
                        .commit_oid
                        .iter()
                        .map(|oid| {
                            (
                                "commit_oid".to_owned(),
                                Value::string(oid.as_str().to_owned()),
                            )
                        })
                        .chain([
                            (
                                "kind".to_owned(),
                                Value::string("repository-path".to_owned()),
                            ),
                            (
                                "path".to_owned(),
                                match &intent.path {
                                    RepositoryIntentPath::Empty(_) => Value::string(String::new()),
                                    RepositoryIntentPath::Path(path) => path.to_value(),
                                },
                            ),
                            (
                                "target_kind".to_owned(),
                                Value::string(
                                    Into::<&'static str>::into(intent.target_kind).to_owned(),
                                ),
                            ),
                            (
                                "query_digest".to_owned(),
                                intent.query_digest.map_or(Value::Null, |digest| {
                                    Value::string(digest.to_string())
                                }),
                            ),
                            (
                                "fragment_digest".to_owned(),
                                intent.fragment_digest.map_or(Value::Null, |digest| {
                                    Value::string(digest.to_string())
                                }),
                            ),
                        ])
                        .collect(),
                ),
            ),
            (
                "occurrence".to_owned(),
                Value::object(vec![
                    (
                        "kind".to_owned(),
                        Value::string("source-projection".to_owned()),
                    ),
                    (
                        "source_projection_digest".to_owned(),
                        Value::string(occurrence.source_projection_digest.to_string()),
                    ),
                ]),
            ),
        ]),
        FindingKeyScope::Control {
            control_path,
            rule_id,
            ..
        } => Value::object(vec![
            ("kind".to_owned(), Value::string("control".to_owned())),
            (
                "control_path".to_owned(),
                nullable_path(control_path.as_ref()),
            ),
            ("rule_id".to_owned(), Value::string(rule_id.clone())),
        ]),
    }
}

pub(super) fn reference_fact(
    key: &FindingKeyInput<RepoPath>,
    observation: &Observation,
    multiplicity: u64,
) -> FindingFact {
    FindingFact::new(
        key,
        Value::object(vec![
            ("kind".to_owned(), Value::string("reference".to_owned())),
            ("resolution".to_owned(), resolution_value(observation)),
            (
                "occurrence_multiplicity".to_owned(),
                Value::Integer(i64::try_from(multiplicity).unwrap_or(i64::MAX)),
            ),
        ]),
    )
}

/// Only a missing resolution reaches a structural finding, so the match is
/// the kind gate.
pub(super) fn missing_fix(candidates: &[&Observation]) -> Option<FindingFix> {
    let [observation] = candidates else {
        return None;
    };
    match &observation.resolution {
        Resolution::Missing(Missing::HeadingAnchorNotFound {
            near: Some(near), ..
        }) => anchor_fix(observation, near),
        Resolution::Missing(Missing::PathNotFound {
            near: Some(near), ..
        }) => path_fix(observation, near),
        Resolution::Missing(_)
        | Resolution::Resolved { .. }
        | Resolution::DeclaredUntracked(_)
        | Resolution::TypeMismatch { .. }
        | Resolution::UnsupportedTarget(_)
        | Resolution::UnsupportedSemantics(_)
        | Resolution::UnsupportedVersion { .. }
        | Resolution::Invalid { .. }
        | Resolution::External { .. } => None,
    }
}

fn anchor_fix(observation: &Observation, near: &str) -> Option<FindingFix> {
    Some(FindingFix {
        path: RepoPathText::new(observation.document.as_str()?.to_owned())?,
        span: observation.fragment_span?,
        replacement: near.to_owned(),
        kind: FixKind::AnchorRespelling,
    })
}

/// The intent is the resolver's join, so only its tail is the author's
/// spelling to respell.
fn path_fix(observation: &Observation, near: &RepoPath) -> Option<FindingFix> {
    let span = observation.path_span?;
    let part = observation
        .raw_destination
        .split_once('#')
        .map_or(observation.raw_destination.as_str(), |(prefix, _)| prefix);
    let missed_bytes = observation.intent.repository_path.as_ref()?.as_bytes();
    let tail_at = missed_bytes.len().checked_sub(part.len())?;
    if missed_bytes.get(tail_at..)? != part.as_bytes() {
        return None;
    }
    if tail_at != 0 && missed_bytes.get(tail_at.checked_sub(1)?)? != &b'/' {
        return None;
    }
    if near.as_bytes().get(..tail_at)? != missed_bytes.get(..tail_at)? {
        return None;
    }
    let replacement = near.as_str()?.get(tail_at..)?.to_owned();
    Some(FindingFix {
        path: RepoPathText::new(observation.document.as_str()?.to_owned())?,
        span,
        replacement,
        kind: FixKind::PathRespelling,
    })
}

pub(super) fn observation_location(observation: &Observation, side: LocationSide) -> Location {
    Location {
        side,
        path: Some(observation.document.clone()),
        span: Some(observation.span),
        display: Some(observation.display),
    }
}

pub(super) fn candidate_fact_finding(
    kind: FindingKind,
    scope: FindingKeyScope,
    evidence: Value,
    member_count: u64,
    location: Location,
    profile: Profile,
) -> Result<Finding, crate::Error> {
    let mut finding = simple(
        kind,
        scope,
        Attribution::NotApplicable,
        Vec::new(),
        location,
        profile,
    )?;
    finding.candidate_fact = Some(FindingFact::new(&finding.key_input, evidence));
    finding.member_count = member_count;
    Ok(finding)
}

pub(super) fn simple(
    kind: FindingKind,
    scope: FindingKeyScope,
    attribution: Attribution,
    ids: Vec<Digest>,
    location: Location,
    profile: Profile,
) -> Result<Finding, crate::Error> {
    let key_input = FindingKeyInput {
        finding_kind: kind,
        schema: FindingKeyInputSchema::Current,
        scope,
    };
    let finding_key = hj_serde(FINDING_KEY_DOMAIN, |writer| {
        serde_json::to_writer(writer, &key_input)
    })
    .map_err(|_defect| crate::Error::Internal)?;
    let configured = kind.built_in_disposition(profile);
    Ok(Finding {
        key_input,
        finding_key,
        attribution,
        base_fact: None,
        candidate_fact: None,
        member_count: 1,
        observation_ids: ids,
        location,
        configured_disposition: configured,
        effective_disposition: configured,
        debt: None,
        waiver: None,
        fix: None,
        steps: vec![built_in_step(kind, profile)],
    })
}

/// Step one: built-in always starts from `record` and applies the defaults
/// table for the selected profile.
pub(super) fn built_in_step(kind: FindingKind, profile: Profile) -> PolicyStep {
    PolicyStep {
        source: PolicySource::BuiltIn,
        rule_id: format!(
            "scanner-policy-defaults/{}/{}",
            kind.as_ref(),
            Into::<&'static str>::into(profile.policy_defaults())
        ),
        before: Disposition::Record,
        after: kind.built_in_disposition(profile),
    }
}
