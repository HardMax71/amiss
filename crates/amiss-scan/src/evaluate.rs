use std::collections::BTreeMap;

use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::Value;
use amiss_wire::report::{Disposition, FindingKind, ResolutionCode, ResolutionStatus};

use crate::correlate::{Comparison, Impact, Observation, Outcome};
use crate::observe;

pub const FINDING_KEY_SCHEMA: &str = "amiss/scanner-finding-key-input/v1";
pub const FINDING_KEY_DOMAIN: &str = "amiss/scanner-finding-key/v1";
pub const FACT_SCHEMA: &str = "amiss/scanner-fact/v1";
pub const FACT_DOMAIN: &str = "amiss/scanner-fact/v1";

/// One document path's paired sides, reduced to what finding construction
/// reads. A failed side never reaches this projection: analysis errors are
/// not findings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentInput {
    pub path: String,
    pub base: Option<DocumentSide>,
    pub candidate: Option<DocumentSide>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentSide {
    Scanned {
        mdx_regions: u64,
        html_regions: u64,
        extracted_references: u64,
    },
    Unsupported,
    ExcludedBuiltIn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attribution {
    Introduced,
    PreExisting,
    Resolved,
    Unknown,
    NotApplicable,
}

impl Attribution {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Introduced => "introduced",
            Self::PreExisting => "pre-existing",
            Self::Resolved => "resolved",
            Self::Unknown => "unknown",
            Self::NotApplicable => "not-applicable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationSide {
    Base,
    Candidate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub side: LocationSide,
    pub path: Option<String>,
    pub span: Option<(usize, usize)>,
}

/// One constructed finding: its key, its facts where the reference scope
/// defines them, its aggregation, and its built-in dispositions. Policy
/// steps beyond the built-in table live with the control layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub kind: FindingKind,
    pub key_input: Value,
    pub finding_key: Digest,
    pub attribution: Attribution,
    pub base_fact: Option<(Value, Digest)>,
    pub candidate_fact: Option<(Value, Digest)>,
    pub member_count: u64,
    pub observation_ids: Vec<Digest>,
    pub location: Location,
    pub configured_disposition: Disposition,
    pub effective_disposition: Disposition,
}

fn key_digest(input: &Value) -> Digest {
    hj(FINDING_KEY_DOMAIN, input)
}

fn key_input(kind: FindingKind, scope: Value) -> (Value, Digest) {
    let input = Value::Object(vec![
        (
            "schema".to_owned(),
            Value::String(FINDING_KEY_SCHEMA.to_owned()),
        ),
        (
            "finding_kind".to_owned(),
            Value::String(kind.as_str().to_owned()),
        ),
        ("scope".to_owned(), scope),
    ]);
    let digest = key_digest(&input);
    (input, digest)
}

fn document_scope(path: &str) -> Value {
    Value::Object(vec![
        ("kind".to_owned(), Value::String("document".to_owned())),
        ("document".to_owned(), Value::String(path.to_owned())),
    ])
}

fn observation_scope(id: Digest) -> Value {
    Value::Object(vec![
        ("kind".to_owned(), Value::String("observation".to_owned())),
        ("observation_id".to_owned(), Value::String(id.to_string())),
    ])
}

