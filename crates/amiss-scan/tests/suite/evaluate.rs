use amiss_md::extract::BlockKind;
use amiss_md::extract::Occurrence;
use amiss_scan::Observation;
use amiss_scan::correlate::{Comparison, Side, correlate};
use amiss_scan::evaluate::{
    Attribution, DocumentInput, DocumentSide, Finding, GovernedSeed, LocationSide, evaluate,
    evaluate_with_policy,
};
use amiss_scan::observe::{ObservationIdentity, observation_input};
use amiss_scan::policy::{Effects, TimeContext, WaiverContext};
use amiss_scan::resolve::{Intent, Resolution};
use amiss_scan::scan::{ScannedOccurrence, SpanDisplay};
use amiss_wire::controls::{Profile, SourceConstruct, TargetKind};
use amiss_wire::digest::{hb, hj};
use amiss_wire::model::{Adapter, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::{
    Disposition, EngineProvenance, FindingKind, IntentKind, adapter_contract,
};
use amiss_wire::resolution::{
    BlobContent, BlobMode, BlobTarget, InvalidReference, Missing, Target, UnsupportedSemantics,
    UnsupportedTarget, VersionScope,
};

mod key_contract;

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine", b"test engine"),
    }
}

fn repo_intent(path: &str) -> Intent {
    Intent {
        kind: IntentKind::RepositoryPath,
        commit_oid: None,
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
        near: None,
        same_object_at: None,
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
            fragment_span: None,
            path_span: None,
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
    let adapter_contract_digest = adapter_contract(&engine(), Adapter::Markdown).1;
    let id = hj(
        amiss_scan::observe::OBSERVATION_ID_DOMAIN,
        &observation_input(&ObservationIdentity {
            adapter: Adapter::Markdown,
            contract_digest: adapter_contract_digest,
            document: &from.document,
            construct: scanned.occurrence.construct,
            node_path: &scanned.occurrence.node_path,
            projection_digest: scanned.projection_digest,
            intent: &from.intent,
            raw_destination_digest: scanned.raw_destination_digest,
        }),
    );
    Observation {
        id,
        adapter_contract_digest,
        document: from.document.clone(),
        span: (4, 10),
        display: scanned.display,
        block_kind: scanned.occurrence.block_kind,
        node_path: scanned.occurrence.node_path.clone(),
        adapter: Adapter::Markdown,
        construct: SourceConstruct::InlineLink,
        external_destination: None,
        intent: from.intent.clone(),
        raw_destination: String::new(),
        raw_destination_digest: scanned.raw_destination_digest,
        projection_digest: scanned.projection_digest,
        resolution: from.resolution.clone(),
        fragment_span: None,
        path_span: None,
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
    correlate(side(base), side(candidate)).expect("correlate")
}

fn kinds(findings: &[Finding]) -> Vec<FindingKind> {
    findings
        .iter()
        .map(|finding| finding.key_input.finding_kind)
        .collect()
}

fn only(findings: Vec<Finding>, kind: FindingKind) -> Finding {
    let mut matching: Vec<Finding> = findings
        .into_iter()
        .filter(|finding| finding.key_input.finding_kind == kind)
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
    let findings = evaluate(&documents, &[], Profile::Observe).expect("finding evaluation");
    let got = kinds(&findings);
    assert_eq!(got.len(), 3);
    assert!(got.contains(&FindingKind::DocumentRemoved));
    assert!(got.contains(&FindingKind::UnsupportedDocumentFormat));
    assert!(got.contains(&FindingKind::OpaqueMdxRegion));
    assert!(!got.contains(&FindingKind::UnlinkedDocument));

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
    ];
    for (resolution, expected) in rows {
        let candidate = observation(&spec("d.md", "t.md", resolution));
        let findings = evaluate(
            &[],
            &comparisons(Vec::new(), vec![candidate]),
            Profile::Observe,
        )
        .expect("finding evaluation");
        assert!(
            kinds(&findings).contains(&expected),
            "typed boundary emits {expected:?}"
        );
    }

    let pointer = spec("d.md", "t.md", lfs_pointer("t.md"));
    let findings = evaluate(
        &[],
        &comparisons(Vec::new(), vec![observation(&pointer)]),
        Profile::Observe,
    )
    .expect("finding evaluation");
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
        Profile::Observe,
    )
    .expect("finding evaluation");
    let finding = only(introduced, FindingKind::ExplicitTargetMissing);
    assert_eq!(finding.attribution, Attribution::Introduced);
    assert_eq!(finding.member_count, 2, "duplicates share one key");
    assert_eq!(finding.observation_ids.len(), 2);
    assert!(finding.base_fact.as_ref().is_none() && finding.candidate_fact.as_ref().is_some());
    assert_eq!(finding.configured_disposition, Disposition::Warn);

    let pre_existing = evaluate(
        &[],
        &comparisons(vec![observation(&missing)], vec![observation(&missing)]),
        Profile::Observe,
    )
    .expect("finding evaluation");
    let finding = only(pre_existing, FindingKind::ExplicitTargetMissing);
    assert_eq!(finding.attribution, Attribution::PreExisting);

    let resolved = evaluate(
        &[],
        &comparisons(vec![observation(&missing)], Vec::new()),
        Profile::Observe,
    )
    .expect("finding evaluation");
    let removal_and_projection = resolved;
    let finding = only(
        removal_and_projection
            .iter()
            .filter(|finding| finding.key_input.finding_kind == FindingKind::ExplicitTargetMissing)
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
        Profile::Enforce,
    )
    .expect("finding evaluation");
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
                near: None,
                same_object_at: None,
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
        let findings = evaluate(
            &[],
            &comparisons(Vec::new(), vec![candidate]),
            Profile::Observe,
        )
        .expect("finding evaluation");
        assert!(
            kinds(&findings).contains(&FindingKind::ExplicitTargetMissing),
            "every typed missing reason is structural"
        );
    }
}

