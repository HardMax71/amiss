use amiss_wire::controls::{
    EntryKind, FACT_DOMAIN, FINDING_KEY_DOMAIN, FindingScope, IncludeKind, PromotableFindingKind,
    SourceConstruct, TargetKind, WaiverBundleSchema, parse_waiver_bundle,
};
use amiss_wire::de::ErrorKind;

use amiss_wire::json;
use strum::IntoEnumIterator;

use crate::support::{
    DEFAULT_CONSTRUCT, computed_digests, fact_json, key_input_json, waiver_bundle, waiver_item,
};

#[expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "roundtrip helper on known-valid templates"
)]
fn roundtrip_scope(context: &str, edit: &dyn Fn(String) -> String) -> FindingScope {
    let key_input = edit(key_input_json("explicit-target-missing"));
    let key = amiss_wire::digest::hb(
        FINDING_KEY_DOMAIN,
        &serde_json_canonicalizer::to_vec(&json::parse(key_input.as_bytes()).unwrap()).unwrap(),
    )
    .to_string();
    let fact_doc = edit(fact_json());
    let fact = amiss_wire::digest::hb(
        FACT_DOMAIN,
        &serde_json_canonicalizer::to_vec(&json::parse(fact_doc.as_bytes()).unwrap()).unwrap(),
    )
    .to_string();
    let item = edit(waiver_item(
        "waiver/one",
        &key,
        &fact,
        "team:release-engineering",
    ));
    let bundle = parse_waiver_bundle(waiver_bundle(&[item]).as_bytes())
        .unwrap_or_else(|error| panic!("{context} is a scope the bundle accepts: {error:?}"));
    bundle
        .items
        .first()
        .unwrap()
        .authorized_fact
        .key_input
        .scope
        .clone()
}

#[test]
fn every_scope_spelling_survives_the_waiver_round_trip() {
    for construct in SourceConstruct::iter() {
        let named: &'static str = construct.into();
        let scope = roundtrip_scope(named, &|doc| doc.replace(DEFAULT_CONSTRUCT, named));
        assert_eq!(scope.source_construct, construct, "{named}");
        assert_eq!(construct.is_image(), named.contains("image"), "{named}");
    }
    for target_kind in TargetKind::iter() {
        let named: &'static str = target_kind.into();
        let scope = roundtrip_scope(named, &|doc| {
            doc.replace(
                r#""target_kind": "either""#,
                &format!(r#""target_kind": "{named}""#),
            )
        });
        assert_eq!(
            scope.normalized_target_intent.target_kind, target_kind,
            "{named}"
        );
    }
}

#[test]
fn waiver_instants_bind_at_their_exact_boundaries() {
    let (key, fact) = computed_digests();
    let item = |created: &str, not_before: &str| {
        waiver_item("waiver/one", &key, &fact, "team:release-engineering")
            .replace("2026-07-01T00:00:00Z", created)
            .replace("2026-07-02T00:00:00Z", not_before)
    };

    let at_activation = waiver_bundle(&[item("2026-07-02T00:00:00Z", "2026-07-02T00:00:00Z")]);
    parse_waiver_bundle(at_activation.as_bytes())
        .expect("a waiver active from its creation instant is consistent");

    let at_bundle_instant = waiver_bundle(&[item("2026-07-03T00:00:00Z", "2026-07-03T00:00:00Z")]);
    parse_waiver_bundle(at_bundle_instant.as_bytes())
        .expect("an item created at the bundle instant is not from the future");

    let backdated = waiver_bundle(&[item("2026-07-02T00:00:01Z", "2026-07-02T00:00:00Z")]);
    assert_eq!(
        parse_waiver_bundle(backdated.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    let from_the_future = waiver_bundle(&[item("2026-07-04T00:00:00Z", "2026-07-05T00:00:00Z")]);
    assert_eq!(
        parse_waiver_bundle(from_the_future.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::Inconsistent
    );
}

#[test]
fn wire_spellings_are_the_ones_the_contract_publishes() {
    assert_eq!(
        [
            IncludeKind::Document.as_ref(),
            IncludeKind::Tree.as_ref(),
            EntryKind::Blob.as_ref(),
            EntryKind::Gitlink.as_ref(),
            PromotableFindingKind::InvalidReference.as_ref(),
        ],
        ["document", "tree", "blob", "gitlink", "invalid-reference"]
    );
}

#[test]
fn a_waiver_answers_for_every_spelling_its_scope_may_carry() {
    let bundle_for = |edit: &dyn Fn(String) -> String| {
        let key_input = edit(key_input_json("explicit-target-missing"));
        let key = amiss_wire::digest::hb(
            FINDING_KEY_DOMAIN,
            &serde_json_canonicalizer::to_vec(&json::parse(key_input.as_bytes()).unwrap()).unwrap(),
        )
        .to_string();
        let fact_doc = edit(fact_json());
        let fact = amiss_wire::digest::hb(
            FACT_DOMAIN,
            &serde_json_canonicalizer::to_vec(&json::parse(fact_doc.as_bytes()).unwrap()).unwrap(),
        )
        .to_string();
        let item = edit(waiver_item(
            "waiver/one",
            &key,
            &fact,
            "team:release-engineering",
        ));
        parse_waiver_bundle(waiver_bundle(&[item]).as_bytes())
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
                .key_input
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
            .key_input
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
    let bundle = parse_waiver_bundle(doc.as_bytes()).unwrap();
    assert_eq!(bundle.schema, WaiverBundleSchema::Current);
    assert_eq!(bundle.items.len(), 1);

    let same_owner = waiver_item("waiver/one", &key, &fact, "team:docs-platform");
    let doc = waiver_bundle(&[same_owner]);
    assert!(
        parse_waiver_bundle(doc.as_bytes()).is_ok(),
        "owner==issuer is a selected-item defect, not a parse defect"
    );

    let first = waiver_item("waiver/one", &key, &fact, "team:release-engineering");
    let second = waiver_item("waiver/two", &key, &fact, "team:release-engineering");
    let doc = waiver_bundle(&[first, second]);
    assert_eq!(
        parse_waiver_bundle(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::DuplicateMember,
        "duplicate (candidate_tree, finding_key) pair"
    );

    let bad_window = waiver_item("waiver/one", &key, &fact, "team:release-engineering").replace(
        "\"not_before\": \"2026-07-02T00:00:00Z\"",
        "\"not_before\": \"2026-09-01T00:00:00Z\"",
    );
    let doc = waiver_bundle(&[bad_window]);
    assert_eq!(
        parse_waiver_bundle(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    let bad_residual = waiver_item("waiver/one", &key, &fact, "team:release-engineering").replace(
        "\"residual_disposition\": \"warn\"",
        "\"residual_disposition\": \"record\"",
    );
    let doc = waiver_bundle(&[bad_residual]);
    assert_eq!(
        parse_waiver_bundle(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::InvalidValue
    );
}
