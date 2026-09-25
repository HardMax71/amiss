use amiss_wire::controls::ScannerPolicy;
use amiss_wire::controls::{
    ANCHOR_RENDERERS, BlobLineSelection, DEFAULT_BRANCH_ALIASES, DOCUMENT_SUFFIX_BYTES,
    ProjectionKind, ProjectionSource, SOURCE_MARKER_BYTES, TRANSLATION_PAIRS,
    check_projection_source,
};
use amiss_wire::de::Document as _;
use amiss_wire::de::ErrorKind;
use amiss_wire::envelope::document_digest;
use amiss_wire::repo_path_text;
use sha2::Digest as _;

use super::support::POLICY;

#[test]
fn parses_the_policy_fixture() {
    let policy = ScannerPolicy::parse(POLICY).unwrap();
    assert_eq!(policy.document_includes.len(), 2);
    assert_eq!(
        policy
            .projection_assertions
            .as_deref()
            .unwrap_or_default()
            .len(),
        2
    );
    assert_eq!(policy.protected_inventory.len(), 2);
    assert_eq!(policy.finding_dispositions.len(), 1);
    assert_eq!(policy.document_includes[0].suffix.as_deref(), Some(".txt"));
    let assertions = policy.projection_assertions.as_deref().unwrap_or_default();
    let assertion = &assertions[0];
    assert_eq!(assertion.document.as_str(), "docs/architecture.md");
    assert_eq!(assertion.name, "request-shape");
    assert_eq!(assertion.projection, ProjectionKind::CodeTextV1);
    let ProjectionSource::BlobLines(source) = &assertion.source else {
        panic!("fixture assertion uses blob lines");
    };
    assert_eq!(source.path.as_str(), "crates/amiss/src/request.rs");
    assert_eq!((source.first_line, source.last_line), (10, 14));
    let ProjectionSource::NamedRegion(source) = &assertions[1].source else {
        panic!("fixture second assertion uses a named region");
    };
    assert_eq!(source.path.as_str(), "examples/generated.txt");
    assert_eq!(source.start_marker, "// amiss:generated:start");
    assert_eq!(source.end_marker, "// amiss:generated:end");
    let digest = document_digest("amiss/scanner-policy", &policy).unwrap();
    let reparsed = ScannerPolicy::parse(POLICY).unwrap();
    assert_eq!(
        digest,
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/scanner-policy")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&reparsed).unwrap())
                .finalize()
                .0
        )
    );
}

#[test]
fn directly_constructed_projection_sources_reuse_the_policy_grammar() {
    let source = ProjectionSource::BlobLines(BlobLineSelection {
        path: repo_path_text!("src/lib.rs"),
        first_line: 0,
        last_line: 1,
    });
    assert_eq!(
        check_projection_source(ProjectionKind::CodeTextV1, &source)
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );

    let policy = ScannerPolicy::parse(POLICY).unwrap();
    let source = &policy.projection_assertions.as_deref().unwrap_or_default()[0].source;
    assert!(check_projection_source(ProjectionKind::CodeTextV1, source).is_ok());
    assert_eq!(
        serde_json::from_slice::<ProjectionSource>(
            br#"{"kind":"blob-lines","path":"crates/amiss/src/request.rs","first_line":10,"last_line":14}"#,
        )
        .unwrap(),
        *source
    );
    assert_eq!(
        check_projection_source(ProjectionKind::SortedRowsV1, source)
            .unwrap_err()
            .kind,
        ErrorKind::Inconsistent
    );
}

fn policy_with_assertions(assertions: &str) -> String {
    format!(
        r#"{{"schema":"amiss/scanner-policy","document_includes":[],"projection_assertions":[{assertions}],"protected_inventory":[],"finding_dispositions":[]}}"#
    )
}

