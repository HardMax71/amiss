use amiss_controller::{OidPair, RunRefs};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid};
use strum::IntoEnumIterator;

#[test]
fn run_refs_keep_the_stored_layout_and_reject_unknown_or_invalid_fields() {
    let tags = [
        "github",
        "gitlab",
        "gitea",
        "bitbucket-cloud",
        "bitbucket-data-center",
    ];
    let forges: Vec<_> = ForgeDialect::iter().collect();
    assert_eq!(forges.len(), tags.len());
    for (forge, tag) in forges.into_iter().zip(tags) {
        let refs = RunRefs {
            forge,
            candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
            target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            default_branch: BranchRef::new("refs/heads/default".to_owned()).unwrap(),
        };
        let encoded = serde_json::to_string(&refs).unwrap();
        assert_eq!(
            encoded,
            format!(
                r#"{{"forge":"{tag}","candidate":"refs/heads/topic","target":"refs/heads/main","default_branch":"refs/heads/default"}}"#
            )
        );
        assert_eq!(serde_json::from_str::<RunRefs>(&encoded).unwrap(), refs);
        for mutation in [
            encoded.replacen('{', r#"{"unknown":false,"#, 1),
            encoded.replace(r#""candidate""#, r#""other""#),
            encoded.replace("refs/heads/topic", "refs/heads/topic..invalid"),
            encoded.replace("refs/heads/main", "refs/heads/.hidden"),
            encoded.replace("refs/heads/default", "refs/heads/default.lock"),
            encoded.replace(&format!(r#""{tag}""#), r#"{"gitea":null}"#),
        ] {
            assert_ne!(mutation, encoded);
            assert!(serde_json::from_str::<RunRefs>(&mutation).is_err());
        }
    }
}

#[test]
fn oid_pairs_keep_the_stored_layout_and_validate_at_deserialization() {
    for (object_format, width) in [(ObjectFormat::Sha1, 40), (ObjectFormat::Sha256, 64)] {
        let base = "a".repeat(width);
        let candidate = "b".repeat(width);
        let pair = OidPair {
            base: Oid::new(object_format, base.clone()).unwrap(),
            candidate: Oid::new(object_format, candidate.clone()).unwrap(),
        };
        let encoded = serde_json::to_string(&pair).unwrap();
        assert_eq!(
            encoded,
            format!(r#"{{"base":"{base}","candidate":"{candidate}"}}"#)
        );
        assert_eq!(serde_json::from_str::<OidPair>(&encoded).unwrap(), pair);
        for mutation in [
            encoded.replacen('{', r#"{"unknown":false,"#, 1),
            encoded.replace(r#""base""#, r#""other""#),
            encoded.replace(&base, &base.to_ascii_uppercase()),
            encoded.replace(&candidate, "invalid"),
            encoded.replace(&format!(r#""{base}""#), "null"),
        ] {
            assert_ne!(mutation, encoded);
            assert!(serde_json::from_str::<OidPair>(&mutation).is_err());
        }
    }
}
