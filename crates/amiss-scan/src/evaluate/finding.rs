use amiss_wire::controls::GitMode;
use amiss_wire::controls::ProjectionSource;
use amiss_wire::controls::{FactSchema, FindingKeyInputSchema, Profile};
use amiss_wire::digest::{Digest, hj_serde};
use amiss_wire::model::{RepoPath, RepoPathText};
use amiss_wire::report::model::ProjectionDifference;
use amiss_wire::report::model::RowsProjectionDifference;
use amiss_wire::report::model::{
    FindingFactEvidence, FindingFactInput, FindingKeyInput, PolicySource, ReferenceFactEvidence,
    ReferenceFactEvidenceKind,
};
use amiss_wire::report::{Disposition, FindingKind, FixKind};
use amiss_wire::resolution::{Missing, Resolution};

use crate::correlate::Observation;

use super::{
    Attribution, FACT_DOMAIN, FINDING_KEY_DOMAIN, Finding, FindingFact, FindingFix,
    FindingKeyScope, Location, LocationSide, PolicyStep,
};

pub(crate) fn fact(
    key: &FindingKeyInput<RepoPath>,
    evidence: FindingFactEvidence<
        RepoPath,
        Resolution<RepoPath>,
        ProjectionSource,
        ProjectionDifference<Box<RowsProjectionDifference>>,
        GitMode,
    >,
) -> Result<FindingFact, crate::Error> {
    let input = FindingFactInput {
        evidence,
        finding_kind: key.finding_kind,
        key_input: key.clone(),
        schema: FactSchema::Current,
    };
    let digest = hj_serde(FACT_DOMAIN, |mut writer| {
        serde_json_canonicalizer::to_writer(&input, &mut writer)
    })
    .map_err(|_defect| crate::Error::Internal)?;
    Ok(FindingFact { input, digest })
}

pub(super) fn reference_fact(
    key: &FindingKeyInput<RepoPath>,
    observation: &Observation,
    multiplicity: u64,
) -> Result<FindingFact, crate::Error> {
    fact(
        key,
        FindingFactEvidence::Reference(ReferenceFactEvidence {
            kind: ReferenceFactEvidenceKind::Reference,
            occurrence_multiplicity: multiplicity,
            resolution: observation.resolution.clone(),
        }),
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
    evidence: FindingFactEvidence<
        RepoPath,
        Resolution<RepoPath>,
        ProjectionSource,
        ProjectionDifference<Box<RowsProjectionDifference>>,
        GitMode,
    >,
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
    finding.candidate_fact = Some(fact(&finding.key_input, evidence)?);
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
