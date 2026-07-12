#![cfg(unix)]

use amiss_md::extract::BlockKind;
use amiss_md::extract::Occurrence;
use amiss_scan::Observation;
use amiss_scan::correlate::{Comparison, Side, correlate};
use amiss_scan::evaluate::{
    Attribution, DocumentInput, DocumentSide, Finding, LocationSide, evaluate,
};
use amiss_scan::observe::occurrence_id;
use amiss_scan::resolve::{Intent, Resolution};
use amiss_scan::scan::{ScannedOccurrence, SpanDisplay};
use amiss_wire::controls::{ContentAvailability, EntryKind, GitMode, SourceConstruct, TargetKind};
use amiss_wire::digest::hb;
use amiss_wire::model::Adapter;
use amiss_wire::report::{Disposition, EngineProvenance, FindingKind, IntentKind, ResolutionCode};

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine/v1", b"test engine"),
    }
}

fn repo_intent(path: &str) -> Intent {
    Intent {
        kind: IntentKind::RepositoryPath,
        repository_path: Some(path.to_owned()),
        target_kind: Some(TargetKind::Either),
        external_scheme: None,
        query: None,
        fragment: None,
    }
}

fn resolution(code: ResolutionCode, path: Option<&str>) -> Resolution {
    Resolution {
        code,
        path: path.map(str::to_owned),
        entry_kind: None,
        git_mode: None,
        raw_digest: None,
        projection_digest: None,
        content_availability: ContentAvailability::NotApplicable,
    }
}

struct Spec {
    document: String,
    node_path: Vec<usize>,
    block: String,
    intent: Intent,
    resolution: Resolution,
}

fn spec(document: &str, target: &str, code: ResolutionCode) -> Spec {
    Spec {
        document: document.to_owned(),
        node_path: vec![0, 0],
        block: format!("see [x]({target})"),
        intent: repo_intent(target),
        resolution: resolution(code, Some(target)),
    }
}

fn observation(from: &Spec) -> Observation {
    let scanned = ScannedOccurrence {
        occurrence: Occurrence {
            construct: SourceConstruct::InlineLink,
            raw_destination: "x".to_owned(),
            semantic_destination: "x".to_owned(),
            span: (4, 10),
            node_path: from.node_path.clone(),
            block_kind: BlockKind::Paragraph,
            block_span: (0, 12),
        },
        display: SpanDisplay {
            start_line: 1,
            start_column: 5,
            end_line: 1,
            end_column: 11,
        },
        projection_digest: hb("amiss/scanner-source-projection/v1", from.block.as_bytes()),
        raw_destination_digest: hb("amiss/scanner-raw-destination/v1", b"x"),
    };
    Observation {
        id: occurrence_id(
            &engine(),
            Adapter::Markdown,
            &from.document,
            &scanned,
            &from.intent,
        ),
        document: from.document.clone(),
        span: (4, 10),
        adapter: Adapter::Markdown,
        construct: SourceConstruct::InlineLink,
        intent: from.intent.clone(),
        raw_destination_digest: scanned.raw_destination_digest,
        projection_digest: scanned.projection_digest,
        resolution: from.resolution.clone(),
    }
}

