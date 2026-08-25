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
use amiss_wire::json::{Value, canonical};
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

fn observation<'a>(observations: &'a [Value], route: &str) -> &'a Value {
    observations
        .iter()
        .find(|row| row.text("route") == Some(route))
        .unwrap()
}

fn texts<'a>(observation: &'a Value, name: &str) -> Vec<&'a str> {
    let Some(Value::Array(values)) = observation.member(name) else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(|value| match value {
            Value::String(value) => Some(value.as_ref()),
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::Array(_)
            | Value::Object(_) => None,
        })
        .collect()
}

#[test]
fn postprocessed_pages_become_exact_source_bound_routes_and_anchors() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(
        root.path().join("intro.html"),
        br#"<!doctype html><h1 id="intro"></h1><span id="entity&amp;anchor"></span><i id="intro"></i>"#,
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
    let parsed = amiss_wire::semantic::parse(&canonical(&evidence)).unwrap();

    assert_eq!(parsed.payload.candidate_identity_digest, candidate);
    assert_eq!(parsed.payload.producer_kind.as_str(), "site-build");
    assert_eq!(parsed.payload.producer_version, "0.5.0");
    assert_eq!(
        parsed.payload.context_digest,
        mdbook_site_expectation(&site).unwrap().context_digest
    );
    assert!(parsed.payload.complete);
    assert_eq!(parsed.payload.observations.len(), 4);
    let intro = observation(&parsed.payload.observations, "/manual/intro.html");
    assert_eq!(intro.text("source"), Some("docs/guide/README.md"));
    assert_eq!(texts(intro, "anchors"), ["entity&anchor", "intro"]);
    let index = observation(&parsed.payload.observations, "/manual/index.html");
    assert_eq!(index.text("source"), Some("docs/guide/README.md"));
    assert_eq!(texts(index, "anchors"), ["home"]);
    let nested = observation(
        &parsed.payload.observations,
        "/manual/nested/%C3%BCber%20view.html",
    );
    assert_eq!(nested.text("source"), Some("docs/guide/nested/chapter.md"));
    assert_eq!(texts(nested, "anchors"), ["über-view"]);
    let navigation = parsed
        .payload
        .observations
        .iter()
        .find(|row| row.text("kind") == Some("site-navigation"))
        .unwrap();
    assert_eq!(navigation.text("root"), Some("docs/guide"));
    assert_eq!(navigation.text("manifest"), Some("docs/guide/SUMMARY.md"));
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
    let parsed = amiss_wire::semantic::parse(&canonical(&evidence)).unwrap();
    let routes: Vec<&Value> = parsed
        .payload
        .observations
        .iter()
        .filter(|row| row.text("kind") == Some("site-generated-route"))
        .collect();
    assert_eq!(routes.len(), 2);
    assert!(
        routes
            .iter()
            .all(|route| route.member("source") == Some(&Value::Null))
    );
    let generated = observation(&parsed.payload.observations, "/manual/generated.html");
    assert_eq!(texts(generated, "anchors"), ["generated"]);
    let navigation = parsed
        .payload
        .observations
        .iter()
        .find(|row| row.text("kind") == Some("site-navigation"))
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
    let parsed = amiss_wire::semantic::parse(&canonical(&evidence)).unwrap();
    let navigation = parsed
        .payload
        .observations
        .iter()
        .find(|row| row.text("kind") == Some("site-navigation"))
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
    let first = amiss_wire::semantic::parse(&canonical(&first)).unwrap();
    let second = amiss_wire::semantic::parse(&canonical(&second)).unwrap();

    assert_eq!(first.payload.context_digest, second.payload.context_digest);
    assert_ne!(first.payload.input_digest, second.payload.input_digest);
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
