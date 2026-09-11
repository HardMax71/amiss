use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::reference::{GitObject, RefRecord};
use amiss_wire::model::ObjectKind;

#[test]
fn reference_inputs_keep_consumed_facts_from_both_providers()
-> Result<(), Box<dyn std::error::Error>> {
    for (input, reference, sha) in [
        (
            include_str!("../fixtures/gitea-refs.json"),
            "refs/heads/main",
            "58931b5d170d54e3f7e8da9873404f6311e8ecd6",
        ),
        (
            include_str!("../fixtures/forgejo-refs.json"),
            "refs/heads/forgejo",
            "ee74d47e1e302a1f129b0ce4c6b2584503a13790",
        ),
    ] {
        let (records, length): (Vec<RefRecord>, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })?;
        assert_eq!(length, input.len());
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.reference, reference);
        assert_eq!(record.object.kind, ObjectKind::Commit);
        assert_eq!(record.object.sha.as_str(), sha);
        for candidate in [
            serde_json::to_string(&records)?,
            input.replacen('{', r#"{"future":{"nested":[1.5,null,true]},"#, 1),
            input.replacen(r#""object":{"#, r#""object":{"future":false,"#, 1),
        ] {
            assert_eq!(serde_json::from_str::<Vec<RefRecord>>(&candidate)?, records);
        }
        assert_eq!(
            decode_bounded_json::<Vec<RefRecord>, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| serde_json::from_slice(bytes)
            ),
            Err(ProviderError::InvalidResponse)
        );
        for (old, new) in [
            (r#""type":"#, r#""ty\u0070e":"commit","type":"#),
            (r#""type":"commit""#, r#""type":"unknown""#),
            (r#""type":"commit""#, r#""type":null"#),
            (r#""type":"commit""#, r#""type":true"#),
            (r#""type":"commit""#, r#""type":1"#),
            (r#""type":"commit""#, r#""type":["commit"]"#),
            (r#""type":"commit""#, r#""type":{"commit":null}"#),
        ] {
            let invalid = input.replace(old, new);
            assert_ne!(invalid, input);
            assert!(
                serde_json::from_str::<Vec<RefRecord>>(&invalid).is_err(),
                "{new}"
            );
        }
        for invalid in [
            String::new(),
            "g".repeat(40),
            "A".repeat(40),
            "a".repeat(39),
            "a".repeat(41),
        ] {
            assert!(serde_json::from_str::<Vec<RefRecord>>(&input.replace(sha, &invalid)).is_err());
        }
        let positional =
            serde_json::to_vec(&[(&record.reference, (&record.object.kind, &record.object.sha))])?;
        assert_eq!(
            serde_json::from_slice::<Vec<RefRecord>>(&positional)?,
            records
        );
        for invalid in [
            "null",
            "{}",
            "true",
            "1",
            "[{}]",
            r#"[{"ref":"main","object":null}]"#,
        ] {
            assert!(serde_json::from_str::<Vec<RefRecord>>(invalid).is_err());
        }
    }
    Ok(())
}

#[test]
fn object_kinds_use_the_shared_git_vocabulary() -> Result<(), Box<dyn std::error::Error>> {
    for (kind, spelling) in [
        (ObjectKind::Blob, "blob"),
        (ObjectKind::Commit, "commit"),
        (ObjectKind::Tag, "tag"),
        (ObjectKind::Tree, "tree"),
    ] {
        assert_eq!(kind.as_ref(), spelling);
        assert_eq!(
            serde_json::to_string(&kind)?,
            serde_json::to_string(spelling)?
        );
        for sha in ["a".repeat(40), "b".repeat(64)] {
            let object = GitObject {
                kind,
                sha: sha.parse()?,
            };
            let encoded = serde_json::to_vec(&object)?;
            assert_eq!(serde_json::from_slice::<GitObject>(&encoded)?, object);
        }
    }
    Ok(())
}
