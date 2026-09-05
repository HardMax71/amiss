#![expect(
    clippy::unwrap_used,
    reason = "the fixture constructs known-valid renderer contexts and output trees"
)]

use std::fs;

use amiss_controller::{
    MDBOOK_HTML_BYTES, MDBOOK_RENDER_CONTEXT_BYTES, MdBookEvidenceError, SiteBuildContext,
    mdbook_site_evidence, mdbook_site_expectation,
};
use amiss_wire::digest::hb;
use amiss_wire::model::RepoPathText;
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use serde_json::json;

fn chapter(
    path: Option<&str>,
    source_path: Option<&str>,
    sub_items: &[serde_json::Value],
) -> serde_json::Value {
    json!({
        "Chapter": {
            "content": "ignored by the evidence projection",
            "name": "fixture",
            "number": null,
            "parent_names": [],
            "path": path,
            "source_path": source_path,
            "sub_items": sub_items
        }
    })
}

fn context(version: &str, html_renderer: bool, items: &[serde_json::Value]) -> Vec<u8> {
    let output = if html_renderer {
        json!({"html": {}})
    } else {
        json!({"capture": {}})
    };
    serde_json::to_vec(&json!({
        "book": {"items": items},
        "config": {"book": {"src": "guide"}, "output": output},
        "destination": "/operator/build/capture",
        "root": "/operator/checkout/docs",
        "version": version
    }))
    .unwrap()
}

fn output(root: &tempfile::TempDir) -> Dir {
    Dir::open_ambient_dir(root.path(), ambient_authority()).unwrap()
}

fn site(configuration: &str, route_prefix: &str) -> SiteBuildContext {
    SiteBuildContext {
        configuration: RepoPathText::new(configuration.to_owned()).unwrap(),
        route_prefix: route_prefix.to_owned(),
        locale: None,
        version: None,
    }
}

fn observation<'a>(observations: &'a [serde_json::Value], route: &str) -> &'a serde_json::Value {
    observations
        .iter()
        .find(|row| row.get("route").and_then(serde_json::Value::as_str) == Some(route))
        .unwrap()
}

fn texts<'a>(observation: &'a serde_json::Value, name: &str) -> Vec<&'a str> {
    let Some(values) = observation.get(name).and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect()
}

#[test]
fn postprocessed_pages_become_exact_source_bound_routes_and_anchors() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(
        root.path().join("intro.html"),
        br#"<!doctype html><h1 id="intro"></h1><span id="entity&amp;anchor"></span><i id="intro"></i><a name="legacy"></a><a id="both" name="named"></a><a name=""></a><area name="map">"#,
    )
    .unwrap();
    fs::write(
        root.path().join("index.html"),
        br#"<!doctype html><h1 id="home"></h1><a href="nested/%C3%BCber%20view.html">next</a>"#,
    )
    .unwrap();
    fs::write(
        root.path().join("nested/über view.html"),
        r#"<!doctype html><h2 id="über-view"></h2>"#,
    )
    .unwrap();
    let nested = chapter(Some("nested/über view.md"), Some("nested/chapter.md"), &[]);
    let context = context(
        "0.5.4",
        true,
        &[
            chapter(Some("intro.md"), Some("README.md"), &[nested]),
            json!("Separator"),
        ],
    );
    let candidate = hb("amiss/test-mdbook-candidate", b"candidate");
    let site = site("docs/book.toml", "/manual/");

    let evidence = mdbook_site_evidence(candidate, &site, &context, &output(&root)).unwrap();
    let parsed = amiss_wire::semantic::parse(&evidence).unwrap();

    assert_eq!(parsed.payload.subject.candidate_identity_digest, candidate);
    assert_eq!(parsed.payload.producer.kind.as_str(), "site-build");
    assert_eq!(parsed.payload.producer.version, "0.5.1");
    assert_eq!(
        parsed.payload.producer.context_digest,
        mdbook_site_expectation(&site).unwrap().context_digest
    );
    assert!(parsed.payload.complete);
    assert_eq!(parsed.payload.observations.len(), 4);
    let intro = observation(&parsed.payload.observations, "/manual/intro.html");
    assert_eq!(intro["source"], "docs/guide/README.md");
    assert_eq!(
        texts(intro, "anchors"),
        ["both", "entity&anchor", "intro", "legacy", "named"]
    );
    let index = observation(&parsed.payload.observations, "/manual/index.html");
    assert_eq!(index["source"], "docs/guide/README.md");
    assert_eq!(texts(index, "anchors"), ["home"]);
    let nested = observation(
        &parsed.payload.observations,
        "/manual/nested/%C3%BCber%20view.html",
    );
    assert_eq!(nested["source"], "docs/guide/nested/chapter.md");
    assert_eq!(texts(nested, "anchors"), ["über-view"]);
    let navigation = parsed
        .payload
        .observations
        .iter()
        .find(|row| row.get("kind").and_then(serde_json::Value::as_str) == Some("site-navigation"))
        .unwrap();
    assert_eq!(navigation["root"], "docs/guide");
    assert_eq!(navigation["manifest"], "docs/guide/SUMMARY.md");
    assert_eq!(texts(navigation, "entrypoints"), ["/manual/index.html"]);
    assert_eq!(
        texts(navigation, "reachable"),
        ["docs/guide/README.md", "docs/guide/nested/chapter.md"]
    );

    let repeated = mdbook_site_evidence(candidate, &site, &context, &output(&root)).unwrap();
    assert_eq!(evidence, repeated);
}

