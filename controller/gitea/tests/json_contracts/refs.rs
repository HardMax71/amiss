use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::reference::{GitObject, RefRecord};
use amiss_wire::model::ObjectKind;

#[test]
fn public_ref_captures_require_complete_git_objects() -> Result<(), Box<dyn std::error::Error>> {
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
                amiss_wire::read_json(bytes, u64::MAX)
            })?;
        assert_eq!(length, input.len());
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.reference, reference);
        assert_eq!(record.object.kind, ObjectKind::Commit);
        assert_eq!(record.object.sha.as_str(), sha);
        assert!(record.url.ends_with(&format!("/git/{reference}")));
        assert!(record.object.url.ends_with(&format!("/git/commits/{sha}")));
        assert_eq!(
            decode_bounded_json::<Vec<RefRecord>, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| { amiss_wire::read_json(bytes, u64::MAX) }
            ),
            Err(ProviderError::InvalidResponse)
        );
        for (old, new) in [
            (r#""ref":"#, r#""extra":true,"ref":"#),
            (r#""type":"#, r#""extra":true,"type":"#),
            (r#""type":"#, r#""ty\u0070e":"commit","type":"#),
            (r#""type":"commit""#, r#""type":"unknown""#),
            (r#""type":"commit""#, r#""type":null"#),
            (r#""type":"commit""#, r#""type":true"#),
            (r#""type":"commit""#, r#""type":1"#),
            (r#""type":"commit""#, r#""type":["commit"]"#),
            (r#""type":"commit""#, r#""type":{"commit":null}"#),
        ] {
            let changed = input.replace(old, new);
            assert_ne!(changed, input);
            assert!(serde_json::from_str::<Vec<RefRecord>>(&changed).is_err());
            assert!(amiss_wire::read_json::<Vec<RefRecord>>(changed.as_bytes(), u64::MAX).is_err());
        }
        for invalid in [
            String::new(),
            "g".repeat(40),
            "A".repeat(40),
            "a".repeat(39),
            "a".repeat(41),
        ] {
            let changed = input.replace(sha, &invalid);
            assert_ne!(changed, input);
            assert!(serde_json::from_str::<Vec<RefRecord>>(&changed).is_err());
        }
        let encoded = serde_json::to_string(&records)?;
        for field in [
            format!(",\"url\":{}", serde_json::to_string(&record.url)?),
            format!(",\"url\":{}", serde_json::to_string(&record.object.url)?),
            format!(",\"object\":{}", serde_json::to_string(&record.object)?),
        ] {
            let changed = encoded.replace(&field, "");
            assert_ne!(changed, encoded);
            assert!(serde_json::from_str::<Vec<RefRecord>>(&changed).is_err());
        }
        let null_object = encoded.replace(&serde_json::to_string(&record.object)?, "null");
        assert_ne!(null_object, encoded);
        assert!(serde_json::from_str::<Vec<RefRecord>>(&null_object).is_err());
        let positional = serde_json::to_vec(&[(
            &record.reference,
            &record.url,
            (&record.object.kind, &record.object.sha, &record.object.url),
        )])?;
        assert_eq!(
            serde_json::from_slice::<Vec<RefRecord>>(&positional)?,
            records
        );
        assert!(amiss_wire::read_json::<Vec<RefRecord>>(&positional, u64::MAX).is_err());
        for invalid in [b"null".as_slice(), b"{}", b"true", b"1", b"[{}]"] {
            assert!(amiss_wire::read_json::<Vec<RefRecord>>(invalid, u64::MAX).is_err());
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
                url: String::new(),
            };
            let encoded = serde_json::to_vec(&object)?;
            assert_eq!(
                amiss_wire::read_json::<GitObject>(&encoded, u64::MAX)?,
                object
            );
        }
    }
    Ok(())
}