fn side(observations: Vec<Observation>) -> Side {
    Side {
        observations,
        documents: std::collections::BTreeMap::new(),
    }
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn comparisons(base: Vec<Observation>, candidate: Vec<Observation>) -> Vec<Comparison> {
    correlate(&side(base), &side(candidate)).expect("correlate")
}

fn kinds(findings: &[Finding]) -> Vec<FindingKind> {
    findings.iter().map(|finding| finding.kind).collect()
}

fn only(findings: Vec<Finding>, kind: FindingKind) -> Finding {
    let mut matching: Vec<Finding> = findings
        .into_iter()
        .filter(|finding| finding.kind == kind)
        .collect();
    assert_eq!(matching.len(), 1, "exactly one {kind:?}");
    matching.remove(0)
}

#[test]
fn document_findings_follow_step_one() {
    let documents = vec![
        DocumentInput {
            path: "gone.md".to_owned(),
            base: Some(DocumentSide::Unsupported),
            candidate: None,
        },
        DocumentInput {
            path: "weird.bin.md".to_owned(),
            base: None,
            candidate: Some(DocumentSide::Unsupported),
        },
        DocumentInput {
            path: "page.mdx".to_owned(),
            base: None,
            candidate: Some(DocumentSide::Scanned {
                mdx_regions: 2,
                html_regions: 0,
                extracted_references: 0,
            }),
        },
        DocumentInput {
            path: "vendor.md".to_owned(),
            base: None,
            candidate: Some(DocumentSide::ExcludedBuiltIn),
        },
    ];
    let findings = evaluate(&documents, &[], false);
    let got = kinds(&findings);
    assert_eq!(got.len(), 4);
    assert!(got.contains(&FindingKind::DocumentRemoved));
    assert!(got.contains(&FindingKind::UnsupportedDocumentFormat));
    assert!(got.contains(&FindingKind::OpaqueMdxRegion));
    assert!(got.contains(&FindingKind::UnlinkedDocument));

    let removed = only(findings, FindingKind::DocumentRemoved);
    assert_eq!(removed.location.side, LocationSide::Base);
    assert_eq!(removed.location.path.as_deref(), Some("gone.md"));
    assert_eq!(removed.location.span, None);
    assert_eq!(removed.configured_disposition, Disposition::Record);
}

#[test]
fn boundary_kinds_follow_the_mapping() {
    let rows = [
        (ResolutionCode::PathTraversal, FindingKind::InvalidReference),
        (
            ResolutionCode::UnsupportedFragmentSemantics,
            FindingKind::UnsupportedReferenceSemantics,
        ),
        (
            ResolutionCode::SiteRouteUnsupported,
            FindingKind::UnsupportedReferenceSemantics,
        ),
        (
            ResolutionCode::UnsupportedVersionScope,
            FindingKind::UnsupportedVersionScope,
        ),
        (
            ResolutionCode::SymlinkEntry,
            FindingKind::UnsupportedTargetKind,
        ),
        (ResolutionCode::ExternalUrl, FindingKind::ExternalOutOfScope),
    ];
    for (code, expected) in rows {
        let candidate = observation(&spec("d.md", "t.md", code));
        let findings = evaluate(&[], &comparisons(Vec::new(), vec![candidate]), false);
        assert!(
            kinds(&findings).contains(&expected),
            "{code:?} emits {expected:?}"
        );
    }

    let mut pointer = spec("d.md", "t.md", ResolutionCode::ExactPath);
    pointer.resolution.content_availability = ContentAvailability::LfsPointerOnly;
    pointer.resolution.entry_kind = Some(EntryKind::Blob);
    pointer.resolution.git_mode = Some(GitMode::RegularFile);
    let findings = evaluate(
        &[],
        &comparisons(Vec::new(), vec![observation(&pointer)]),
        false,
    );
    assert_eq!(
        kinds(&findings),
        vec![FindingKind::UnsupportedTargetKind],
        "a compatible pointer emits the content boundary and nothing else"
    );
}

#[test]
fn structural_findings_aggregate_and_attribute() {
    let missing = spec("d.md", "absent.md", ResolutionCode::PathNotFound);
    let mut second = spec("d.md", "absent.md", ResolutionCode::PathNotFound);
    second.node_path = vec![3, 1];

    let introduced = evaluate(
        &[],
        &comparisons(
            Vec::new(),
            vec![observation(&missing), observation(&second)],
        ),
        false,
    );
    let finding = only(introduced, FindingKind::ExplicitTargetMissing);
    assert_eq!(finding.attribution, Attribution::Introduced);
    assert_eq!(finding.member_count, 2, "duplicates share one key");
    assert_eq!(finding.observation_ids.len(), 2);
    assert!(finding.base_fact.is_none() && finding.candidate_fact.is_some());
    assert_eq!(finding.configured_disposition, Disposition::Warn);

    let pre_existing = evaluate(
        &[],
        &comparisons(vec![observation(&missing)], vec![observation(&missing)]),
        false,
    );
    let finding = only(pre_existing, FindingKind::ExplicitTargetMissing);
    assert_eq!(finding.attribution, Attribution::PreExisting);

    let resolved = evaluate(
        &[],
        &comparisons(vec![observation(&missing)], Vec::new()),
        false,
    );
    let removal_and_projection = resolved;
    let finding = only(
        removal_and_projection
            .iter()
            .filter(|finding| finding.kind == FindingKind::ExplicitTargetMissing)
            .cloned()
            .collect(),
        FindingKind::ExplicitTargetMissing,
    );
    assert_eq!(finding.attribution, Attribution::Resolved);
    assert_eq!(
        finding.configured_disposition,
        Disposition::Record,
        "a base-only projection is forced to record even under enforce"
    );
    assert_eq!(finding.location.side, LocationSide::Base);

    let enforced = evaluate(
        &[],
        &comparisons(Vec::new(), vec![observation(&missing)]),
        true,
    );
    assert_eq!(
        only(enforced, FindingKind::ExplicitTargetMissing).configured_disposition,
        Disposition::Fail
    );
}

#[test]
fn unknown_attribution_needs_unequal_facts_on_one_key() {
    let base = spec("d.md", "absent.md", ResolutionCode::PathNotFound);
    let mut doubled = spec("d.md", "absent.md", ResolutionCode::PathNotFound);
    doubled.node_path = vec![7, 0];
    let findings = evaluate(
        &[],
        &comparisons(
            vec![observation(&base)],
            vec![observation(&base), observation(&doubled)],
        ),
        false,
    );
    let finding = only(findings, FindingKind::ExplicitTargetMissing);
    assert_eq!(
        finding.attribution,
        Attribution::Unknown,
        "multiplicity one versus two is an unequal fact body"
    );
}

#[test]
fn comparison_findings_follow_step_four() {
    let removed_spec = spec("d.md", "t.md", ResolutionCode::ExactPath);
    let findings = evaluate(
        &[],
        &comparisons(vec![observation(&removed_spec)], Vec::new()),
        false,
    );
    let removed = only(findings, FindingKind::ExplicitReferenceRemoved);
    assert_eq!(removed.location.side, LocationSide::Base);
    assert_eq!(removed.configured_disposition, Disposition::Warn);

    let mut lone_base = spec("d.md", "t.md", ResolutionCode::ExactPath);
    lone_base.block = "base wording [x](t.md)".to_owned();
    let mut one = spec("d.md", "t.md", ResolutionCode::ExactPath);
    one.block = "first candidate [x](t.md)".to_owned();
    let mut two = spec("d.md", "t.md", ResolutionCode::ExactPath);
    two.block = "second candidate [x](t.md)".to_owned();
    two.node_path = vec![9, 9];
    let ambiguous = evaluate(
        &[],
        &comparisons(
            vec![observation(&lone_base)],
            vec![observation(&one), observation(&two)],
        ),
        false,
    );
    let finding = only(ambiguous, FindingKind::ObservationCorrelationAmbiguous);
    assert_eq!(finding.member_count, 1);

    let mut base_available = spec("d.md", "t.md", ResolutionCode::ExactPath);
    base_available.resolution.content_availability = ContentAvailability::Available;
    base_available.resolution.projection_digest =
        Some(hb("amiss/scanner-target-projection/v1", b"before"));
    let mut candidate_available = base_available_clone(&base_available);
    candidate_available.resolution.projection_digest =
        Some(hb("amiss/scanner-target-projection/v1", b"after"));
    let impact = evaluate(
        &[],
        &comparisons(
            vec![observation(&base_available)],
            vec![observation(&candidate_available)],
        ),
        false,
    );
    let finding = only(impact, FindingKind::DependencyChangedSubjectUnchanged);
    assert_eq!(finding.configured_disposition, Disposition::Warn);
    assert_eq!(finding.attribution, Attribution::NotApplicable);
}

fn base_available_clone(from: &Spec) -> Spec {
    Spec {
        document: from.document.clone(),
        node_path: from.node_path.clone(),
        block: from.block.clone(),
        intent: from.intent.clone(),
        resolution: from.resolution.clone(),
    }
}

#[test]
fn findings_sort_by_canonical_key() {
    let one = spec("a.md", "missing-one.md", ResolutionCode::PathNotFound);
    let two = spec("b.md", "missing-two.md", ResolutionCode::PathNotFound);
    let findings = evaluate(
        &[],
        &comparisons(Vec::new(), vec![observation(&one), observation(&two)]),
        false,
    );
    let keys: Vec<_> = findings.iter().map(|finding| finding.finding_key).collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
}