#[test]
fn generated_chapters_need_no_repository_attribution() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("generated.html"),
        r#"<h1 id="generated"></h1>"#,
    )
    .unwrap();
    fs::write(root.path().join("index.html"), "<p>generated index</p>").unwrap();
    let context = context("0.5.4", true, &[chapter(Some("generated.md"), None, &[])]);

    let evidence = mdbook_site_evidence(
        hb("amiss/test", b"candidate"),
        &site("book.toml", "/manual/"),
        &context,
        &output(&root),
    )
    .unwrap();
    let parsed = amiss_wire::semantic::parse(&evidence).unwrap();
    let routes: Vec<&serde_json::Value> = parsed
        .payload
        .observations
        .iter()
        .filter(|row| {
            row.get("kind").and_then(serde_json::Value::as_str) == Some("site-generated-route")
        })
        .collect();
    assert_eq!(routes.len(), 2);
    assert!(
        routes
            .iter()
            .all(|route| route.get("source").is_some_and(serde_json::Value::is_null))
    );
    let generated = observation(&parsed.payload.observations, "/manual/generated.html");
    assert_eq!(texts(generated, "anchors"), ["generated"]);
    let navigation = parsed
        .payload
        .observations
        .iter()
        .find(|row| row.get("kind").and_then(serde_json::Value::as_str) == Some("site-navigation"))
        .unwrap();
    assert_eq!(texts(navigation, "entrypoints"), ["/manual/index.html"]);
    assert!(texts(navigation, "reachable").is_empty());
}

#[test]
fn completed_links_not_chapter_membership_define_navigation() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(
        root.path().join("index.html"),
        r#"<base href="/manual/"><a href="first.html">first</a>"#,
    )
    .unwrap();
    fs::write(
        root.path().join("first.html"),
        r#"<a href="nested/second.html">second</a>"#,
    )
    .unwrap();
    fs::write(
        root.path().join("nested/second.html"),
        r#"<a href="https://example.com/elsewhere">external</a>"#,
    )
    .unwrap();
    fs::write(root.path().join("orphan.html"), "<p>orphan</p>").unwrap();
    let nested = chapter(Some("nested/second.md"), Some("nested/second.md"), &[]);
    let context = context(
        "0.5.4",
        true,
        &[
            chapter(Some("first.md"), Some("first.md"), &[nested]),
            chapter(Some("orphan.md"), Some("orphan.md"), &[]),
        ],
    );

    let evidence = mdbook_site_evidence(
        hb("amiss/test", b"candidate"),
        &site("book.toml", "/manual/"),
        &context,
        &output(&root),
    )
    .unwrap();
    let parsed = amiss_wire::semantic::parse(&evidence).unwrap();
    let navigation = parsed
        .payload
        .observations
        .iter()
        .find(|row| row.get("kind").and_then(serde_json::Value::as_str) == Some("site-navigation"))
        .unwrap();
    assert_eq!(
        texts(navigation, "reachable"),
        ["guide/first.md", "guide/nested/second.md"]
    );
}