/// The structural reference scope: document, construct, the repository
/// projection of the intent, and the containing source projection. Line and
/// column are excluded, so moving a construct keeps its key, while changing
/// the broken target resolves the old key and introduces a new one.
fn reference_scope(observation: &Observation) -> Value {
    let intent = &observation.intent;
    Value::Object(vec![
        ("kind".to_owned(), Value::String("reference".to_owned())),
        (
            "document".to_owned(),
            Value::String(observation.document.clone()),
        ),
        (
            "source_construct".to_owned(),
            Value::String(observation.construct.as_str().to_owned()),
        ),
        (
            "normalized_target_intent".to_owned(),
            Value::Object(vec![
                (
                    "kind".to_owned(),
                    Value::String("repository-path".to_owned()),
                ),
                (
                    "path".to_owned(),
                    Value::String(intent.repository_path.clone().unwrap_or_default()),
                ),
                (
                    "target_kind".to_owned(),
                    Value::String(
                        intent
                            .target_kind
                            .map_or("either", amiss_wire::controls::TargetKind::as_str)
                            .to_owned(),
                    ),
                ),
                (
                    "query_digest".to_owned(),
                    observe::query_digest(intent)
                        .map_or(Value::Null, |digest| Value::String(digest.to_string())),
                ),
                (
                    "fragment_digest".to_owned(),
                    observe::fragment_digest(intent)
                        .map_or(Value::Null, |digest| Value::String(digest.to_string())),
                ),
            ]),
        ),
        (
            "occurrence".to_owned(),
            Value::Object(vec![
                (
                    "kind".to_owned(),
                    Value::String("source-projection".to_owned()),
                ),
                (
                    "source_projection_digest".to_owned(),
                    Value::String(observation.projection_digest.to_string()),
                ),
            ]),
        ),
    ])
}

fn resolution_value(observation: &Observation) -> Value {
    let resolution = &observation.resolution;
    let nullable = |text: Option<String>| text.map_or(Value::Null, Value::String);
    Value::Object(vec![
        (
            "status".to_owned(),
            Value::String(resolution.code.status().as_str().to_owned()),
        ),
        (
            "code".to_owned(),
            Value::String(resolution.code.as_str().to_owned()),
        ),
        ("path".to_owned(), nullable(resolution.path.clone())),
        (
            "entry_kind".to_owned(),
            nullable(resolution.entry_kind.map(|kind| kind.as_str().to_owned())),
        ),
        (
            "git_mode".to_owned(),
            nullable(resolution.git_mode.map(|mode| mode.as_str().to_owned())),
        ),
        (
            "raw_digest".to_owned(),
            nullable(resolution.raw_digest.map(|digest| digest.to_string())),
        ),
        (
            "projection_digest".to_owned(),
            nullable(
                resolution
                    .projection_digest
                    .map(|digest| digest.to_string()),
            ),
        ),
        (
            "content_availability".to_owned(),
            Value::String(resolution.content_availability.as_str().to_owned()),
        ),
    ])
}

fn reference_fact(
    kind: FindingKind,
    key: &Value,
    observation: &Observation,
    multiplicity: u64,
) -> (Value, Digest) {
    let fact = Value::Object(vec![
        ("schema".to_owned(), Value::String(FACT_SCHEMA.to_owned())),
        (
            "finding_kind".to_owned(),
            Value::String(kind.as_str().to_owned()),
        ),
        ("key_input".to_owned(), key.clone()),
        (
            "evidence".to_owned(),
            Value::Object(vec![
                ("kind".to_owned(), Value::String("reference".to_owned())),
                ("resolution".to_owned(), resolution_value(observation)),
                (
                    "occurrence_multiplicity".to_owned(),
                    Value::Integer(i64::try_from(multiplicity).unwrap_or(i64::MAX)),
                ),
            ]),
        ),
    ]);
    let digest = hj(FACT_DOMAIN, &fact);
    (fact, digest)
}

const fn structural_kind(status: ResolutionStatus) -> Option<FindingKind> {
    match status {
        ResolutionStatus::Missing => Some(FindingKind::ExplicitTargetMissing),
        ResolutionStatus::TypeMismatch => Some(FindingKind::ExplicitTargetTypeMismatch),
        ResolutionStatus::Resolved
        | ResolutionStatus::Unsupported
        | ResolutionStatus::Invalid
        | ResolutionStatus::ExternalOutOfScope => None,
    }
}

