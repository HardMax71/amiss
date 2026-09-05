use amiss_wire::assessment::Nullable;
use amiss_wire::controls::{FactSchema, FindingKeyInputSchema, SourceConstruct, TargetKind};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::RepoPath;
use amiss_wire::report::model as report;
use amiss_wire::report::{FindingKind, PAYLOAD_SCHEMA};

#[test]
fn report_producers_can_borrow_validated_text_and_byte_paths() {
    for raw in [
        b"docs/guide.md".to_vec(),
        "docs/quoted-\"β\n.md".as_bytes().to_vec(),
        b"docs/raw-\xff.md".to_vec(),
        [b"docs/".as_slice(), &vec![0xff; 4091]].concat(),
    ] {
        let path = RepoPath::from_bytes(raw.clone()).unwrap();
        for relocation in [None, Some(Nullable::Null), Some(Nullable::Value(&path))] {
            let relocation_value = relocation
                .as_ref()
                .map(|value| serde_json::to_value(value).unwrap());
            let resolution = report::Resolution::Missing {
                kind: report::MissingResolutionKind::Missing,
                detail: report::MissingResolution::PathNotFound {
                    near: Some(&path),
                    path: &path,
                    reason: report::PathNotFoundResolutionReason::PathNotFound,
                    same_object_at: relocation,
                },
            };
            let payload = producer_payload(&path, resolution).unwrap();
            let payload_bytes = serde_json_canonicalizer::to_vec(&payload).unwrap();
            let envelope = report::ReportEnvelope {
                payload,
                payload_digest: hb(PAYLOAD_SCHEMA, &payload_bytes),
                schema: report::ReportEnvelopeSchema::Current,
            };
            let wire = serde_json_canonicalizer::to_vec(&envelope).unwrap();
            let decoded: report::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
            assert_eq!(serde_json_canonicalizer::to_vec(&decoded).unwrap(), wire);
            let json: serde_json::Value = serde_json::from_slice(&wire).unwrap();
            let expected = path.as_str().map_or_else(
                || serde_json::json!({"bytes_hex": hex::encode(&raw)}),
                serde_json::Value::from,
            );
            for pointer in [
                "/payload/documents/0/path",
                "/payload/feedback/items/0/target",
                "/payload/findings/0/location/path",
                "/payload/findings/0/key_input/scope/document",
                "/payload/findings/0/key_input/scope/normalized_target_intent/path",
                "/payload/findings/0/candidate_fact/key_input/scope/document",
                "/payload/findings/0/candidate_fact/key_input/scope/normalized_target_intent/path",
                "/payload/findings/0/candidate_fact/evidence/resolution/path",
            ] {
                assert_eq!(json.pointer(pointer), Some(&expected), "{pointer}");
            }
            assert_eq!(
                json.pointer(
                    "/payload/findings/0/candidate_fact/evidence/resolution/same_object_at"
                ),
                relocation_value.as_ref(),
            );
            assert!(
                json["payload"]["findings"][0]["key_input"]["scope"]["normalized_target_intent"]
                    .get("commit_oid")
                    .is_none()
            );
        }
    }
}

fn producer_payload<'a>(
    path: &'a RepoPath,
    resolution: report::Resolution<&'a RepoPath>,
) -> Result<report::ReportPayload<&'a RepoPath>, serde_json::Error> {
    let template: report::ReportEnvelope = serde_json::from_slice(super::REPORT)?;
    let mut template = template.payload;
    let finding = template.findings.remove(0);
    let digest: Digest = hb("amiss/test-producer-paths", b"fixed fixture");
    let key_input = report::FindingKeyInput {
        finding_kind: FindingKind::ExplicitTargetMissing,
        schema: FindingKeyInputSchema::Current,
        scope: report::FindingKeyScope::Reference {
            document: path,
            kind: report::ReferenceFindingKeyScopeKind::Reference,
            normalized_target_intent: report::RepositoryTargetIntent {
                commit_oid: None,
                fragment_digest: None,
                kind: report::RepositoryIntentKind::RepositoryPath,
                path: report::RepositoryIntentPath::Path(path),
                query_digest: None,
                target_kind: TargetKind::Blob,
            },
            occurrence: report::ReferenceOccurrence {
                kind: report::ReferenceOccurrenceKind::SourceProjection,
                source_projection_digest: digest,
            },
            source_construct: SourceConstruct::InlineLink,
        },
    };
    Ok(report::ReportPayload {
        compatibility: template.compatibility,
        controls: template.controls,
        documents: vec![report::DocumentResult {
            base: None,
            candidate: None,
            change: report::DocumentChange::Unchanged,
            classification: report::DocumentClassification::StructuredMarkdown,
            path,
        }],
        engine: template.engine,
        errors: Vec::<report::AnalysisError<&RepoPath>>::new(),
        evaluation: template.evaluation,
        feedback: report::Feedback::Available(report::AvailableFeedback {
            existing_count: 0,
            items: vec![report::FeedbackItem {
                action: report::FeedbackAction::Check,
                annotation: None,
                effective_disposition: finding.effective_disposition,
                finding_kinds: vec![finding.kind],
                location_count: std::num::NonZeroU64::MIN,
                target: Some(path),
            }],
            status: report::AvailableFeedbackStatus::Available,
        }),
        findings: vec![report::Finding {
            aggregation: finding.aggregation,
            attribution: finding.attribution,
            base_fact: None,
            base_fact_digest: None,
            candidate_fact: Some(report::FindingFactInput {
                evidence: report::FindingFactEvidence::Reference {
                    kind: report::ReferenceFactEvidenceKind::Reference,
                    occurrence_multiplicity: 1,
                    resolution,
                },
                finding_kind: key_input.finding_kind,
                key_input: key_input.clone(),
                schema: FactSchema::Current,
            }),
            candidate_fact_digest: Some(digest),
            configured_disposition: finding.configured_disposition,
            coverage_requirement: finding.coverage_requirement,
            debt: None,
            description: finding.description,
            effective_disposition: finding.effective_disposition,
            evidence_class: finding.evidence_class,
            finding_key: digest,
            fix: None,
            invariant_class: finding.invariant_class,
            key_input,
            kind: finding.kind,
            location: report::FindingLocation {
                path: Some(path),
                side: report::LocationSide::Candidate,
                span: None,
            },
            observation_ids: vec![digest],
            policy_trace: finding.policy_trace,
            waiver: None,
        }],
        observations: Vec::<report::ObservationComparison<&RepoPath>>::new(),
        result: template.result,
        schema: template.schema,
        summary: template.summary,
    })
}
