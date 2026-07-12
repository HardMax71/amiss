#![cfg(unix)]

use amiss_md::extract::{BlockKind, Occurrence};
use amiss_scan::correlate::{
    Comparison, Impact, Observation, Outcome, Reason, Side, SourceChange, TargetChange, correlate,
};
use amiss_scan::observe::occurrence_id;
use amiss_scan::resolve::{Intent, Resolution};
use amiss_scan::scan::{ScannedOccurrence, SpanDisplay};
use amiss_wire::controls::{ContentAvailability, EntryKind, GitMode, SourceConstruct, TargetKind};
use amiss_wire::digest::hb;
use amiss_wire::model::Adapter;
use amiss_wire::report::{EngineProvenance, IntentKind, ResolutionCode};

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

fn resolved(path: &str, body: &[u8]) -> Resolution {
    let raw = hb("amiss/raw-evidence/v1", body);
    Resolution {
        code: ResolutionCode::ExactPath,
        path: Some(path.to_owned()),
        entry_kind: Some(EntryKind::Blob),
        git_mode: Some(GitMode::RegularFile),
        raw_digest: Some(raw),
        projection_digest: Some(hb("amiss/scanner-target-projection/v1", body)),
        content_availability: ContentAvailability::Available,
    }
}

fn missing(path: &str) -> Resolution {
    Resolution {
        code: ResolutionCode::PathNotFound,
        path: Some(path.to_owned()),
        entry_kind: None,
        git_mode: None,
        raw_digest: None,
        projection_digest: None,
        content_availability: ContentAvailability::NotApplicable,
    }
}

#[derive(Clone)]
struct Spec {
    document: String,
    node_path: Vec<usize>,
    raw_destination: String,
    block: String,
    intent: Intent,
    resolution: Resolution,
}

fn observation(spec: &Spec) -> Observation {
    let scanned = ScannedOccurrence {
        occurrence: Occurrence {
            construct: SourceConstruct::InlineLink,
            raw_destination: spec.raw_destination.clone(),
            semantic_destination: spec.raw_destination.clone(),
            span: (0, 1),
            node_path: spec.node_path.clone(),
            block_kind: BlockKind::Paragraph,
            block_span: (0, 1),
        },
        display: SpanDisplay {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 2,
        },
        projection_digest: hb("amiss/scanner-source-projection/v1", spec.block.as_bytes()),
        raw_destination_digest: hb(
            "amiss/scanner-raw-destination/v1",
            spec.raw_destination.as_bytes(),
        ),
    };
    Observation {
        id: occurrence_id(
            &engine(),
            Adapter::Markdown,
            &spec.document,
            &scanned,
            &spec.intent,
        ),
        document: spec.document.clone(),
        span: (0, 1),
        adapter: Adapter::Markdown,
        construct: SourceConstruct::InlineLink,
        intent: spec.intent.clone(),
        raw_destination_digest: scanned.raw_destination_digest,
        projection_digest: scanned.projection_digest,
        resolution: spec.resolution.clone(),
    }
}