/// The occurrence-boundary mapping of step two: which non-structural kind one
/// candidate resolution emits, if any.
const fn boundary_kind(code: ResolutionCode) -> Option<FindingKind> {
    match code {
        ResolutionCode::InvalidUri
        | ResolutionCode::InvalidPercentEncoding
        | ResolutionCode::DecodedPathControl
        | ResolutionCode::PathTraversal
        | ResolutionCode::BackslashSeparator
        | ResolutionCode::EncodedSlash
        | ResolutionCode::InvalidFragmentEncoding
        | ResolutionCode::InvalidReference => Some(FindingKind::InvalidReference),
        ResolutionCode::UnsupportedQuerySemantics
        | ResolutionCode::UnsupportedFragmentSemantics
        | ResolutionCode::CodeFragmentUnevaluated
        | ResolutionCode::SiteRouteUnsupported
        | ResolutionCode::NetworkPathUnsupported => {
            Some(FindingKind::UnsupportedReferenceSemantics)
        }
        ResolutionCode::UnsupportedVersionScope => Some(FindingKind::UnsupportedVersionScope),
        ResolutionCode::SymlinkEntry | ResolutionCode::GitlinkEntry => {
            Some(FindingKind::UnsupportedTargetKind)
        }
        ResolutionCode::ExternalUrl | ResolutionCode::ForeignRepository => {
            Some(FindingKind::ExternalOutOfScope)
        }
        ResolutionCode::ExactPath
        | ResolutionCode::PathNotFound
        | ResolutionCode::TargetTypeMismatch => None,
    }
}

fn observation_location(observation: &Observation, side: LocationSide) -> Location {
    Location {
        side,
        path: Some(observation.document.clone()),
        span: Some(observation.span),
    }
}

fn simple(
    kind: FindingKind,
    scope: Value,
    attribution: Attribution,
    candidate_fact: Option<(Value, Digest)>,
    ids: Vec<Digest>,
    location: Location,
    enforce: bool,
) -> Finding {
    let (key_value, digest) = key_input(kind, scope);
    let configured = kind.built_in_disposition(enforce);
    Finding {
        kind,
        key_input: key_value,
        finding_key: digest,
        attribution,
        base_fact: None,
        candidate_fact,
        member_count: 1,
        observation_ids: ids,
        location,
        configured_disposition: configured,
        effective_disposition: configured,
    }
}

/// Every candidate occurrence: primaries plus alternatives on the candidate
/// side of every comparison.
fn candidate_occurrences(comparisons: &[Comparison]) -> Vec<&Observation> {
    let mut out: Vec<&Observation> = Vec::new();
    for comparison in comparisons {
        out.extend(comparison.candidate.iter());
        out.extend(comparison.alternatives_candidate.iter());
    }
    out
}

fn base_occurrences(comparisons: &[Comparison]) -> Vec<&Observation> {
    let mut out: Vec<&Observation> = Vec::new();
    for comparison in comparisons {
        out.extend(comparison.base.iter());
        out.extend(comparison.alternatives_base.iter());
    }
    out
}

/// The exact ordinary-finding projection: document findings, occurrence
/// boundaries, structural aggregation by key with attribution, and the
/// comparison-derived removal, ambiguity, and impact findings. Analysis
/// errors never enter, and the result is in canonical finding-key order.
#[must_use]
pub fn evaluate(
    documents: &[DocumentInput],
    comparisons: &[Comparison],
    enforce: bool,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    for document in documents {
        document_findings(document, enforce, &mut findings);
    }

    for observation in candidate_occurrences(comparisons) {
        let mut emit = |kind: FindingKind| {
            findings.push(simple(
                kind,
                observation_scope(observation.id),
                Attribution::NotApplicable,
                None,
                vec![observation.id],
                observation_location(observation, LocationSide::Candidate),
                enforce,
            ));
        };
        if let Some(kind) = boundary_kind(observation.resolution.code) {
            emit(kind);
        }
        let pointer_only = observation.resolution.content_availability
            == amiss_wire::controls::ContentAvailability::LfsPointerOnly;
        if pointer_only {
            emit(FindingKind::UnsupportedTargetKind);
        }
    }

    structural_findings(comparisons, enforce, &mut findings);

    for comparison in comparisons {
        comparison_findings(comparison, enforce, &mut findings);
    }

    findings.sort_by_key(|finding| finding.finding_key);
    findings
}