#[test]
fn immutable_commits_keep_separate_structural_finding_keys() {
    let mut first = missing_spec("d.md", "absent.md");
    first.block = "two immutable references".to_owned();
    first.intent.kind = IntentKind::SameRepositoryGithub;
    first.intent.commit_oid = Oid::new(ObjectFormat::Sha1, "a".repeat(40));
    let mut second = missing_spec("d.md", "absent.md");
    second.block = first.block.clone();
    second.intent.kind = IntentKind::SameRepositoryGithub;
    second.intent.commit_oid = Oid::new(ObjectFormat::Sha1, "b".repeat(40));

    let findings = evaluate(
        &[],
        &comparisons(Vec::new(), vec![observation(&first), observation(&second)]),
        Profile::Observe,
    )
    .expect("finding evaluation");
    assert_eq!(
        findings
            .iter()
            .filter(|finding| finding.key_input.finding_kind == FindingKind::ExplicitTargetMissing)
            .count(),
        2
    );
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
        Profile::Observe,
    )
    .expect("finding evaluation");
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
        Profile::Observe,
    )
    .expect("finding evaluation");
    let removed = only(findings, FindingKind::ExplicitReferenceRemoved);
    assert_eq!(removed.location.side, LocationSide::Base);
    assert_eq!(removed.configured_disposition, Disposition::Record);
    assert_eq!(removed.steps[0].after, Disposition::Record);

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
        Profile::Observe,
    )
    .expect("finding evaluation");
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
        Profile::Observe,
    )
    .expect("finding evaluation");
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
        Profile::Observe,
    )
    .expect("finding evaluation");
    let keys: Vec<_> = findings.iter().map(|finding| finding.finding_key).collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
}

fn invalid_spec(document: &str, target: &str) -> Spec {
    spec(
        document,
        target,
        Resolution::Invalid(InvalidReference::PathTraversal),
    )
}

/// A pre-existing invalid reference is the same invalid destination, not
/// merely another invalid one at the same place.
#[test]
fn an_invalid_attribution_needs_the_same_destination() {
    let same = invalid_spec("d.md", "../out.md");
    let paired = comparisons(vec![observation(&same)], vec![observation(&same)]);
    let finding = only(
        evaluate(&[], &paired, Profile::Observe).expect("finding evaluation"),
        FindingKind::InvalidReference,
    );
    assert_eq!(finding.attribution, Attribution::PreExisting);

    let mut base = observation(&same);
    base.raw_destination_digest = hb("amiss/scanner-raw-destination", b"elsewhere");
    let moved = comparisons(vec![base], vec![observation(&same)]);
    let finding = only(
        evaluate(&[], &moved, Profile::Observe).expect("finding evaluation"),
        FindingKind::InvalidReference,
    );
    assert_eq!(
        finding.attribution,
        Attribution::Introduced,
        "an invalid base pointing elsewhere never carried this reference"
    );
}

