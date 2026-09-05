use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::resolution::{
    BlobMode, BlobTarget, DeclaredUntracked, Missing, Target, UnsupportedSemantics,
    UnsupportedTarget, VersionScope,
};
use serde::Serialize;

fn assert_json(value: &impl Serialize, expected: &str) -> Result<(), serde_json::Error> {
    let expected: serde_json::Value = serde_json::from_str(expected)?;
    assert_eq!(serde_json::to_value(value)?, expected);
    Ok(())
}

#[test]
fn missing_details_keep_required_nulls_and_reason_specific_fields() -> Result<(), serde_json::Error>
{
    for (missing, expected) in [
        (
            Missing::PathNotFound {
                path: "docs/guide.md",
                near: None,
                same_object_at: None,
            },
            r#"{"reason":"path-not-found","path":"docs/guide.md","near":null,"same_object_at":null}"#,
        ),
        (
            Missing::PathNotFound {
                path: "docs/guide.md",
                near: Some("docs/guide.rst"),
                same_object_at: Some("guide.md"),
            },
            r#"{"reason":"path-not-found","path":"docs/guide.md","near":"docs/guide.rst","same_object_at":"guide.md"}"#,
        ),
        (
            Missing::LineFragmentOutOfRange { path: "guide.md" },
            r#"{"reason":"line-fragment-out-of-range","path":"guide.md"}"#,
        ),
        (
            Missing::HeadingAnchorNotFound {
                path: "guide.md",
                near: None,
            },
            r#"{"reason":"heading-anchor-not-found","path":"guide.md","near":null}"#,
        ),
        (
            Missing::HeadingAnchorNotFound {
                path: "guide.md",
                near: Some("installation".to_owned()),
            },
            r#"{"reason":"heading-anchor-not-found","path":"guide.md","near":"installation"}"#,
        ),
        (
            Missing::LabelNotDeclared,
            r#"{"reason":"label-not-declared"}"#,
        ),
    ] {
        assert_json(&missing, expected)?;
    }
    Ok(())
}

#[test]
fn target_and_version_details_keep_their_wire_fields() -> Result<(), serde_json::Error> {
    assert_json(
        &DeclaredUntracked {
            path: "generated",
            declared_by: ".gitignore",
        },
        r#"{"path":"generated","declared_by":".gitignore"}"#,
    )?;
    for (target, expected) in [
        (
            UnsupportedTarget::Symlink { path: "docs" },
            r#"{"reason":"symlink","path":"docs"}"#,
        ),
        (
            UnsupportedTarget::Gitlink { path: "docs" },
            r#"{"reason":"gitlink","path":"docs"}"#,
        ),
    ] {
        assert_json(&target, expected)?;
    }
    assert_json(
        &VersionScope::KnownPath { path: "docs" },
        r#"{"kind":"known-path","path":"docs"}"#,
    )?;
    assert_json(
        &VersionScope::<&str>::UnknownPath,
        r#"{"kind":"unknown-path"}"#,
    )?;
    for (format, width) in [(ObjectFormat::Sha1, 40), (ObjectFormat::Sha256, 64)] {
        let commit_oid = Oid::new(format, "a".repeat(width)).unwrap();
        let expected =
            format!(r#"{{"kind":"known-commit","commit_oid":"{commit_oid}","path":"docs"}}"#);
        assert_json(
            &VersionScope::KnownCommit {
                commit_oid,
                path: "docs",
            },
            &expected,
        )?;
    }
    Ok(())
}

#[test]
fn semantic_details_tag_blob_fragments_without_changing_target_decoding()
-> Result<(), serde_json::Error> {
    for content in [super::available(), super::pointer()] {
        for mode in [BlobMode::Regular, BlobMode::Executable] {
            let blob = BlobTarget {
                path: "docs/guide.md",
                mode,
                content,
            };
            let target = Target::Blob(blob.clone());
            let expected = serde_json::to_value(&target).unwrap();
            for (semantics, reason) in [
                (UnsupportedSemantics::Fragment(blob), "fragment"),
                (UnsupportedSemantics::Query(target.clone()), "query"),
                (UnsupportedSemantics::CodeFragment(target), "code-fragment"),
            ] {
                let encoded = serde_json::to_value(&semantics).unwrap();
                assert_eq!(encoded["reason"], reason);
                assert_eq!(encoded["target"], expected);
                assert_eq!(encoded.as_object().unwrap().len(), 2);
                let decoded: Target<String> =
                    serde_json::from_value(encoded["target"].clone()).unwrap();
                assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
            }
        }
    }
    for (semantics, expected) in [
        (
            UnsupportedSemantics::<&str>::SiteRoute,
            r#"{"reason":"site-route"}"#,
        ),
        (
            UnsupportedSemantics::NetworkPath,
            r#"{"reason":"network-path"}"#,
        ),
        (
            UnsupportedSemantics::AttributeDependent,
            r#"{"reason":"attribute-dependent"}"#,
        ),
        (
            UnsupportedSemantics::DuplicateLabel,
            r#"{"reason":"duplicate-label"}"#,
        ),
        (
            UnsupportedSemantics::ExternalInventory,
            r#"{"reason":"external-inventory"}"#,
        ),
    ] {
        assert_json(&semantics, expected)?;
    }
    Ok(())
}
