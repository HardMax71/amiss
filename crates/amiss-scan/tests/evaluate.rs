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
use amiss_wire::controls::{SourceConstruct, TargetKind};
use amiss_wire::digest::hb;
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::report::{Disposition, EngineProvenance, FindingKind, IntentKind};
use amiss_wire::resolution::{
    BlobContent, BlobMode, BlobTarget, ExternalReference, InvalidReference, Missing, Target,
    UnsupportedSemantics, UnsupportedTarget, VersionScope,
};

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine", b"test engine"),
    }
}

fn repo_intent(path: &str) -> Intent {
    Intent {
        kind: IntentKind::RepositoryPath,
        repository_path: RepoPath::new(path.to_owned()),
        target_kind: Some(TargetKind::Either),
        external_scheme: None,
        query: None,
        fragment: None,
    }
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn repo_path(path: &str) -> RepoPath {
    RepoPath::new(path.to_owned()).unwrap()
}

fn available_blob(path: &str, body: &[u8]) -> BlobTarget<RepoPath> {
    BlobTarget {
        path: repo_path(path),
        mode: BlobMode::Regular,
        content: BlobContent::Available {
            raw_digest: hb("amiss/raw-evidence", body),
            projection_digest: hb("amiss/scanner-target-projection", body),
        },
    }
}

fn resolved_blob(path: &str, body: &[u8]) -> Resolution {
    Resolution::Resolved(Target::Blob(available_blob(path, body)))
}

fn lfs_pointer(path: &str) -> Resolution {
    Resolution::Resolved(Target::Blob(BlobTarget {
        path: repo_path(path),
        mode: BlobMode::Regular,
        content: BlobContent::LfsPointer {
            raw_digest: hb("amiss/raw-evidence", b"lfs pointer"),
        },
    }))
}

fn path_not_found(path: &str) -> Resolution {
    Resolution::Missing(Missing::PathNotFound {
        path: repo_path(path),
    })
}

struct Spec {
    document: RepoPath,
    node_path: Vec<usize>,
    block: String,
    intent: Intent,
    resolution: Resolution,
}

fn spec(document: &str, target: &str, resolution: Resolution) -> Spec {
    Spec {
        document: repo_path(document),
        node_path: vec![0, 0],
        block: format!("see [x]({target})"),
        intent: repo_intent(target),
        resolution,
    }
}

fn resolved_spec(document: &str, target: &str) -> Spec {
    spec(document, target, resolved_blob(target, target.as_bytes()))
}

fn missing_spec(document: &str, target: &str) -> Spec {
    spec(document, target, path_not_found(target))
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
        projection_digest: hb("amiss/scanner-source-projection", from.block.as_bytes()),
        raw_destination_digest: hb("amiss/scanner-raw-destination", b"x"),
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
        display: scanned.display,
        block_kind: scanned.occurrence.block_kind,
        node_path: scanned.occurrence.node_path.clone(),
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
            path: RepoPath::new("gone.md".to_owned()).unwrap(),
            base: Some(DocumentSide::Unsupported),
            candidate: None,
        },
        DocumentInput {
            path: RepoPath::new("weird.bin.md".to_owned()).unwrap(),
            base: None,
            candidate: Some(DocumentSide::Unsupported),
        },
        DocumentInput {
            path: RepoPath::new("page.mdx".to_owned()).unwrap(),
            base: None,
            candidate: Some(DocumentSide::Scanned {
                mdx_regions: 2,
                html_regions: 0,
                extracted_references: 0,
            }),
        },
        DocumentInput {
            path: RepoPath::new("vendor.md".to_owned()).unwrap(),
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
    assert_eq!(
        removed.location.path.as_ref().and_then(RepoPath::as_str),
        Some("gone.md")
    );
    assert_eq!(removed.location.span, None);
    assert_eq!(removed.configured_disposition, Disposition::Record);
}

#[test]
fn boundary_kinds_follow_the_mapping() {
    let rows = [
        (
            Resolution::Invalid(InvalidReference::PathTraversal),
            FindingKind::InvalidReference,
        ),
        (
            Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(available_blob(
                "t.md", b"target",
            ))),
            FindingKind::UnsupportedReferenceSemantics,
        ),
        (
            Resolution::UnsupportedSemantics(UnsupportedSemantics::SiteRoute),
            FindingKind::UnsupportedReferenceSemantics,
        ),
        (
            Resolution::UnsupportedVersion(VersionScope::KnownPath {
                path: repo_path("t.md"),
            }),
            FindingKind::UnsupportedVersionScope,
        ),
        (
            Resolution::UnsupportedTarget(UnsupportedTarget::Symlink {
                path: repo_path("t.md"),
            }),
            FindingKind::UnsupportedTargetKind,
        ),
        (
            Resolution::External(ExternalReference::Url),
            FindingKind::ExternalOutOfScope,
        ),
    ];
    for (resolution, expected) in rows {
        let candidate = observation(&spec("d.md", "t.md", resolution));
        let findings = evaluate(&[], &comparisons(Vec::new(), vec![candidate]), false);
        assert!(
            kinds(&findings).contains(&expected),
            "typed boundary emits {expected:?}"
        );
    }

    let pointer = spec("d.md", "t.md", lfs_pointer("t.md"));
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
    let missing = missing_spec("d.md", "absent.md");
    let mut second = missing_spec("d.md", "absent.md");
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
fn every_missing_reason_emits_the_structural_finding() {
    let rows = [
        (
            "absent.md",
            Missing::PathNotFound {
                path: repo_path("absent.md"),
            },
        ),
        (
            "target.rs",
            Missing::LineFragmentOutOfRange {
                path: repo_path("target.rs"),
            },
        ),
    ];
    for (target, missing) in rows {
        let candidate = observation(&spec("d.md", target, Resolution::Missing(missing)));
        let findings = evaluate(&[], &comparisons(Vec::new(), vec![candidate]), false);
        assert!(
            kinds(&findings).contains(&FindingKind::ExplicitTargetMissing),
            "every typed missing reason is structural"
        );
    }
}

#[test]
fn unknown_attribution_needs_unequal_facts_on_one_key() {
    let base = missing_spec("d.md", "absent.md");
    let mut doubled = missing_spec("d.md", "absent.md");
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
    let removed_spec = resolved_spec("d.md", "t.md");
    let findings = evaluate(
        &[],
        &comparisons(vec![observation(&removed_spec)], Vec::new()),
        false,
    );
    let removed = only(findings, FindingKind::ExplicitReferenceRemoved);
    assert_eq!(removed.location.side, LocationSide::Base);
    assert_eq!(removed.configured_disposition, Disposition::Warn);

    let mut lone_base = resolved_spec("d.md", "t.md");
    lone_base.block = "base wording [x](t.md)".to_owned();
    let mut one = resolved_spec("d.md", "t.md");
    one.block = "first candidate [x](t.md)".to_owned();
    let mut two = resolved_spec("d.md", "t.md");
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

    let base_available = spec("d.md", "t.md", resolved_blob("t.md", b"before"));
    let candidate_available = spec("d.md", "t.md", resolved_blob("t.md", b"after"));
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

#[test]
fn findings_sort_by_canonical_key() {
    let one = missing_spec("a.md", "missing-one.md");
    let two = missing_spec("b.md", "missing-two.md");
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