/// Enforce-introduced demotes exactly the pre-existing failures.
#[test]
fn introduced_only_demotes_pre_existing_failures_alone() {
    let carried = missing_spec("d.md", "absent.md");
    let mut fresh = missing_spec("d.md", "new.md");
    fresh.node_path = vec![3, 1];
    fresh.intent = repo_intent("new.md");
    let comparisons = comparisons(
        vec![observation(&carried)],
        vec![observation(&carried), observation(&fresh)],
    );
    let governed = GovernedSeed {
        document: repo_path("governed.md"),
        member_count: 1,
        sources: vec![(hb("amiss/scanner-source-projection", b"governed"), 1)],
        representative_span: None,
        representative_display: None,
    };
    let (findings, errors) = evaluate_with_policy(
        &[],
        &comparisons,
        Profile::EnforceIntroduced,
        &Effects::default(),
        std::slice::from_ref(&governed),
        &[],
    )
    .expect("finding evaluation");
    assert!(errors.is_empty());

    let mut demoted: Vec<(Attribution, Disposition)> = findings
        .iter()
        .filter(|finding| finding.key_input.finding_kind == FindingKind::ExplicitTargetMissing)
        .map(|finding| (finding.attribution, finding.effective_disposition))
        .collect();
    demoted.sort_by_key(|(attribution, _)| format!("{attribution:?}"));
    assert_eq!(
        demoted,
        vec![
            (Attribution::Introduced, Disposition::Fail),
            (Attribution::PreExisting, Disposition::Warn),
        ],
        "the carried failure warns while the fresh one still fails"
    );
    let control = only(findings, FindingKind::UnsupportedCapability);
    assert_eq!(
        control.effective_disposition,
        Disposition::Fail,
        "a failure nobody carried in is not pre-existing"
    );
    assert!(
        control
            .steps
            .iter()
            .all(|step| !step.rule_id.contains("enforce-introduced")),
        "{:?}",
        control.steps
    );
}

/// A raise names a stronger disposition or says nothing at all.
#[test]
fn a_raise_to_the_standing_disposition_adds_no_step() {
    let missing = missing_spec("d.md", "absent.md");
    let comparisons = comparisons(Vec::new(), vec![observation(&missing)]);
    let policy = Effects {
        raised: vec![(FindingKind::ExplicitTargetMissing, Disposition::Fail)],
        ..Effects::default()
    };
    let (findings, _errors) =
        evaluate_with_policy(&[], &comparisons, Profile::Enforce, &policy, &[], &[])
            .expect("finding evaluation");
    let finding = only(findings, FindingKind::ExplicitTargetMissing);
    assert_eq!(finding.effective_disposition, Disposition::Fail);
    assert_eq!(
        finding.steps.len(),
        1,
        "enforce already fails, so the repository raise is silent"
    );
}

