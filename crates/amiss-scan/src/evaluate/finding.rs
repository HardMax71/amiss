use amiss_wire::controls::GitMode;
use amiss_wire::controls::ProjectionSource;
use amiss_wire::controls::{FactSchema, FindingKeyInputSchema, Profile};
use amiss_wire::envelope::document_digest;
use amiss_wire::model::Digest;
use amiss_wire::model::{RepoPath, RepoPathText};
use amiss_wire::report::model::ProjectionDifference;
use amiss_wire::report::model::RowsProjectionDifference;
use amiss_wire::report::model::{
    FindingFactEvidence, FindingFactInput, PolicySource, ReportFindingKeyInput,
};
use amiss_wire::report::{Disposition, FindingKind, FixKind};
use amiss_wire::resolution::{Missing, Resolution};

use crate::correlate::Observation;

use super::{
    Attribution, FACT_DOMAIN, FINDING_KEY_DOMAIN, Finding, FindingFact, FindingFix,
    FindingKeyScope, Location, LocationSide, PolicyStep,
};

pub(crate) fn fact(
    key: &ReportFindingKeyInput<RepoPath>,
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
    let digest = document_digest(FACT_DOMAIN, &input).ok_or(crate::Error::Internal)?;
    Ok(FindingFact { input, digest })
}

pub(super) fn reference_fact(
    key: &ReportFindingKeyInput<RepoPath>,
    observation: &Observation,
    multiplicity: u64,
) -> Result<FindingFact, crate::Error> {
    fact(
        key,
        FindingFactEvidence::Reference {
            occurrence_multiplicity: multiplicity,
            resolution: observation.resolution.clone(),
        },
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
        Resolution::Missing(Missing::PathNotFound {
            same_object_at: Some(moved),
            ..
        }) => relocation_fix(observation, moved),
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
        path: RepoPathText::try_from(observation.document.as_str()?.to_owned()).ok()?,
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
        path: RepoPathText::try_from(observation.document.as_str()?.to_owned()).ok()?,
        span,
        replacement,
        kind: FixKind::PathRespelling,
    })
}

/// A plain relative path that reached the missed file from beside its
/// document is rewritten, relative the same way, to the one path the change
/// moved identical bytes to. Any other spelling is left to its author.
fn relocation_fix(observation: &Observation, moved: &RepoPath) -> Option<FindingFix> {
    let span = observation.path_span?;
    let part = observation
        .raw_destination
        .split(['#', '?'])
        .next()
        .unwrap_or_default();
    let target = moved.as_str()?;
    let plain = |text: &str| {
        !text.is_empty()
            && !text.starts_with('/')
            && !text.contains(['%', ' ', '(', ')', '<', '>', '\\', ':'])
    };
    if !plain(part) || !plain(target) {
        return None;
    }
    let document = observation.document.as_bytes();
    let beside = crate::route::directory(document);
    let (reached, _) = crate::route::normalized_path_under(beside, false, part).ok()?;
    if Some(&reached) != observation.intent.repository_path.as_ref() {
        return None;
    }
    let from: Vec<&str> = std::str::from_utf8(beside)
        .ok()?
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let to: Vec<&str> = target.split('/').collect();
    let shared = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut written: Vec<&str> = vec![".."; from.len().saturating_sub(shared)];
    written.extend(to.get(shared..)?);
    let relative = written.join("/");
    let replacement = if part.starts_with("./") && !relative.starts_with("../") {
        format!("./{relative}")
    } else {
        relative
    };
    Some(FindingFix {
        path: RepoPathText::try_from(observation.document.as_str()?.to_owned()).ok()?,
        span,
        replacement,
        kind: FixKind::PathRelocation,
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
    let key_input = ReportFindingKeyInput {
        finding_kind: kind,
        schema: FindingKeyInputSchema::Current,
        scope,
    };
    let finding_key =
        document_digest(FINDING_KEY_DOMAIN, &key_input).ok_or(crate::Error::Internal)?;
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
