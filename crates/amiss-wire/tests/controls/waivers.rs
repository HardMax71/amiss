use amiss_wire::controls::{
    EntryKind, FACT_DOMAIN, FINDING_KEY_DOMAIN, IncludeKind, PromotableFindingKind,
    SourceConstruct, TargetKind, WaiverBundle,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hj;
use amiss_wire::json;

use crate::support::{
    DEFAULT_CONSTRUCT, computed_digests, fact_json, key_input_json, waiver_bundle, waiver_item,
};

#[test]
fn every_source_construct_survives_the_waiver_round_trip() {
    for construct in [
        SourceConstruct::InlineLink,
        SourceConstruct::FullReferenceLink,
        SourceConstruct::CollapsedReferenceLink,
        SourceConstruct::ShortcutReferenceLink,
        SourceConstruct::Autolink,
        SourceConstruct::InlineImage,
        SourceConstruct::FullReferenceImage,
        SourceConstruct::CollapsedReferenceImage,
        SourceConstruct::ShortcutReferenceImage,
    ] {
        let named = construct.as_str();
        let key_input = key_input_json("explicit-target-missing").replace(DEFAULT_CONSTRUCT, named);
        let key = hj(
            FINDING_KEY_DOMAIN,
            &json::parse(key_input.as_bytes()).unwrap(),
        )
        .to_string();
        let fact_doc = fact_json().replace(DEFAULT_CONSTRUCT, named);
        let fact = hj(FACT_DOMAIN, &json::parse(fact_doc.as_bytes()).unwrap()).to_string();
        let item = waiver_item("waiver/one", &key, &fact, "team:release-engineering")
            .replace(DEFAULT_CONSTRUCT, named);
        let bundle = WaiverBundle::parse(waiver_bundle(&[item]).as_bytes())
            .unwrap_or_else(|error| panic!("{named} is a construct the bundle accepts: {error:?}"));
        let parsed = bundle
            .items
            .first()
            .map(|item| item.authorized_fact.key_input().scope.source_construct);
        assert_eq!(
            parsed,
            Some(construct),
            "{named} decodes to its own variant"
        );
        assert_eq!(construct.is_image(), named.contains("image"), "{named}");
    }
}

#[test]
fn wire_spellings_are_the_ones_the_contract_publishes() {
    assert_eq!(IncludeKind::Document.as_str(), "document");
    assert_eq!(IncludeKind::Tree.as_str(), "tree");
    assert_eq!(EntryKind::Blob.as_str(), "blob");
    assert_eq!(EntryKind::Gitlink.as_str(), "gitlink");
    assert_eq!(
        PromotableFindingKind::InvalidReference.as_str(),
        "invalid-reference"
    );
}

#[test]
fn a_waiver_answers_for_every_spelling_its_scope_may_carry() {
    let bundle_for = |edit: &dyn Fn(String) -> String| {
        let key_input = edit(key_input_json("explicit-target-missing"));
        let key = hj(
            FINDING_KEY_DOMAIN,
            &json::parse(key_input.as_bytes()).unwrap(),
        )
        .to_string();
        let fact_doc = edit(fact_json());
        let fact = hj(FACT_DOMAIN, &json::parse(fact_doc.as_bytes()).unwrap()).to_string();
        let item = edit(waiver_item(
            "waiver/one",
            &key,
            &fact,
            "team:release-engineering",
        ));
        WaiverBundle::parse(waiver_bundle(&[item]).as_bytes())
    };

    for (spelling, expected) in [("blob", TargetKind::Blob), ("tree", TargetKind::Tree)] {
        let edit = |doc: String| {
            doc.replace(
                "\"target_kind\": \"either\"",
                &format!("\"target_kind\": \"{spelling}\""),
            )
        };
        let bundle = bundle_for(&edit).unwrap_or_else(|e| panic!("{spelling}: {e:?}"));
        let kind = bundle.items.first().map(|item| {
            item.authorized_fact
                .key_input()
                .scope
                .normalized_target_intent
                .target_kind
        });
        assert_eq!(kind, Some(expected), "{spelling}");
    }

    let sha256 =
        |doc: String| {
            doc.replace(
            r#""object_format": "sha1", "tree_oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb""#,
            &format!(r#""object_format": "sha256", "tree_oid": "{}""#, "b".repeat(64)),
        )
        };
    assert!(
        bundle_for(&sha256).is_ok(),
        "a sha256 candidate tree is a tree identity"
    );

    let digest = "sha256:3333333333333333333333333333333333333333333333333333333333333333";
    let carried = |doc: String| {
        doc.replace(
            r#""query_digest": null"#,
            &format!(r#""query_digest": "{digest}""#),
        )
    };
    let bundle = bundle_for(&carried).expect("a query digest is carried");
    let got = bundle.items.first().and_then(|item| {
        item.authorized_fact
            .key_input()
            .scope
            .normalized_target_intent
            .query_digest
    });
    assert!(got.is_some(), "a present digest is not dropped");

    let blank = |doc: String| doc.replace("Release window exception.", "   ");
    assert!(bundle_for(&blank).is_err(), "whitespace is not a reason");
    let empty = |doc: String| doc.replace("Release window exception.", "");
    assert!(
        bundle_for(&empty).is_err(),
        "an empty reason is not a reason"
    );
    let unknown = |doc: String| {
        doc.replace(
            r#""residual_disposition": "warn""#,
            r#""residual_disposition": "quietly""#,
        )
    };
    assert!(
        bundle_for(&unknown).is_err(),
        "only the two dispositions decode"
    );
}

#[test]
fn parses_a_valid_waiver_bundle_and_rejects_duplicates() {
    let (key, fact) = computed_digests();

    let item = waiver_item("waiver/one", &key, &fact, "team:release-engineering");
    let doc = waiver_bundle(&[item]);
    let bundle = WaiverBundle::parse(doc.as_bytes()).unwrap();
    assert_eq!(bundle.schema(), "amiss/waiver-bundle");
    assert_eq!(bundle.items.len(), 1);

    let same_owner = waiver_item("waiver/one", &key, &fact, "team:docs-platform");
    let doc = waiver_bundle(&[same_owner]);
    assert!(
        WaiverBundle::parse(doc.as_bytes()).is_ok(),
        "owner==issuer is a selected-item defect, not a parse defect"
    );

    let first = waiver_item("waiver/one", &key, &fact, "team:release-engineering");
    let second = waiver_item("waiver/two", &key, &fact, "team:release-engineering");
    let doc = waiver_bundle(&[first, second]);
    assert_eq!(
        WaiverBundle::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::DuplicateMember,
        "duplicate (candidate_tree, finding_key) pair"
    );

    let bad_window = waiver_item("waiver/one", &key, &fact, "team:release-engineering").replace(
        "\"not_before\": \"2026-07-02T00:00:00Z\"",
        "\"not_before\": \"2026-09-01T00:00:00Z\"",
    );
    let doc = waiver_bundle(&[bad_window]);
    assert_eq!(
        WaiverBundle::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    let bad_residual = waiver_item("waiver/one", &key, &fact, "team:release-engineering").replace(
        "\"residual_disposition\": \"warn\"",
        "\"residual_disposition\": \"record\"",
    );
    let doc = waiver_bundle(&[bad_residual]);
    assert_eq!(
        WaiverBundle::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::InvalidValue
    );
}