fn side(observations: Vec<Observation>) -> Side {
    let mut documents = std::collections::BTreeMap::new();
    for entry in &observations {
        documents.insert(
            entry.document.clone(),
            (
                GitMode::RegularFile,
                hb("amiss/raw-evidence/v1", entry.document.as_bytes()),
            ),
        );
    }
    Side {
        observations,
        documents,
    }
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn run(base: &Side, candidate: &Side) -> Vec<Comparison> {
    correlate(base, candidate).expect("correlate")
}

fn basic(document: &str, target: &str, block: &str) -> Spec {
    Spec {
        document: document.to_owned(),
        node_path: vec![0, 0],
        raw_destination: "x".to_owned(),
        block: block.to_owned(),
        intent: repo_intent(target),
        resolution: resolved(target, b"target body"),
    }
}

#[test]
fn identical_occurrences_pair_exactly_with_no_impact() {
    let spec = basic("docs/a.md", "docs/b.md", "see [x](x)");
    let got = run(
        &side(vec![observation(&spec)]),
        &side(vec![observation(&spec)]),
    );
    assert_eq!(got.len(), 1);
    let Some(row) = got.first() else { return };
    assert_eq!(row.outcome, Outcome::Exact);
    assert_eq!(row.reason, Reason::SameExtractionKeyAndProjection);
    assert_eq!(row.source_change, SourceChange::Equal);
    assert_eq!(row.target_change, TargetChange::Equal);
    assert_eq!(row.impact, Impact::None);
}

#[test]
fn an_address_change_alone_is_a_candidate_with_equal_projection() {
    let base = basic("docs/a.md", "docs/b.md", "see [x](x)");
    let mut moved = basic("docs/a.md", "docs/b.md", "see [x](x)");
    moved.node_path = vec![2, 1];
    let got = run(
        &side(vec![observation(&base)]),
        &side(vec![observation(&moved)]),
    );
    assert_eq!(got.len(), 1);
    let Some(row) = got.first() else { return };
    assert_eq!(row.outcome, Outcome::Candidate);
    assert_eq!(row.reason, Reason::SameIntentUnchangedProjection);
    assert_eq!(row.source_change, SourceChange::Equal);
    assert_eq!(row.impact, Impact::None);
}

#[test]
fn an_escape_spelling_change_is_candidate_never_exact() {
    let base = basic("docs/a.md", "docs/b.md", "see [x](x)");
    let mut respelled = basic("docs/a.md", "docs/b.md", "see [x](x)");
    respelled.raw_destination = "docs%2Fb.md".to_owned();
    let got = run(
        &side(vec![observation(&base)]),
        &side(vec![observation(&respelled)]),
    );
    assert_eq!(got.len(), 1);
    assert_eq!(got.first().map(|row| row.outcome), Some(Outcome::Candidate));
}

#[test]
fn the_derivation_table_is_total() {
    let source_changed = |from: &Spec| {
        let mut changed = from.clone();
        changed.block = "reworded [x](x)".to_owned();
        changed
    };

    let base = basic("d.md", "t.md", "same [x](x)");
    let mut target_changed = basic("d.md", "t.md", "same [x](x)");
    target_changed.resolution = resolved("t.md", b"different target body");
    let got = run(
        &side(vec![observation(&base)]),
        &side(vec![observation(&target_changed)]),
    );
    assert_eq!(
        got.first().map(|row| (row.target_change, row.impact)),
        Some((
            TargetChange::Changed,
            Impact::DependencyChangedSubjectUnchanged
        ))
    );

    let cochanged = source_changed(&target_changed);
    let got = run(
        &side(vec![observation(&base)]),
        &side(vec![observation(&cochanged)]),
    );
    assert_eq!(
        got.first().map(|row| (row.source_change, row.impact)),
        Some((SourceChange::Changed, Impact::DependencyAndSubjectCochanged))
    );

    let subject_only = source_changed(&base);
    let got = run(
        &side(vec![observation(&base)]),
        &side(vec![observation(&subject_only)]),
    );
    assert_eq!(
        got.first().map(|row| (row.target_change, row.impact)),
        Some((TargetChange::Equal, Impact::SubjectChanged))
    );

    let mut was_missing = basic("d.md", "t.md", "same [x](x)");
    was_missing.resolution = missing("t.md");
    let got = run(
        &side(vec![observation(&was_missing)]),
        &side(vec![observation(&base)]),
    );
    assert_eq!(
        got.first().map(|row| (row.target_change, row.impact)),
        Some((TargetChange::NewlyResolved, Impact::ReferenceResolved))
    );

    let got = run(
        &side(vec![observation(&base)]),
        &side(vec![observation(&was_missing)]),
    );
    assert_eq!(
        got.first().map(|row| (row.target_change, row.impact)),
        Some((TargetChange::BecameMissing, Impact::NotApplicable))
    );

    let both_missing = run(
        &side(vec![observation(&was_missing)]),
        &side(vec![observation(&was_missing)]),
    );
    assert_eq!(
        both_missing
            .first()
            .map(|row| (row.outcome, row.target_change, row.impact)),
        Some((Outcome::Exact, TargetChange::Equal, Impact::None))
    );

    let mut external = basic("d.md", "t.md", "same [x](x)");
    external.intent = Intent {
        kind: IntentKind::ExternalUrl,
        repository_path: None,
        target_kind: None,
        external_scheme: Some("https".to_owned()),
        query: None,
        fragment: None,
    };
    external.resolution = Resolution {
        code: ResolutionCode::ExternalUrl,
        path: None,
        entry_kind: None,
        git_mode: None,
        raw_digest: None,
        projection_digest: None,
        content_availability: ContentAvailability::NotApplicable,
    };
    let got = run(
        &side(vec![observation(&external)]),
        &side(vec![observation(&external)]),
    );
    assert_eq!(
        got.first().map(|row| (row.target_change, row.impact)),
        Some((TargetChange::NotComparable, Impact::NotApplicable))
    );
}

#[test]
fn ambiguity_selects_smallest_primaries_and_keeps_alternatives() {
    let base = basic("d.md", "t.md", "one [x](x)");
    let first = basic("d.md", "t.md", "candidate one [x](x)");
    let mut second = basic("d.md", "t.md", "candidate two [x](x)");
    second.node_path = vec![5, 0];
    let base_side = side(vec![observation(&base)]);
    let candidate_side = side(vec![observation(&first), observation(&second)]);
    let got = run(&base_side, &candidate_side);
    assert_eq!(got.len(), 1);
    let Some(row) = got.first() else { return };
    assert_eq!(row.outcome, Outcome::Ambiguous);
    assert_eq!(row.reason, Reason::MultipleCounterparts);
    assert_eq!(row.source_change, SourceChange::Unknown);
    assert_eq!(row.impact, Impact::ObservationCorrelationAmbiguous);
    let primary = row.candidate.as_ref().map(|observation| observation.id);
    let alternative = row
        .alternatives_candidate
        .first()
        .map(|observation| observation.id);
    assert!(
        primary < alternative,
        "the primary is the smallest identity"
    );
    assert_eq!(
        row.alternatives_base,
        Vec::new(),
        "a lone base side has no alternatives"
    );
}

#[test]
fn isolated_occurrences_are_added_or_removed() {
    let only_base = basic("d.md", "gone.md", "old [x](x)");
    let only_candidate = basic("d.md", "new.md", "new [x](x)");
    let got = run(
        &side(vec![observation(&only_base)]),
        &side(vec![observation(&only_candidate)]),
    );
    assert_eq!(got.len(), 2);
    let outcomes: Vec<(Outcome, Reason, SourceChange, Impact)> = got
        .iter()
        .map(|row| (row.outcome, row.reason, row.source_change, row.impact))
        .collect();
    assert!(outcomes.contains(&(
        Outcome::None,
        Reason::NewObservation,
        SourceChange::Added,
        Impact::NewObservation
    )));
    assert!(outcomes.contains(&(
        Outcome::None,
        Reason::RemovedObservation,
        SourceChange::Removed,
        Impact::RemovedObservation
    )));
}

#[test]
fn an_exact_rename_pairs_only_unique_content() {
    let moved = |document: &str| basic(document, "shared/t.md", "kept [x](x)");
    let base_spec = moved("old/name.md");
    let candidate_spec = moved("new/name.md");

    let digest = hb("amiss/raw-evidence/v1", b"the very same document bytes");
    let mut base_side = side(vec![observation(&base_spec)]);
    base_side
        .documents
        .insert("old/name.md".to_owned(), (GitMode::RegularFile, digest));
    base_side.documents.remove("new/name.md");
    let mut candidate_side = side(vec![observation(&candidate_spec)]);
    candidate_side
        .documents
        .insert("new/name.md".to_owned(), (GitMode::RegularFile, digest));
    candidate_side.documents.remove("old/name.md");

    let got = run(&base_side, &candidate_side);
    assert_eq!(got.len(), 1);
    assert_eq!(
        got.first().map(|row| (row.outcome, row.reason)),
        Some((
            Outcome::Candidate,
            Reason::ExactDocumentRenameUnchangedProjection
        ))
    );

    let mut duplicated = candidate_side.clone();
    duplicated
        .documents
        .insert("another/copy.md".to_owned(), (GitMode::RegularFile, digest));
    let got = run(&base_side, &duplicated);
    assert_eq!(got.len(), 2, "duplicate content forms no rename edge");
    assert!(got.iter().all(|row| row.outcome == Outcome::None));
}

#[test]
fn duplicate_identities_within_a_side_are_internal_defects() {
    let spec = basic("d.md", "t.md", "same [x](x)");
    let doubled = Side {
        observations: vec![observation(&spec), observation(&spec)],
        documents: std::collections::BTreeMap::new(),
    };
    assert_eq!(
        correlate(&doubled, &Side::default()),
        Err(amiss_scan::Error::Internal)
    );
}
