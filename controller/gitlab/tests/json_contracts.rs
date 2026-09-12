use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitlab::tree::TreeObject;

#[path = "json_contracts/claims.rs"]
mod claims;

#[test]
fn tree_pages_keep_typed_entries_without_unused_metadata() {
    let input = include_bytes!("fixtures/tree.json");
    let (rows, length): (Vec<TreeObject>, _) =
        decode_bounded_json(input.as_slice(), None, input.len(), |bytes| {
            serde_json::from_slice(bytes)
        })
        .unwrap();
    assert_eq!(length, input.len());
    assert_eq!(rows, vec![TreeObject::Tree]);
    let encoded = serde_json::to_vec(&rows).unwrap();
    assert_eq!(
        serde_json::from_slice::<Vec<TreeObject>>(&encoded).unwrap(),
        rows
    );
    assert!(
        serde_json::from_str::<Vec<TreeObject>>("[]")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        serde_json::from_str::<Vec<TreeObject>>(r#"[["tree"]]"#).unwrap(),
        rows
    );

    for (kind, expected) in [
        ("blob", TreeObject::Blob),
        ("tree", TreeObject::Tree),
        ("commit", TreeObject::Commit),
    ] {
        for metadata in [
            "",
            r#","id":null,"name":false,"path":[],"mode":0"#,
            r#","future":{"nested":[null,true,{"value":3}]}"#,
        ] {
            let input = format!(r#"[{{"type":"{kind}"{metadata}}}]"#);
            assert_eq!(
                serde_json::from_str::<Vec<TreeObject>>(&input).unwrap(),
                vec![expected.clone()]
            );
        }
    }
    assert_eq!(
        decode_bounded_json::<Vec<TreeObject>, _>(
            input.as_slice(),
            None,
            input.len() - 1,
            |bytes| serde_json::from_slice(bytes)
        ),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn tree_pages_require_unambiguous_known_entry_kinds() {
    for invalid in [
        "null",
        "{}",
        r#"{"type":"tree"}"#,
        "[null]",
        "[true]",
        "[1]",
        "[{}]",
        r#"[["unknown"]]"#,
        r#"[{"type":"unknown"}]"#,
        r#"[{"type":null}]"#,
        r#"[{"type":false}]"#,
        r#"[{"type":{"tree":null}}]"#,
        r#"[{"type":"tree","type":"blob"}]"#,
        r#"[{"type":"tree","\u0074ype":"tree"}]"#,
        r#"[{"type":"tree"}] {}"#,
    ] {
        assert_eq!(
            decode_bounded_json::<Vec<TreeObject>, _>(
                invalid.as_bytes(),
                None,
                invalid.len(),
                |bytes| serde_json::from_slice(bytes)
            ),
            Err(ProviderError::InvalidResponse),
            "{invalid}"
        );
    }
}
