use std::sync::Arc;

use amiss_controller::semantic_artifact::input_artifact_size;
use amiss_controller::semantic_artifact::{InputArtifact, InputArtifactRow, InputArtifactSchema};
use amiss_wire::digest::sha256;

#[test]
fn semantic_artifact_bytes_use_the_closed_lossless_serde_contract() {
    for (bytes, encoded) in [
        (b"".as_slice(), ""),
        (b"x".as_slice(), "eA=="),
        (b"xy".as_slice(), "eHk="),
        (b"xyz".as_slice(), "eHl6"),
        (b"\xfb\xff".as_slice(), "+/8="),
    ] {
        let artifact = InputArtifact {
            schema: InputArtifactSchema::Current,
            inputs: vec![InputArtifactRow::<amiss_wire::model::ArtifactId> {
                acquisition_identity: None,
                template_bytes: bytes.into(),
                envelope_bytes: bytes.into(),
                template_digest: sha256(bytes),
                envelope_digest: sha256(bytes),
                payload_digest: sha256(bytes),
            }],
        };
        let wire = serde_json::to_string(&artifact).unwrap();
        for field in ["template_bytes_base64", "envelope_bytes_base64"] {
            let original = format!("\"{field}\":\"{encoded}\"");
            assert_eq!(wire.matches(&original).count(), 1);
            for invalid in [
                "null",
                "[]",
                "{}",
                "42",
                "\"A\"",
                "\"eA===\"",
                "\"eB==\"",
                "\"-_8=\"",
            ] {
                let tampered = wire.replace(&original, &format!("\"{field}\":{invalid}"));
                assert!(
                    amiss_wire::read_json::<InputArtifact>(tampered.as_bytes(), u64::MAX).is_err(),
                    "{field}: {invalid}"
                );
            }
            if encoded.ends_with('=') {
                let unpadded = wire.replace(
                    &original,
                    &format!("\"{field}\":\"{}\"", encoded.trim_end_matches('=')),
                );
                assert!(
                    amiss_wire::read_json::<InputArtifact>(unpadded.as_bytes(), u64::MAX).is_err()
                );
            }
        }
        let exact = u64::try_from(wire.len()).unwrap();
        assert_eq!(input_artifact_size(&artifact, exact).unwrap(), exact);
        assert!(matches!(
            input_artifact_size(&artifact, exact - 1),
            Err(amiss_wire::JsonInputError::LimitExceeded)
        ));
        let parsed: InputArtifact = amiss_wire::read_json(wire.as_bytes(), exact).unwrap();
        assert!(!Arc::ptr_eq(
            &parsed.inputs[0].template_bytes,
            &artifact.inputs[0].template_bytes
        ));
        assert!(!Arc::ptr_eq(
            &parsed.inputs[0].envelope_bytes,
            &artifact.inputs[0].envelope_bytes
        ));
        assert_eq!(parsed.inputs[0].template_bytes.as_ref(), bytes);
        assert_eq!(parsed.inputs[0].envelope_bytes.as_ref(), bytes);
        assert_eq!(serde_json::to_string(&parsed).unwrap(), wire);
        assert!(amiss_wire::read_json::<InputArtifact>(wire.as_bytes(), exact - 1).is_err());
        for invalid in [
            wire.replacen('{', "{\"unknown\":null,", 1),
            wire.replacen("\"inputs\":[{", "\"inputs\":[{\"unknown\":null,", 1),
            wire.replace("\"acquisition_identity\":null,", ""),
            wire.replace(
                "\"acquisition_identity\":null",
                "\"acquisition_identity\":\"../bad\"",
            ),
            wire.replace("\"schema\":", "\"schema\":\"duplicate\",\"schema\":"),
            wire.replace(
                "amiss/controller-semantic-input-artifact-v1",
                "another-artifact",
            ),
            wire.replace(
                "\"schema\":\"amiss/controller-semantic-input-artifact-v1\"",
                "\"schema\":{\"amiss/controller-semantic-input-artifact-v1\":null}",
            ),
            wire.replace(
                &format!("\"envelope_digest\":\"{}\"", sha256(bytes)),
                "\"envelope_digest\":\"SHA256:bad\"",
            ),
            wire.replace(
                &format!("\"payload_digest\":\"{}\"", sha256(bytes)),
                "\"payload_digest\":null",
            ),
            format!("{wire} null"),
        ] {
            assert_ne!(invalid, wire);
            assert!(amiss_wire::read_json::<InputArtifact>(invalid.as_bytes(), u64::MAX).is_err());
        }
    }
}