#[test]
fn configuration_locale_and_version_define_the_planned_context() {
    let base = site("docs/book.toml", "/manual/");
    let mut variants = Vec::new();
    let mut configuration = base.clone();
    configuration.configuration = RepoPathText::new("archive/book.toml".to_owned()).unwrap();
    variants.push(configuration);
    let mut prefix = base.clone();
    prefix.route_prefix = "/archive/".to_owned();
    variants.push(prefix);
    let mut locale = base.clone();
    locale.locale = Some("fr-FR".to_owned());
    variants.push(locale);
    let mut version = base.clone();
    version.version = Some("2.1.0".to_owned());
    variants.push(version);

    let expected = mdbook_site_expectation(&base).unwrap().context_digest;
    assert!(
        variants.into_iter().all(|variant| {
            mdbook_site_expectation(&variant).unwrap().context_digest != expected
        })
    );
    assert!(matches!(
        mdbook_site_expectation(&site("docs/config.toml", "/manual/")),
        Err(MdBookEvidenceError::ContextIdentity)
    ));
    let mut invalid_locale = base;
    invalid_locale.locale = Some("en us".to_owned());
    assert!(matches!(
        mdbook_site_expectation(&invalid_locale),
        Err(MdBookEvidenceError::ContextIdentity)
    ));
}

#[test]
fn resolved_renderer_configuration_is_part_of_the_input_identity() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("chapter.html"), "<h1 id=\"chapter\"></h1>").unwrap();
    fs::write(
        root.path().join("index.html"),
        "<a href=\"chapter.html\">chapter</a>",
    )
    .unwrap();
    let items = [chapter(Some("chapter.md"), Some("chapter.md"), &[])];
    let original = context("0.5.4", true, &items);
    let changed = String::from_utf8(original.clone())
        .unwrap()
        .replace(
            r#""book":{"src":"guide"}"#,
            r#""book":{"src":"guide","title":"changed"}"#,
        )
        .into_bytes();
    assert_ne!(original, changed);
    let candidate = hb("amiss/test", b"candidate");
    let site = site("book.toml", "/");
    let first = mdbook_site_evidence(candidate, &site, &original, &output(&root)).unwrap();
    let second = mdbook_site_evidence(candidate, &site, &changed, &output(&root)).unwrap();
    let first = amiss_wire::semantic::parse(&first).unwrap();
    let second = amiss_wire::semantic::parse(&second).unwrap();

    assert_eq!(
        first.payload.producer.context_digest,
        second.payload.producer.context_digest
    );
    assert_ne!(
        first.payload.producer.input_digest,
        second.payload.producer.input_digest
    );
}

