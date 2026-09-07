use amiss_wire::report::model::Resolution;

#[test]
fn blob_fragments_reuse_the_producer_type_and_require_its_tag() {
    use amiss_wire::resolution::{
        self, BlobContent, BlobMode, BlobTarget, TaggedBlobTarget, UnsupportedSemantics,
    };
    use strum::IntoDiscriminant;

    let digest = amiss_wire::digest::hb("amiss/test", b"fragment");
    for content in [
        BlobContent::Available {
            raw_digest: digest,
            projection_digest: digest,
        },
        BlobContent::LfsPointer { raw_digest: digest },
    ] {
        for mode in [BlobMode::Regular, BlobMode::Executable] {
            let producer = resolution::Resolution::UnsupportedSemantics(
                UnsupportedSemantics::Fragment(TaggedBlobTarget::Blob(BlobTarget {
                    content,
                    mode,
                    path: "docs/a.md",
                })),
            );
            let encoded =
                String::from_utf8(serde_json_canonicalizer::to_vec(&producer).unwrap()).unwrap();
            let decoded: Resolution = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded.as_ref(), producer.discriminant().as_ref());
            assert_eq!(
                serde_json_canonicalizer::to_vec(&decoded).unwrap(),
                encoded.as_bytes()
            );
            for invalid in [
                encoded.replace("\"kind\":\"blob\"", "\"kind\":\"tree\""),
                encoded.replace("\"kind\":\"blob\",", ""),
            ] {
                assert_ne!(encoded, invalid);
                assert!(
                    serde_json::from_str::<Resolution>(&invalid).is_err(),
                    "{invalid}"
                );
            }
        }
    }
}

#[test]
fn resolution_tags_reject_fields_from_other_variants() {
    for invalid in [
        r#"{"kind":"missing","reason":"label-not-declared","path":"a.md"}"#,
        r#"{"kind":"missing","reason":"line-fragment-out-of-range","path":"a.md","near":null}"#,
        r#"{"kind":"external","reason":"url","path":"a.md"}"#,
        r#"{"kind":"invalid","reason":"syntax","target":{"kind":"tree","path":"docs"}}"#,
        r#"{"kind":"unsupported-semantics","reason":"network-path","target":{"kind":"tree","path":"docs"}}"#,
        r#"{"kind":"unsupported-semantics","reason":"fragment","target":{"kind":"tree","path":"docs"}}"#,
        r#"{"kind":"unsupported-version","scope":{"kind":"unknown-path","path":"a.md"}}"#,
    ] {
        assert!(
            serde_json::from_str::<Resolution>(invalid).is_err(),
            "{invalid}"
        );
    }
}