#[test]
fn projection_assertions_have_one_closed_sorted_grammar() {
    let row = |document: &str, name: &str, first: u64, last: u64| {
        format!(
            r#"{{"document":"{document}","name":"{name}","projection":"code-text-v1","sink":"previous-code","source":{{"kind":"blob-lines","path":"src/lib.rs","first_line":{first},"last_line":{last}}}}}"#
        )
    };
    let valid = policy_with_assertions(&row("docs/a.md", "example", 1, 9_007_199_254_740_991));
    assert!(ScannerPolicy::parse(valid.as_bytes()).is_ok());

    let unsorted = policy_with_assertions(&format!(
        "{},{}",
        row("docs/b.md", "example", 1, 1),
        row("docs/a.md", "example", 1, 1)
    ));
    assert_eq!(
        ScannerPolicy::parse(unsorted.as_bytes()).unwrap_err().kind,
        ErrorKind::UnsortedSet
    );

    let duplicate = policy_with_assertions(&format!(
        "{},{}",
        row("docs/a.md", "example", 1, 1),
        row("docs/a.md", "example", 2, 2)
    ));
    assert_eq!(
        ScannerPolicy::parse(duplicate.as_bytes()).unwrap_err().kind,
        ErrorKind::DuplicateMember,
        "a selector change does not mint another assertion identity"
    );

    let reversed = policy_with_assertions(&row("docs/a.md", "example", 2, 1));
    assert_eq!(
        ScannerPolicy::parse(reversed.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    let named = policy_with_assertions(
        r#"{"document":"docs/a.md","name":"example","projection":"code-text-v1","sink":"previous-code","source":{"kind":"named-region","path":"src/lib.rs","start_marker":"// amiss:start","end_marker":"// amiss:end"}}"#,
    );
    let parsed = ScannerPolicy::parse(named.as_bytes()).unwrap();
    let ProjectionSource::NamedRegion(source) =
        &parsed.projection_assertions.as_deref().unwrap_or_default()[0].source
    else {
        panic!("named-region source survives the policy reader");
    };
    assert_eq!(source.path.as_str(), "src/lib.rs");
    assert_eq!(source.start_marker, "// amiss:start");
    assert_eq!(source.end_marker, "// amiss:end");

    let tree = policy_with_assertions(
        r#"{"document":"docs/a.md","name":"example","projection":"sorted-rows-v1","sink":"previous-code","source":{"kind":"tree-paths","root":"crates","suffix":".rs","maximum_depth":3}}"#,
    );
    let parsed = ScannerPolicy::parse(tree.as_bytes()).unwrap();
    assert_eq!(
        parsed.projection_assertions.as_deref().unwrap_or_default()[0].projection,
        ProjectionKind::SortedRowsV1
    );
    let ProjectionSource::TreePaths(source) =
        &parsed.projection_assertions.as_deref().unwrap_or_default()[0].source
    else {
        panic!("tree-paths source survives the policy reader");
    };
    assert_eq!(source.root.as_str(), "crates");
    assert_eq!(source.suffix.as_deref(), Some(".rs"));
    assert_eq!(source.maximum_depth, 3);

    let count = tree.replace("sorted-rows-v1", "decimal-count-v1");
    let parsed = ScannerPolicy::parse(count.as_bytes()).unwrap();
    assert_eq!(
        parsed.projection_assertions.as_deref().unwrap_or_default()[0].projection,
        ProjectionKind::DecimalCountV1
    );
    assert!(matches!(
        parsed.projection_assertions.as_deref().unwrap_or_default()[0].source,
        ProjectionSource::TreePaths(_)
    ));

    let record = policy_with_assertions(
        r#"{"document":"docs/a.md","name":"example","projection":"code-text-v1","sink":"previous-code","source":{"kind":"record-value","set":"rust/public-api","key":"amiss::check"}}"#,
    );
    let parsed = ScannerPolicy::parse(record.as_bytes()).unwrap();
    let ProjectionSource::RecordValue(source) =
        &parsed.projection_assertions.as_deref().unwrap_or_default()[0].source
    else {
        panic!("record-value source survives the policy reader");
    };
    assert_eq!(source.set.as_str(), "rust/public-api");
    assert_eq!(source.key, "amiss::check");

    let records = policy_with_assertions(
        r#"{"document":"docs/a.md","name":"example","projection":"sorted-rows-v1","sink":"previous-code","source":{"kind":"record-set","set":"rust/public-api"}}"#,
    );
    let parsed = ScannerPolicy::parse(records.as_bytes()).unwrap();
    let ProjectionSource::RecordSet(source) =
        &parsed.projection_assertions.as_deref().unwrap_or_default()[0].source
    else {
        panic!("record-set source survives the policy reader");
    };
    assert_eq!(source.set.as_str(), "rust/public-api");
    let count = records.replace("sorted-rows-v1", "decimal-count-v1");
    assert!(ScannerPolicy::parse(count.as_bytes()).is_ok());
}

#[test]
fn projection_assertions_refuse_unknown_or_unsafe_words() {
    let valid = r#"{"document":"docs/a.md","name":"example","projection":"code-text-v1","sink":"previous-code","source":{"kind":"blob-lines","path":"src/lib.rs","first_line":1,"last_line":1}}"#;
    for invalid in [
        valid.replace("\"name\":\"example\"", "\"name\":\"-example\""),
        valid.replace("code-text-v1", "code-text-v2"),
        valid.replace("previous-code", "next-code"),
        valid.replace("blob-lines", "blob-region"),
        valid.replace("\"first_line\":1", "\"first_line\":0"),
    ] {
        let policy = policy_with_assertions(&invalid);
        assert_eq!(
            ScannerPolicy::parse(policy.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "invalid row: {invalid}"
        );
    }

    let tree = r#"{"document":"docs/a.md","name":"example","projection":"sorted-rows-v1","sink":"previous-code","source":{"kind":"tree-paths","root":"crates","suffix":".rs","maximum_depth":2}}"#;
    for invalid in [
        tree.replace("sorted-rows-v1", "code-text-v1"),
        tree.replace("\"maximum_depth\":2", "\"maximum_depth\":0"),
        tree.replace("\"suffix\":\".rs\",", "\"suffix\":\"rs\","),
        valid.replace("code-text-v1", "sorted-rows-v1"),
        valid.replace("code-text-v1", "decimal-count-v1"),
        r#"{"document":"docs/a.md","name":"example","projection":"sorted-rows-v1","sink":"previous-code","source":{"kind":"record-value","set":"rust/public-api","key":"amiss::check"}}"#.to_owned(),
        r#"{"document":"docs/a.md","name":"example","projection":"code-text-v1","sink":"previous-code","source":{"kind":"record-value","set":"Rust","key":"amiss::check"}}"#.to_owned(),
        r#"{"document":"docs/a.md","name":"example","projection":"code-text-v1","sink":"previous-code","source":{"kind":"record-value","set":"rust","key":"line\nbreak"}}"#.to_owned(),
        r#"{"document":"docs/a.md","name":"example","projection":"code-text-v1","sink":"previous-code","source":{"kind":"record-set","set":"rust/public-api"}}"#.to_owned(),
    ] {
        assert!(
            ScannerPolicy::parse(policy_with_assertions(&invalid).as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let unsafe_integer =
        policy_with_assertions(&valid.replace("\"last_line\":1", "\"last_line\":9007199254740992"));
    assert!(matches!(
        ScannerPolicy::parse(unsafe_integer.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    ));

    let named = |start: &str, end: &str| {
        policy_with_assertions(&format!(
            r#"{{"document":"docs/a.md","name":"example","projection":"code-text-v1","sink":"previous-code","source":{{"kind":"named-region","path":"src/lib.rs","start_marker":{start},"end_marker":{end}}}}}"#
        ))
    };
    for invalid in [
        named(r#"""#, r#""end""#),
        named(r#""   ""#, r#""end""#),
        named(r#""\tstart""#, r#""end""#),
        named(r#""same""#, r#""same""#),
        named(
            &format!("\"{}\"", "x".repeat(SOURCE_MARKER_BYTES.saturating_add(1))),
            r#""end""#,
        ),
    ] {
        assert!(
            ScannerPolicy::parse(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn rejects_policy_shape_defects() {
    let unknown = br#"{
      "schema": "amiss/scanner-policy",
      "document_includes": [],
      "protected_inventory": [],
      "finding_dispositions": [],
      "extra": 1
    }"#;
    assert_eq!(
        ScannerPolicy::parse(unknown).unwrap_err().kind,
        ErrorKind::UnknownField
    );

    let wrong_schema = br#"{
      "schema": "assure/scanner-policy",
      "document_includes": [],
      "protected_inventory": [],
      "finding_dispositions": []
    }"#;
    assert_eq!(
        ScannerPolicy::parse(wrong_schema).unwrap_err().kind,
        ErrorKind::InvalidValue
    );

    let unsorted = br#"{
      "schema": "amiss/scanner-policy",
      "document_includes": [],
      "protected_inventory": ["b.md", "a.md"],
      "finding_dispositions": []
    }"#;
    assert_eq!(
        ScannerPolicy::parse(unsorted).unwrap_err().kind,
        ErrorKind::UnsortedSet
    );

    for bad_path in ["/abs.md", "a//b.md", "a/../b.md", "a\\\\b.md", "a/./b.md"] {
        let doc = format!(
            r#"{{
              "schema": "amiss/scanner-policy",
              "document_includes": [],
              "protected_inventory": ["{bad_path}"],
              "finding_dispositions": []
            }}"#
        );
        assert_eq!(
            ScannerPolicy::parse(doc.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "path {bad_path}"
        );
    }
}

#[test]
fn optional_projection_assertions_distinguish_absence_from_empty() {
    let absent = br#"{"schema":"amiss/scanner-policy","document_includes":[],"protected_inventory":[],"finding_dispositions":[]}"#;
    let present = br#"{"schema":"amiss/scanner-policy","document_includes":[],"projection_assertions":[],"protected_inventory":[],"finding_dispositions":[]}"#;
    let null = br#"{"schema":"amiss/scanner-policy","document_includes":[],"projection_assertions":null,"protected_inventory":[],"finding_dispositions":[]}"#;

    let absent_policy = ScannerPolicy::parse(absent).unwrap();
    let present_policy = ScannerPolicy::parse(present).unwrap();
    assert_eq!(absent_policy.projection_assertions, None);
    assert_eq!(present_policy.projection_assertions, Some(Vec::new()));
    assert_eq!(
        serde_json_canonicalizer::to_vec(&absent_policy).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(absent).unwrap()
        )
        .unwrap()
    );
    assert_eq!(
        serde_json_canonicalizer::to_vec(&present_policy).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(present).unwrap()
        )
        .unwrap()
    );
    assert_ne!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/scanner-policy")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&absent_policy).unwrap())
                .finalize()
                .0
        ),
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/scanner-policy")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&present_policy).unwrap())
                .finalize()
                .0
        )
    );
    assert_eq!(ScannerPolicy::parse(null).unwrap(), absent_policy);
}

#[test]
fn policy_rechecks_mutable_public_fields() {
    let mut policy = ScannerPolicy::parse(POLICY).unwrap();
    policy.document_includes.swap(0, 1);
    assert_eq!(policy.validate().unwrap_err().kind, ErrorKind::UnsortedSet);
}

/// An include's optional adapter is a closed spelling: each wire id parses to
/// its adapter, absence stays unbound, and anything else refuses.
#[test]
fn an_include_binding_is_a_closed_adapter_spelling() {
    let bound = r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"tree","path":"manual"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    let policy = ScannerPolicy::parse(bound.as_bytes()).unwrap();
    assert_eq!(
        policy.document_includes[0].adapter,
        Some(amiss_wire::model::Adapter::Rst)
    );

    let unbound = r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"manual"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    let policy = ScannerPolicy::parse(unbound.as_bytes()).unwrap();
    assert_eq!(policy.document_includes[0].adapter, None);

    for bad in ["latex", "Rst", "restructuredtext", ""] {
        let doc = format!(
            r#"{{"schema":"amiss/scanner-policy","document_includes":[{{"adapter":"{bad}","kind":"tree","path":"manual"}}],"protected_inventory":[],"finding_dispositions":[]}}"#
        );
        assert_eq!(
            ScannerPolicy::parse(doc.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "adapter {bad}"
        );
    }
}

#[test]
fn a_tree_suffix_is_one_bounded_exact_selector() {
    let selected = r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"tree","path":"manual","suffix":".txt"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    let policy = ScannerPolicy::parse(selected.as_bytes()).unwrap();
    assert_eq!(policy.document_includes[0].suffix.as_deref(), Some(".txt"));

    let longest = format!(".{}", "x".repeat(DOCUMENT_SUFFIX_BYTES.saturating_sub(1)));
    let at_limit = format!(
        r#"{{"schema":"amiss/scanner-policy","document_includes":[{{"kind":"tree","path":"manual","suffix":"{longest}"}}],"protected_inventory":[],"finding_dispositions":[]}}"#
    );
    assert!(ScannerPolicy::parse(at_limit.as_bytes()).is_ok());

    for suffix in ["", ".", "txt", ".a/b"] {
        let invalid = format!(
            r#"{{"schema":"amiss/scanner-policy","document_includes":[{{"kind":"tree","path":"manual","suffix":"{suffix}"}}],"protected_inventory":[],"finding_dispositions":[]}}"#
        );
        assert_eq!(
            ScannerPolicy::parse(invalid.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "suffix {suffix:?}"
        );
    }

    for invalid in [
        r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"manual","suffix":".a\\b"}],"protected_inventory":[],"finding_dispositions":[]}"#,
        r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"manual","suffix":".a\u0000b"}],"protected_inventory":[],"finding_dispositions":[]}"#,
    ] {
        assert_eq!(
            ScannerPolicy::parse(invalid.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue
        );
    }

    let too_long = format!(".{}", "x".repeat(DOCUMENT_SUFFIX_BYTES));
    let multibyte_too_long = format!(".{}", "é".repeat(DOCUMENT_SUFFIX_BYTES / 2));
    for suffix in [too_long, multibyte_too_long] {
        let invalid = format!(
            r#"{{"schema":"amiss/scanner-policy","document_includes":[{{"kind":"tree","path":"manual","suffix":"{suffix}"}}],"protected_inventory":[],"finding_dispositions":[]}}"#
        );
        assert_eq!(
            ScannerPolicy::parse(invalid.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "the UTF-8 encoding crosses the byte ceiling"
        );
    }

    let document = r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"document","path":"manual.txt","suffix":".txt"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    assert_eq!(
        ScannerPolicy::parse(document.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    let duplicate = r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"manual","suffix":".rst"},{"kind":"tree","path":"manual","suffix":".txt"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    assert_eq!(
        ScannerPolicy::parse(duplicate.as_bytes()).unwrap_err().kind,
        ErrorKind::DuplicateMember,
        "suffix does not mint a second selector identity at one root"
    );
}

/// The old names a policy declares for its default branch are full branch
/// refs, and a renderer pin names lowercase rules, which the engine checks
/// against its own table. Both are sorted, unique and bounded sets, the pin is
/// never empty, and a policy without either key reads the way it always did.
#[test]
fn the_alias_and_renderer_sets_are_bounded_and_sorted() {
    let kind = |key: &str, members: &str| {
        let text = format!(
            r#"{{"schema":"amiss/scanner-policy","document_includes":[],"protected_inventory":[],"finding_dispositions":[],"{key}":[{members}]}}"#
        );
        ScannerPolicy::parse(text.as_bytes())
            .err()
            .map(|defect| defect.kind)
    };
    let oversized = |limit: usize, spell: fn(usize) -> String| {
        (0..=limit).map(spell).collect::<Vec<_>>().join(",")
    };
    let aliases = "default_branch_aliases";
    assert_eq!(
        kind(aliases, r#""refs/heads/master","refs/heads/trunk""#),
        None
    );
    assert_eq!(
        kind(aliases, r#""refs/heads/trunk","refs/heads/master""#),
        Some(ErrorKind::UnsortedSet)
    );
    assert_eq!(
        kind(aliases, r#""refs/heads/master","refs/heads/master""#),
        Some(ErrorKind::DuplicateMember)
    );
    assert!(
        kind(aliases, r#""master""#).is_some(),
        "an alias is a full ref"
    );
    assert_eq!(
        kind(
            aliases,
            &oversized(DEFAULT_BRANCH_ALIASES, |index| format!(
                r#""refs/heads/old-{index:02}""#
            ))
        ),
        Some(ErrorKind::LimitExceeded)
    );
    let renderers = "anchor_renderers";
    assert_eq!(kind(renderers, r#""github","mdit-vue""#), None);
    assert_eq!(
        kind(renderers, r#""mdit-vue","github""#),
        Some(ErrorKind::UnsortedSet)
    );
    assert_eq!(kind(renderers, ""), Some(ErrorKind::LimitExceeded));
    assert_eq!(
        kind(renderers, r#""GitHub""#),
        Some(ErrorKind::InvalidValue)
    );
    assert_eq!(
        kind(
            renderers,
            &oversized(ANCHOR_RENDERERS, |index| format!(r#""rule-{index:02}""#))
        ),
        Some(ErrorKind::LimitExceeded)
    );
}

/// Translation pairs are a sorted, unique, bounded set, and a tree cannot be
/// its own translation.
#[test]
fn translation_pairs_are_a_bounded_sorted_set_of_distinct_trees() {
    let kind = |pairs: &str| {
        let text = format!(
            r#"{{"schema":"amiss/scanner-policy","document_includes":[],"protected_inventory":[],"finding_dispositions":[],"translations":[{pairs}]}}"#
        );
        ScannerPolicy::parse(text.as_bytes())
            .err()
            .map(|defect| defect.kind)
    };
    let pair =
        |source: &str, target: &str| format!(r#"{{"source":"{source}","target":"{target}"}}"#);
    assert_eq!(
        kind(&format!(
            "{},{}",
            pair("docs", "docs/de"),
            pair("docs", "docs/fr")
        )),
        None
    );
    assert_eq!(
        kind(&format!(
            "{},{}",
            pair("docs", "docs/fr"),
            pair("docs", "docs/de")
        )),
        Some(ErrorKind::UnsortedSet)
    );
    assert_eq!(
        kind(&format!(
            "{},{}",
            pair("docs", "docs/de"),
            pair("docs", "docs/de")
        )),
        Some(ErrorKind::DuplicateMember)
    );
    assert_eq!(kind(&pair("docs", "docs")), Some(ErrorKind::Inconsistent));
    let many: Vec<String> = (0..=TRANSLATION_PAIRS)
        .map(|index| pair("docs", &format!("i18n/{index:03}")))
        .collect();
    assert_eq!(kind(&many.join(",")), Some(ErrorKind::LimitExceeded));
}

/// A whole-file source names only a path, a key source names a nonempty
/// path of keys to one scalar, and containment reads text, so neither pairs
/// with a row or count projection and `contains-v1` pairs with no inventory.
#[test]
fn blob_and_key_sources_pair_only_with_text_projections() {
    let kind = |projection: &str, source: &str| {
        let text = policy_with_assertions(&format!(
            r#"{{"document":"docs/a.md","name":"sample","projection":"{projection}","sink":"previous-code","source":{source}}}"#
        ));
        ScannerPolicy::parse(text.as_bytes())
            .err()
            .map(|defect| defect.kind)
    };
    let blob = r#"{"kind":"blob","path":"help.txt"}"#;
    let key =
        r#"{"kind":"key-value","path":"Cargo.toml","format":"toml","key":["package","version"]}"#;
    let tree = r#"{"kind":"tree-paths","root":"docs","maximum_depth":1}"#;
    assert_eq!(kind("code-text-v1", blob), None);
    assert_eq!(kind("contains-v1", key), None);
    assert_eq!(kind("sorted-rows-v1", blob), Some(ErrorKind::Inconsistent));
    assert_eq!(kind("contains-v1", tree), Some(ErrorKind::Inconsistent));
    assert_eq!(
        kind(
            "code-text-v1",
            r#"{"kind":"key-value","path":"a.json","format":"json","key":[]}"#
        ),
        Some(ErrorKind::InvalidValue)
    );
    assert!(
        kind(
            "code-text-v1",
            r#"{"kind":"key-value","path":"a.yaml","format":"yaml","key":["a"]}"#
        )
        .is_some(),
        "the formats are closed"
    );
}