#[test]
fn renderer_shapes_preserve_required_nullable_paths_and_default_source_directory() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("chapter.html"), "<h1 id=\"chapter\"></h1>").unwrap();
    fs::write(root.path().join("index.html"), "<p>index</p>").unwrap();
    let ordinary = context(
        "0.5.4",
        true,
        &[chapter(Some("chapter.md"), Some("chapter.md"), &[])],
    );
    let original: serde_json::Value = serde_json::from_slice(&ordinary).unwrap();
    let candidate = hb("amiss/test", b"candidate");
    let site = site("book.toml", "/");
    for (path, invalid) in [
        ("/book/items", json!(null)),
        ("/book/items/0", json!("Unknown")),
        ("/book/items/0", json!({ "Chapter": null })),
        ("/book/items/0", json!({ "PartTitle": 1 })),
        (
            "/book/items/0",
            json!({ "PartTitle": "title", "other": null }),
        ),
        ("/book/items/0/Chapter/path", json!(false)),
        ("/book/items/0/Chapter/source_path", json!([])),
        ("/book/items/0/Chapter/sub_items", json!(null)),
        ("/config/book/src", json!(null)),
        ("/config/book/src", json!(false)),
        ("/config/output/html", json!([])),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(path).unwrap() = invalid;
        let bytes = serde_json::to_vec(&changed).unwrap();
        assert!(
            mdbook_site_evidence(candidate, &site, &bytes, &output(&root)).is_err(),
            "{path}: {changed}"
        );
    }
    for required in ["path", "source_path", "sub_items"] {
        let mut changed = original.clone();
        changed["book"]["items"][0]["Chapter"]
            .as_object_mut()
            .unwrap()
            .remove(required);
        let bytes = serde_json::to_vec(&changed).unwrap();
        assert!(
            matches!(
                mdbook_site_evidence(candidate, &site, &bytes, &output(&root)),
                Err(MdBookEvidenceError::ContextShape)
            ),
            "{required}"
        );
    }
    let mut defaulted = original;
    defaulted["config"]["book"]
        .as_object_mut()
        .unwrap()
        .remove("src");
    defaulted["book"]["items"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({ "PartTitle": "Part one" }));
    let bytes = serde_json::to_vec(&defaulted).unwrap();
    let evidence = mdbook_site_evidence(candidate, &site, &bytes, &output(&root)).unwrap();
    let parsed = amiss_wire::semantic::parse(&evidence).unwrap();
    assert_eq!(
        observation(&parsed.payload.observations, "/chapter.html")["source"],
        "src/chapter.md"
    );
}

#[test]
fn opaque_renderer_configuration_keeps_canonical_identity_and_the_existing_depth_limit() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("chapter.html"), "<h1 id=\"chapter\"></h1>").unwrap();
    fs::write(root.path().join("index.html"), "<p>index</p>").unwrap();
    let ordinary = context(
        "0.5.4",
        true,
        &[chapter(Some("chapter.md"), Some("chapter.md"), &[])],
    );
    let mut changed: serde_json::Value = serde_json::from_slice(&ordinary).unwrap();
    let candidate = hb("amiss/test", b"candidate");
    let site = site("book.toml", "/");
    let baseline = mdbook_site_evidence(candidate, &site, &ordinary, &output(&root)).unwrap();
    let baseline = amiss_wire::semantic::parse(&baseline).unwrap();
    let mut nested = json!({ "\u{1f600}": 1, "\u{e000}": 2 });
    for _ in 0..256 {
        nested = json!([nested]);
    }
    changed["config"]["future-renderer-options"] = nested.clone();
    changed["book"]["items"][0]["Chapter"]["future-metadata"] = nested;
    let compact = serde_json::to_vec(&changed).unwrap();
    let pretty = serde_json::to_vec_pretty(&changed).unwrap();
    let evidence = mdbook_site_evidence(candidate, &site, &compact, &output(&root)).unwrap();
    assert_eq!(
        evidence,
        mdbook_site_evidence(candidate, &site, &pretty, &output(&root)).unwrap()
    );
    let parsed = amiss_wire::semantic::parse(&evidence).unwrap();
    assert_eq!(baseline.payload.observations, parsed.payload.observations);
    assert_ne!(
        baseline.payload.producer.input_digest,
        parsed.payload.producer.input_digest
    );
    let mut nested = json!(null);
    for _ in 0..513 {
        nested = json!([nested]);
    }
    changed["config"]["future-renderer-options"] = nested;
    let bytes = serde_json::to_vec(&changed).unwrap();
    assert!(matches!(
        mdbook_site_evidence(candidate, &site, &bytes, &output(&root)),
        Err(MdBookEvidenceError::Context(_))
    ));
}

