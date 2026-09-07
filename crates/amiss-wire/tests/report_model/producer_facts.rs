use amiss_wire::controls::{FactSchema, FindingKeyInputSchema, SourceConstruct, TargetKind};
use amiss_wire::digest::hb;
use amiss_wire::model::RepoPath;
use amiss_wire::report::FindingKind;
use amiss_wire::report::model as report;
use amiss_wire::resolution::{Missing, Resolution};

#[test]
fn fact_producers_borrow_the_key_and_actual_resolution() -> Result<(), serde_json::Error> {
    let digest = hb("amiss/test-fact", b"source");
    for raw in [
        b"docs/guide.md".to_vec(),
        "docs/quoted-\"β\n.md".as_bytes().to_vec(),
        b"docs/raw-\xff.md".to_vec(),
        [b"docs/".as_slice(), &vec![0xff; 4091]].concat(),
    ] {
        let path = RepoPath::from_bytes(raw).unwrap();
        let key = report::FindingKeyInput {
            finding_kind: FindingKind::ExplicitTargetMissing,
            schema: FindingKeyInputSchema::Current,
            scope: report::FindingKeyScope::Reference {
                document: &path,
                normalized_target_intent: report::RepositoryTargetIntent {
                    commit_oid: None,
                    fragment_digest: None,
                    kind: report::RepositoryIntentKind::RepositoryPath,
                    path: report::RepositoryIntentPath::Path(&path),
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
        for relocation in [None, Some(&path)] {
            let resolution = Resolution::Missing(Missing::PathNotFound {
                path: &path,
                near: None,
                same_object_at: relocation,
            });
            let input = report::FindingFactInput {
                evidence: report::ReferenceFactEvidence {
                    kind: report::ReferenceFactEvidenceKind::Reference,
                    occurrence_multiplicity: 3,
                    resolution: &resolution,
                },
                finding_kind: key.finding_kind,
                key_input: &key,
                schema: FactSchema::Current,
            };
            let encoded = serde_json_canonicalizer::to_vec(&input)?;
            let decoded: report::FindingFactInput = serde_json::from_slice(&encoded)?;
            assert_eq!(serde_json_canonicalizer::to_vec(&decoded)?, encoded);
            let report::FindingFactEvidence::Reference(evidence) = decoded.evidence else {
                panic!("expected reference evidence");
            };
            assert_eq!(evidence.occurrence_multiplicity, 3);
            assert_eq!(
                serde_json_canonicalizer::to_vec(&evidence.resolution)?,
                serde_json_canonicalizer::to_vec(&resolution)?
            );
            assert_eq!(decoded.schema, FactSchema::Current);
            assert_eq!(evidence.kind, report::ReferenceFactEvidenceKind::Reference);
            assert!(matches!(
                evidence.resolution,
                report::Resolution::Missing(report::MissingResolution::PathNotFound {
                    near: None,
                    same_object_at: Some(_),
                    ..
                })
            ));
            assert!(matches!(
                decoded.key_input.scope,
                report::FindingKeyScope::Reference {
                    normalized_target_intent: report::RepositoryTargetIntent {
                        commit_oid: None,
                        ..
                    },
                    ..
                }
            ));
        }
    }
    Ok(())
}
