use std::collections::{BTreeMap, BTreeSet};

use amiss_wire::digest::Digest;
use amiss_wire::model::RepoPath;
use amiss_wire::report::{Disposition, FindingKind};
use amiss_wire::resolution::Resolution;

use crate::correlate::{Comparison, Observation};
use crate::evaluate::{Attribution, Finding, LocationSide};
use crate::scan::SpanDisplay;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Action {
    Fix,
    Check,
    Existing,
}

impl Action {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Fix => "fix",
            Self::Check => "check",
            Self::Existing => "existing",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Annotation {
    pub(crate) path: String,
    pub(crate) span: (usize, usize),
    pub(crate) display: SpanDisplay,
    tie: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) action: Action,
    pub(crate) target: Option<RepoPath>,
    pub(crate) finding_kinds: Vec<FindingKind>,
    pub(crate) location_count: u64,
    pub(crate) effective_disposition: Disposition,
    pub(crate) annotation: Option<Annotation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Feedback {
    pub(crate) items: Vec<Item>,
    pub(crate) existing_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Subject {
    RepositoryPath(RepoPath),
    Untargeted,
}

struct Candidate<'a> {
    observation: &'a Observation,
    check: bool,
}

struct Group {
    target: Option<RepoPath>,
    finding_kinds: BTreeSet<FindingKind>,
    location_count: u64,
    effective_disposition: Disposition,
    annotation: Option<Annotation>,
}

enum Decision {
    Item {
        action: Action,
        subject: Subject,
        target: Option<RepoPath>,
    },
    Ignore,
}

/// The PR-facing projection over the exact finding set. Findings remain the
/// audit unit; this groups only the small surface a reviewer acts on.
pub(crate) fn project(findings: &[Finding], comparisons: &[Comparison]) -> Feedback {
    let candidates = candidate_index(comparisons);
    let mut groups: BTreeMap<(Action, Subject), Group> = BTreeMap::new();

    for finding in findings {
        if finding.effective_disposition == Disposition::Record {
            continue;
        }
        match classify(finding, &candidates) {
            Decision::Item {
                action,
                subject,
                target,
            } => {
                let group = groups.entry((action, subject)).or_insert_with(|| Group {
                    target,
                    finding_kinds: BTreeSet::new(),
                    location_count: 0,
                    effective_disposition: finding.effective_disposition,
                    annotation: None,
                });
                group.finding_kinds.insert(finding.kind());
                group.location_count = group.location_count.saturating_add(finding.member_count);
                group.effective_disposition = group
                    .effective_disposition
                    .max(finding.effective_disposition);
                if action == Action::Fix
                    && let Some(candidate) = candidate_annotation(finding)
                    && group
                        .annotation
                        .as_ref()
                        .is_none_or(|current| annotation_precedes(&candidate, current))
                {
                    group.annotation = Some(candidate);
                }
            }
            Decision::Ignore => {}
        }
    }

    let existing = groups
        .keys()
        .filter(|(action, _subject)| *action == Action::Existing)
        .count();
    let items = groups
        .into_iter()
        .map(|((action, _subject), group)| Item {
            action,
            target: group.target,
            finding_kinds: group.finding_kinds.into_iter().collect(),
            location_count: group.location_count,
            effective_disposition: group.effective_disposition,
            annotation: group.annotation,
        })
        .collect();
    Feedback {
        items,
        existing_count: u64::try_from(existing).unwrap_or(u64::MAX),
    }
}

fn candidate_index(comparisons: &[Comparison]) -> BTreeMap<Digest, Candidate<'_>> {
    let mut candidates = BTreeMap::new();
    for comparison in comparisons {
        if let Some(candidate) = &comparison.candidate {
            candidates.insert(
                candidate.id,
                Candidate {
                    observation: candidate,
                    check: comparison.impact == crate::Impact::DependencyChangedSubjectUnchanged,
                },
            );
        }
        for candidate in &comparison.alternatives_candidate {
            candidates.insert(
                candidate.id,
                Candidate {
                    observation: candidate,
                    check: false,
                },
            );
        }
    }
    candidates
}

fn classify(finding: &Finding, candidates: &BTreeMap<Digest, Candidate<'_>>) -> Decision {
    let candidate = finding
        .observation_ids
        .iter()
        .find_map(|id| candidates.get(id));
    let invalid =
        candidate.is_some_and(|item| matches!(item.observation.resolution, Resolution::Invalid(_)));
    let (subject, target) = if invalid {
        (Subject::Untargeted, None)
    } else {
        subject(finding, candidate)
    };
    if candidate.is_some_and(|item| item.check) {
        return Decision::Item {
            action: Action::Check,
            subject,
            target,
        };
    }
    attributed(finding.attribution, finding.location.side, subject, target)
}

fn attributed(
    attribution: Attribution,
    side: LocationSide,
    subject: Subject,
    target: Option<RepoPath>,
) -> Decision {
    match attribution {
        Attribution::Introduced => Decision::Item {
            action: Action::Fix,
            subject,
            target,
        },
        Attribution::PreExisting => Decision::Item {
            action: Action::Existing,
            subject,
            target,
        },
        Attribution::Unknown => Decision::Item {
            action: Action::Check,
            subject,
            target,
        },
        Attribution::Resolved => Decision::Ignore,
        Attribution::NotApplicable => match side {
            LocationSide::Candidate | LocationSide::Control | LocationSide::Global => {
                Decision::Item {
                    action: Action::Fix,
                    subject,
                    target,
                }
            }
            LocationSide::Base => Decision::Ignore,
        },
    }
}

fn subject(finding: &Finding, candidate: Option<&Candidate<'_>>) -> (Subject, Option<RepoPath>) {
    let target = match finding.location.side {
        LocationSide::Control => finding.location.path.clone(),
        LocationSide::Global => None,
        LocationSide::Base | LocationSide::Candidate => {
            candidate.and_then(|item| item.observation.intent.repository_path.clone())
        }
    };
    target.map_or((Subject::Untargeted, None), |path| {
        (Subject::RepositoryPath(path.clone()), Some(path))
    })
}

fn candidate_annotation(finding: &Finding) -> Option<Annotation> {
    if finding.location.side != LocationSide::Candidate {
        return None;
    }
    Some(Annotation {
        path: finding.location.path.as_ref()?.as_str()?.to_owned(),
        span: finding.location.span?,
        display: finding.location.display?,
        tie: finding.key().digest(),
    })
}

fn annotation_precedes(left: &Annotation, right: &Annotation) -> bool {
    (&left.path, left.span, left.tie) < (&right.path, right.span, right.tie)
}