fn document_findings(document: &DocumentInput, enforce: bool, findings: &mut Vec<Finding>) {
    let path = document.path.as_str();
    if document.base.is_some() && document.candidate.is_none() {
        findings.push(simple(
            FindingKind::DocumentRemoved,
            document_scope(path),
            Attribution::NotApplicable,
            None,
            Vec::new(),
            Location {
                side: LocationSide::Base,
                path: Some(path.to_owned()),
                span: None,
            },
            enforce,
        ));
        return;
    }
    let candidate_location = || Location {
        side: LocationSide::Candidate,
        path: Some(path.to_owned()),
        span: None,
    };
    match document.candidate {
        None | Some(DocumentSide::ExcludedBuiltIn) => {}
        Some(DocumentSide::Unsupported) => {
            findings.push(simple(
                FindingKind::UnsupportedDocumentFormat,
                document_scope(path),
                Attribution::NotApplicable,
                None,
                Vec::new(),
                candidate_location(),
                enforce,
            ));
        }
        Some(DocumentSide::Scanned {
            mdx_regions,
            html_regions,
            extracted_references,
        }) => {
            if mdx_regions > 0 {
                findings.push(simple(
                    FindingKind::OpaqueMdxRegion,
                    document_scope(path),
                    Attribution::NotApplicable,
                    None,
                    Vec::new(),
                    candidate_location(),
                    enforce,
                ));
            }
            if html_regions > 0 {
                findings.push(simple(
                    FindingKind::OpaqueHtmlRegion,
                    document_scope(path),
                    Attribution::NotApplicable,
                    None,
                    Vec::new(),
                    candidate_location(),
                    enforce,
                ));
            }
            if extracted_references == 0 {
                findings.push(simple(
                    FindingKind::UnlinkedDocument,
                    document_scope(path),
                    Attribution::NotApplicable,
                    None,
                    Vec::new(),
                    candidate_location(),
                    enforce,
                ));
            }
        }
    }
}

struct KeyGroup<'a> {
    kind: FindingKind,
    scope: Value,
    base: Vec<&'a Observation>,
    candidate: Vec<&'a Observation>,
}

fn collect_structural<'a>(
    groups: &mut BTreeMap<Digest, KeyGroup<'a>>,
    observation: &'a Observation,
    is_base: bool,
) {
    let Some(kind) = structural_kind(observation.resolution.code.status()) else {
        return;
    };
    let scope = reference_scope(observation);
    let (_input, digest) = key_input(kind, scope.clone());
    let group = groups.entry(digest).or_insert_with(|| KeyGroup {
        kind,
        scope,
        base: Vec::new(),
        candidate: Vec::new(),
    });
    if is_base {
        group.base.push(observation);
    } else {
        group.candidate.push(observation);
    }
}

