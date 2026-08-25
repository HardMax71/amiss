#![expect(
    clippy::unwrap_used,
    reason = "the fixture constructs known-valid renderer contexts and output trees"
)]

use std::fs;

use amiss_controller::{
    MDBOOK_HTML_BYTES, MDBOOK_RENDER_CONTEXT_BYTES, MdBookEvidenceError, mdbook_site_evidence,
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
    let book_root = RepoPathText::new("docs".to_owned()).unwrap();

    let evidence = mdbook_site_evidence(
        candidate,
        Some(&book_root),
        "/manual/",
        &context,
        &output(&root),
    )
    .unwrap();
    let parsed = amiss_wire::semantic::parse(&canonical(&evidence)).unwrap();

    assert_eq!(parsed.payload.candidate_identity_digest, candidate);
    assert_eq!(parsed.payload.producer_kind.as_str(), "site-build");
    assert_eq!(parsed.payload.producer_version, "0.3.0");
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

    let repeated = mdbook_site_evidence(
        candidate,
        Some(&book_root),
        "/manual/",
        &context,
        &output(&root),
    )
    .unwrap();
    assert_eq!(evidence, repeated);
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
        None,
        "/manual/",
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
fn version_renderer_source_and_route_ownership_must_be_exact() {
    let root = tempfile::tempdir().unwrap();
    let ordinary = [chapter(Some("chapter.md"), Some("chapter.md"), &[])];
    assert!(matches!(
        mdbook_site_evidence(
            hb("amiss/test", b"candidate"),
            None,
            "/",
            &context("0.5.3", true, &ordinary),
            &output(&root),
        ),
        Err(MdBookEvidenceError::UnsupportedBuild)
    ));
    assert!(matches!(
        mdbook_site_evidence(
            hb("amiss/test", b"candidate"),
            None,
            "/",
            &context("0.5.4", false, &ordinary),
            &output(&root),
        ),
        Err(MdBookEvidenceError::UnsupportedBuild)
    ));
    assert!(matches!(
        mdbook_site_evidence(
            hb("amiss/test", b"candidate"),
            None,
            "/",
            &context("0.5.4", true, &[chapter(Some("generated.md"), None, &[])],),
            &output(&root),
        ),
        Err(MdBookEvidenceError::UnsupportedBuild)
    ));
    assert!(matches!(
        mdbook_site_evidence(
            hb("amiss/test", b"candidate"),
            None,
            "/",
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
        mdbook_site_evidence(candidate, None, "relative/", &ordinary, &output(&root)),
        Err(MdBookEvidenceError::Route)
    ));
    assert!(matches!(
        mdbook_site_evidence(
            candidate,
            None,
            "/",
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
        mdbook_site_evidence(candidate, None, "/", &ordinary, &output(&root)),
        Err(MdBookEvidenceError::Output(_))
    ));
    assert!(matches!(
        mdbook_site_evidence(
            candidate,
            None,
            "/",
            br#"{"version":"0.5.4","version":"0.5.4"}"#,
            &output(&root),
        ),
        Err(MdBookEvidenceError::Context(_))
    ));
    let oversized_context = vec![b' '; usize::try_from(MDBOOK_RENDER_CONTEXT_BYTES).unwrap() + 1];
    assert!(matches!(
        mdbook_site_evidence(candidate, None, "/", &oversized_context, &output(&root)),
        Err(MdBookEvidenceError::ContextBytes)
    ));

    let file = fs::File::create(root.path().join("chapter.html")).unwrap();
    file.set_len(MDBOOK_HTML_BYTES + 1).unwrap();
    assert!(matches!(
        mdbook_site_evidence(candidate, None, "/", &ordinary, &output(&root)),
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
            None,
            "/",
            &context,
            &output(&root),
        ),
        Err(MdBookEvidenceError::Anchor)
    ));
}
