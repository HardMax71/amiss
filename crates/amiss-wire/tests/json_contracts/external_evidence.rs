use amiss_wire::external;
use serde_json::{Value, json};
use sha2::Digest as _;

const EVIDENCE: &[u8] = include_bytes!("../../../../spec/examples/scanner-external-evidence.json");

#[test]
fn assessments_use_the_digest_of_all_evidence_fields() {
    let original: Value = serde_json::from_slice(EVIDENCE).unwrap();
    let (expected, original_digest) = external::parse_evidence(EVIDENCE).unwrap();
    for path in ["", "/producer", "/rows/0"] {
        let mut extended = original.clone();
        extended.pointer_mut(path).unwrap()["future"] = json!({"😀": "\t", "\u{e000}": null});
        let bytes = serde_json::to_vec_pretty(&extended).unwrap();
        let (typed, digest) = external::parse_evidence(&bytes).unwrap();
        assert_eq!(typed, expected);
        assert_ne!(digest, original_digest);
        assert_eq!(
            digest,
            amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix(external::EVIDENCE_SCHEMA)
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
            external::parse_evidence(&serde_json::to_vec(&extended).unwrap())
                .unwrap()
                .1,
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
            external::parse_assessment(&assessment)
                .unwrap()
                .payload
                .subject
                .evidence_digest,
            digest
        );
        extended.pointer_mut(path).unwrap()["future"] = json!(false);
        let (changed, changed_digest) =
            external::parse_evidence(&serde_json::to_vec(&extended).unwrap()).unwrap();
        assert_eq!(changed, typed);
        assert_ne!(changed_digest, digest);
    }
}