/// Step three: structural kinds aggregate independently by key across both
/// sides, one finding per key with at least one included side. Attribution
/// follows fact presence and equality, and a base-only projection is forced
/// to record so a deletion cannot retain an old blocking failure.
fn structural_findings(comparisons: &[Comparison], enforce: bool, findings: &mut Vec<Finding>) {
    let mut groups: BTreeMap<Digest, KeyGroup<'_>> = BTreeMap::new();
    for observation in candidate_occurrences(comparisons) {
        collect_structural(&mut groups, observation, false);
    }
    for observation in base_occurrences(comparisons) {
        collect_structural(&mut groups, observation, true);
    }

    for (digest, group) in groups {
        let (key_value, _same) = key_input(group.kind, group.scope.clone());
        let base_fact = group.base.first().map(|observation| {
            reference_fact(
                group.kind,
                &key_value,
                observation,
                u64::try_from(group.base.len()).unwrap_or(u64::MAX),
            )
        });
        let candidate_fact = group.candidate.first().map(|observation| {
            reference_fact(
                group.kind,
                &key_value,
                observation,
                u64::try_from(group.candidate.len()).unwrap_or(u64::MAX),
            )
        });
        let attribution = match (&base_fact, &candidate_fact) {
            (None, Some(_)) => Attribution::Introduced,
            (Some(_), None) => Attribution::Resolved,
            (Some((left, _)), Some((right, _))) if left == right => Attribution::PreExisting,
            (Some(_), Some(_)) => Attribution::Unknown,
            (None, None) => Attribution::NotApplicable,
        };
        if attribution == Attribution::NotApplicable {
            continue;
        }

        let members = if group.candidate.is_empty() {
            &group.base
        } else {
            &group.candidate
        };
        let mut ids: Vec<Digest> = members.iter().map(|observation| observation.id).collect();
        ids.sort_unstable();
        let representative = members
            .iter()
            .min_by_key(|observation| {
                (
                    observation.document.clone(),
                    observation.span,
                    observation.id,
                )
            })
            .copied();
        let side = if group.candidate.is_empty() {
            LocationSide::Base
        } else {
            LocationSide::Candidate
        };
        let location = representative.map_or(
            Location {
                side,
                path: None,
                span: None,
            },
            |observation| observation_location(observation, side),
        );

        let configured = if attribution == Attribution::Resolved {
            Disposition::Record
        } else {
            group.kind.built_in_disposition(enforce)
        };
        findings.push(Finding {
            kind: group.kind,
            key_input: key_value,
            finding_key: digest,
            attribution,
            base_fact,
            candidate_fact,
            member_count: u64::try_from(members.len()).unwrap_or(u64::MAX),
            observation_ids: ids,
            location,
            configured_disposition: configured,
            effective_disposition: configured,
        });
    }
}

/// Step four: one removal per base-only comparison, one ambiguity per
/// ambiguous comparison, and the three named impact findings only.
fn comparison_findings(comparison: &Comparison, enforce: bool, findings: &mut Vec<Finding>) {
    if comparison.outcome == Outcome::None
        && comparison.base.is_some()
        && comparison.candidate.is_none()
    {
        if let Some(base) = &comparison.base {
            findings.push(simple(
                FindingKind::ExplicitReferenceRemoved,
                observation_scope(base.id),
                Attribution::NotApplicable,
                None,
                vec![base.id],
                observation_location(base, LocationSide::Base),
                enforce,
            ));
        }
        return;
    }
    let primary = comparison
        .candidate
        .as_ref()
        .map(|observation| (observation, LocationSide::Candidate))
        .or_else(|| {
            comparison
                .base
                .as_ref()
                .map(|observation| (observation, LocationSide::Base))
        });
    let Some((primary, side)) = primary else {
        return;
    };
    if comparison.outcome == Outcome::Ambiguous {
        findings.push(simple(
            FindingKind::ObservationCorrelationAmbiguous,
            observation_scope(primary.id),
            Attribution::NotApplicable,
            None,
            vec![primary.id],
            observation_location(primary, side),
            enforce,
        ));
        return;
    }
    let impact_kind = match comparison.impact {
        Impact::DependencyChangedSubjectUnchanged => {
            Some(FindingKind::DependencyChangedSubjectUnchanged)
        }
        Impact::DependencyAndSubjectCochanged => Some(FindingKind::DependencyAndSubjectCochanged),
        Impact::SubjectChanged => Some(FindingKind::SubjectChanged),
        Impact::None
        | Impact::ReferenceResolved
        | Impact::NotApplicable
        | Impact::ObservationCorrelationAmbiguous
        | Impact::NewObservation
        | Impact::RemovedObservation => None,
    };
    if let Some(kind) = impact_kind {
        findings.push(simple(
            kind,
            observation_scope(primary.id),
            Attribution::NotApplicable,
            None,
            vec![primary.id],
            observation_location(primary, side),
            enforce,
        ));
    }
}
