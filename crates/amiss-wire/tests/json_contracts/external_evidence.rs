use amiss_wire::de::Document as _;
use amiss_wire::envelope::Payload as _;
use amiss_wire::envelope::transcoded_digest;
use amiss_wire::external;
use amiss_wire::external::EVIDENCE_SCHEMA;
use amiss_wire::external::ExternalAssessment;
use amiss_wire::external::ExternalEvidence;
use serde_json::{Value, json};
use sha2::Digest as _;

const EVIDENCE: &[u8] = include_bytes!("../../../../spec/examples/scanner-external-evidence.json");

#[test]
fn assessments_use_the_digest_of_all_evidence_fields() {
    let original: Value = serde_json::from_slice(EVIDENCE).unwrap();
    let expected = ExternalEvidence::parse(EVIDENCE).unwrap();
    let original_digest = transcoded_digest(EVIDENCE_SCHEMA, EVIDENCE).unwrap();
    for path in ["", "/producer", "/rows/0"] {
        let mut extended = original.clone();
        extended.pointer_mut(path).unwrap()["future"] = json!({"😀": "\t", "\u{e000}": null});
        let bytes = serde_json::to_vec_pretty(&extended).unwrap();
        let typed = ExternalEvidence::parse(&bytes).unwrap();
        let digest = transcoded_digest(EVIDENCE_SCHEMA, &bytes).unwrap();
        assert_eq!(typed, expected);
        assert_ne!(digest, original_digest);
        assert_eq!(
            digest,
            amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix(EVIDENCE_SCHEMA)
                    .chain_update([0_u8])
                    .chain_update(
                        serde_json_canonicalizer::to_vec(
                            &serde_json::from_slice::<Value>(&bytes).unwrap()
                        )
                        .unwrap()
                    )
                    .finalize()
                    .0
            )
        );
        assert_eq!(
            transcoded_digest(EVIDENCE_SCHEMA, &serde_json::to_vec(&extended).unwrap()).unwrap(),
            digest
        );
        let assessment = external::assess(
            include_bytes!("../../../../spec/examples/scanner-external-plan.json"),
            &bytes,
            "0.0.0",
            amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix("test")
                    .chain_update([0_u8])
                    .chain_update(b"engine")
                    .finalize()
                    .0,
            ),
        )
        .unwrap();
        assert_eq!(
            ExternalAssessment::parse(&assessment)
                .unwrap()
                .payload
                .subject
                .evidence_digest,
            digest
        );
        extended.pointer_mut(path).unwrap()["future"] = json!(false);
        let changed = ExternalEvidence::parse(&serde_json::to_vec(&extended).unwrap()).unwrap();
        let changed_digest =
            transcoded_digest(EVIDENCE_SCHEMA, &serde_json::to_vec(&extended).unwrap()).unwrap();
        assert_eq!(changed, typed);
        assert_ne!(changed_digest, digest);
    }
}