#[test]
fn version_renderer_and_route_ownership_must_be_exact() {
    let root = tempfile::tempdir().unwrap();
    let ordinary = [chapter(Some("chapter.md"), Some("chapter.md"), &[])];
    assert!(matches!(
        mdbook_site_evidence(
            hb("amiss/test", b"candidate"),
            &site("book.toml", "/"),
            &context("0.5.3", true, &ordinary),
            &output(&root),
        ),
        Err(MdBookEvidenceError::UnsupportedBuild)
    ));
    assert!(matches!(
        mdbook_site_evidence(
            hb("amiss/test", b"candidate"),
            &site("book.toml", "/"),
            &context("0.5.4", false, &ordinary),
            &output(&root),
        ),
        Err(MdBookEvidenceError::UnsupportedBuild)
    ));
    assert!(matches!(
        mdbook_site_evidence(
            hb("amiss/test", b"candidate"),
            &site("book.toml", "/"),
            &context(
                "0.5.4",
                true,
                &[
                    chapter(Some("chapter.md"), Some("chapter.md"), &[]),
                    chapter(Some("index.md"), Some("index.md"), &[]),
                ],
            ),
            &output(&root),
        ),
        Err(MdBookEvidenceError::UnsupportedBuild)
    ));
}

#[test]
fn malformed_escaping_oversized_or_unreadable_input_fails_closed() {
    let root = tempfile::tempdir().unwrap();
    let candidate = hb("amiss/test", b"candidate");
    let ordinary = context(
        "0.5.4",
        true,
        &[chapter(Some("chapter.md"), Some("chapter.md"), &[])],
    );
    assert!(matches!(
        mdbook_site_evidence(
            candidate,
            &site("book.toml", "relative/"),
            &ordinary,
            &output(&root),
        ),
        Err(MdBookEvidenceError::Route)
    ));
    assert!(matches!(
        mdbook_site_evidence(
            candidate,
            &site("book.toml", "/"),
            &context(
                "0.5.4",
                true,
                &[chapter(Some("../escape.md"), Some("chapter.md"), &[],)],
            ),
            &output(&root),
        ),
        Err(MdBookEvidenceError::Path)
    ));
    assert!(matches!(
        mdbook_site_evidence(
            candidate,
            &site("book.toml", "/"),
            &ordinary,
            &output(&root),
        ),
        Err(MdBookEvidenceError::Output(_))
    ));
    assert!(matches!(
        mdbook_site_evidence(
            candidate,
            &site("book.toml", "/"),
            br#"{"version":"0.5.4","version":"0.5.4"}"#,
            &output(&root),
        ),
        Err(MdBookEvidenceError::Context(_))
    ));
    let oversized_context = vec![b' '; usize::try_from(MDBOOK_RENDER_CONTEXT_BYTES).unwrap() + 1];
    assert!(matches!(
        mdbook_site_evidence(
            candidate,
            &site("book.toml", "/"),
            &oversized_context,
            &output(&root),
        ),
        Err(MdBookEvidenceError::ContextBytes)
    ));

    let file = fs::File::create(root.path().join("chapter.html")).unwrap();
    file.set_len(MDBOOK_HTML_BYTES + 1).unwrap();
    assert!(matches!(
        mdbook_site_evidence(
            candidate,
            &site("book.toml", "/"),
            &ordinary,
            &output(&root),
        ),
        Err(MdBookEvidenceError::Output(_))
    ));
}

#[test]
fn unrepresentable_published_anchor_fails_the_complete_set() {
    let root = tempfile::tempdir().unwrap();
    let anchor = "a".repeat(4_097);
    fs::write(
        root.path().join("chapter.html"),
        format!(r#"<!doctype html><h1 id="{anchor}"></h1>"#),
    )
    .unwrap();
    let context = context(
        "0.5.4",
        true,
        &[chapter(Some("chapter.md"), Some("chapter.md"), &[])],
    );
    assert!(matches!(
        mdbook_site_evidence(
            hb("amiss/test", b"candidate"),
            &site("book.toml", "/"),
            &context,
            &output(&root),
        ),
        Err(MdBookEvidenceError::Anchor)
    ));
}