/// A document on both sides was not removed, and a document with no opaque
/// regions has none to report.
#[test]
fn a_present_document_is_neither_removed_nor_opaque() {
    let documents = vec![DocumentInput {
        path: RepoPath::new("page.md".to_owned()).unwrap(),
        base: Some(DocumentSide::Scanned {
            mdx_regions: 0,
            html_regions: 0,
            extracted_references: 1,
        }),
        candidate: Some(DocumentSide::Scanned {
            mdx_regions: 0,
            html_regions: 0,
            extracted_references: 1,
        }),
    }];
    let got = kinds(&evaluate(&documents, &[], Profile::Observe).expect("finding evaluation"));
    assert!(!got.contains(&FindingKind::DocumentRemoved));
    assert!(!got.contains(&FindingKind::OpaqueMdxRegion));
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn owner(raw: &str) -> amiss_wire::model::OwnerId {
    amiss_wire::model::OwnerId::new(raw.to_owned()).expect("an owner id")
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn tree() -> amiss_wire::model::TreeIdentity {
    amiss_wire::model::TreeIdentity {
        object_format: ObjectFormat::Sha1,
        tree_oid: Oid::new(ObjectFormat::Sha1, "a".repeat(40)).expect("a tree identity"),
    }
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn moment(raw: &str) -> amiss_wire::model::UtcInstant {
    amiss_wire::model::UtcInstant::new(raw.to_owned()).expect("an instant")
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn waived_fact() -> amiss_wire::controls::Fact {
    amiss_wire::controls::Fact {
        schema: amiss_wire::controls::FactSchema::Current,
        finding_kind: amiss_wire::controls::EligibleFindingKind::ExplicitTargetMissing,
        key_input: amiss_wire::controls::FindingKeyInput {
            schema: amiss_wire::controls::FindingKeyInputSchema::Current,
            finding_kind: amiss_wire::controls::EligibleFindingKind::ExplicitTargetMissing,
            scope: amiss_wire::controls::FindingScope {
                kind: amiss_wire::controls::ReferenceScopeKind::Reference,
                document: amiss_wire::model::RepoPathText::new("d.md".to_owned()).expect("path"),
                source_construct: SourceConstruct::InlineLink,
                normalized_target_intent: amiss_wire::controls::TargetIntent {
                    kind: amiss_wire::controls::TargetIntentKind::RepositoryPath,
                    commit_oid: None,
                    path: amiss_wire::model::RepoPathText::new("absent.md".to_owned())
                        .expect("path"),
                    target_kind: TargetKind::Either,
                    query_digest: None,
                    fragment_digest: None,
                },
                occurrence: amiss_wire::controls::FindingOccurrence {
                    kind: amiss_wire::controls::OccurrenceKind::SourceProjection,
                    source_projection_digest: hb("amiss/scanner-source-projection", b"block"),
                },
            },
        },
        evidence: amiss_wire::controls::FactEvidence {
            kind: amiss_wire::controls::FactEvidenceKind::Reference,
            resolution: amiss_wire::controls::StructuralResolution::Missing(
                amiss_wire::controls::MissingResolution::PathNotFound {
                    path: amiss_wire::model::RepoPathText::new("absent.md".to_owned())
                        .expect("path"),
                    near: None,
                    same_object_at: None,
                },
            ),
            occurrence_multiplicity: 1,
        },
    }
}

/// A waiver is live from its activation instant, not one tick after it.
#[test]
fn a_waiver_active_at_this_very_instant_is_not_early() {
    let instant = moment("2026-07-02T00:00:00Z");
    let item = amiss_wire::controls::WaiverItem {
        waiver_id: amiss_wire::model::ArtifactId::new("waiver/one".to_owned()).expect("id"),
        finding_key: hb("amiss/scanner-finding-key", b"key"),
        authorized_fact: waived_fact(),
        authorized_fact_digest: hb("amiss/scanner-fact", b"fact"),
        candidate_tree: tree(),
        owner: owner("team:docs"),
        issuer: owner("team:release"),
        reason: "window".to_owned(),
        created_at: moment("2026-07-01T00:00:00Z"),
        not_before: instant.clone(),
        expires_at: moment("2026-08-01T00:00:00Z"),
        residual_disposition: amiss_wire::controls::WaiverResidualDisposition::Warn,
    };
    let statement = amiss_wire::controls::TrustedTimeStatement {
        schema: amiss_wire::controls::TrustedTimeSchema::Current,
        controller: amiss_wire::controls::TrustedTimeController::ExternalRequiredCheckClock,
        repository: amiss_wire::model::RepositoryIdentity::github(
            "acme".to_owned(),
            "widget".to_owned(),
        )
        .expect("identity"),
        ref_name: amiss_wire::model::BranchRef::new("refs/heads/main".to_owned()).expect("ref"),
        candidate_identity_digest: hb("amiss/raw-evidence", b"candidate"),
        provider: "github-actions".to_owned(),
        provider_run_id: "run/1".to_owned(),
        provider_run_attempt: 1,
        evaluation_instant: instant,
        valid_until: moment("2026-07-02T00:10:00Z"),
    };
    let (_, time_digest) =
        amiss_wire::controls::canonical_trusted_time(&statement).expect("trusted time");
    let policy = Effects {
        waiver: Some(WaiverContext {
            digest: hb("amiss/raw-evidence", b"bundle"),
            trust_source: amiss_wire::requests::RequestTrust::OrganizationPolicy,
            candidate_tree: tree(),
            items: vec![item],
            authorized_issuers: vec![owner("team:release")],
            waivable_kinds: vec![amiss_wire::controls::EligibleFindingKind::ExplicitTargetMissing],
        }),
        time: Some(TimeContext {
            statement,
            digest: time_digest,
        }),
        ..Effects::default()
    };
    let (findings, _errors) = evaluate_with_policy(&[], &[], Profile::Enforce, &policy, &[], &[])
        .expect("finding evaluation");
    assert!(
        !kinds(&findings).contains(&FindingKind::WaiverInvalid),
        "a waiver live from this instant carries no defect"
    );
}
