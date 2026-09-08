use std::{borrow::Cow, fs};

use amiss_controller::mdbook::config::NoExtensions;
use amiss_controller::mdbook::{Book, BookItem, Chapter, RenderContext};
use amiss_controller::{MDBOOK_RENDER_CONTEXT_BYTES, SiteBuildContext, mdbook_site_evidence};
use amiss_fixtures::{SiteObservation, site_observation};
use amiss_wire::digest::hb;
use cap_std::{ambient_authority, fs::Dir};

#[derive(serde::Deserialize, serde::Serialize)]
pub(super) struct Renderers {
    pub(super) capture: Capture,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Capture {
    pub(super) command: String,
}

#[test]
fn mdbook_book_metadata_has_a_closed_lossless_serde_contract() {
    let input = br##"{"items":[{"Chapter":{"name":"Introduction","content":"# Introduction\n","number":[0,2,4294967295],"path":"intro.md","source_path":"README.md","sub_items":["Separator",{"PartTitle":"Part two"}],"parent_names":["Root"]}}]}"##;
    let book: Book = serde_json::from_slice(input).unwrap();
    assert_eq!(
        book,
        Book {
            items: vec![BookItem::Chapter(Chapter {
                name: "Introduction".to_owned(),
                content: "# Introduction\n".to_owned(),
                number: Some(vec![0, 2, u32::MAX]),
                path: Some("intro.md".to_owned()),
                source_path: Some("README.md".to_owned()),
                sub_items: vec![
                    BookItem::Separator,
                    BookItem::PartTitle("Part two".to_owned())
                ],
                parent_names: vec!["Root".to_owned()],
            })],
        }
    );
    let expected = br##"{"items":[{"Chapter":{"content":"# Introduction\n","name":"Introduction","number":[0,2,4294967295],"parent_names":["Root"],"path":"intro.md","source_path":"README.md","sub_items":["Separator",{"PartTitle":"Part two"}]}}]}"##;
    let encoded = serde_json_canonicalizer::to_vec(&book).unwrap();
    assert_eq!(
        std::str::from_utf8(&encoded).unwrap(),
        std::str::from_utf8(expected).unwrap()
    );
    for invalid in [
        r#"{"items":[],"extra":true}"#,
        r#"{"items":[{"Chapter":{"name":"Introduction","content":"","number":null,"path":null,"source_path":null,"sub_items":[],"parent_names":[],"extra":true}}]}"#,
        r#"{"items":[{"Chapter":{"content":"","number":null,"path":null,"source_path":null,"sub_items":[],"parent_names":[]}}]}"#,
        r#"{"items":[{"Chapter":{"name":"Introduction","content":"","path":null,"source_path":null,"sub_items":[],"parent_names":[]}}]}"#,
        r#"{"items":[{"Chapter":{"name":"Introduction","content":"","number":[4294967296],"path":null,"source_path":null,"sub_items":[],"parent_names":[]}}]}"#,
        r#"{"items":[{"Chapter":{"name":"Introduction","content":"","number":[-1],"path":null,"source_path":null,"sub_items":[],"parent_names":[]}}]}"#,
        r#"{"items":[{"Chapter":{"name":"Introduction","content":"","number":null,"path":null,"source_path":null,"sub_items":[],"parent_names":[false]}}]}"#,
    ] {
        assert!(serde_json::from_str::<Book>(invalid).is_err(), "{invalid}");
    }
    for required in [
        r#""name":"Introduction","#,
        r##""content":"# Introduction\n","##,
        r#""number":[0,2,4294967295],"#,
        r#""path":"intro.md","#,
        r#""source_path":"README.md","#,
        r#""sub_items":["Separator",{"PartTitle":"Part two"}],"#,
        r#","parent_names":["Root"]"#,
    ] {
        let invalid = std::str::from_utf8(input).unwrap().replace(required, "");
        assert_ne!(invalid.as_bytes(), input);
        assert!(serde_json::from_str::<Book>(&invalid).is_err(), "{invalid}");
    }
}

#[test]
fn real_mdbook_context_reads_only_the_callers_html_directory() {
    let input = include_bytes!("../fixtures/mdbook-render-context.json");
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("intro.html"),
        "<h1 id=\"intro\">Introduction</h1>",
    )
    .unwrap();
    fs::write(
        root.path().join("index.html"),
        "<a href=\"intro.html\">Introduction</a>",
    )
    .unwrap();
    let output = Dir::open_ambient_dir(root.path(), ambient_authority()).unwrap();
    let candidate = hb("test/mdbook", b"candidate");
    let site = SiteBuildContext {
        configuration: "docs/book.toml".parse().unwrap(),
        route_prefix: "/manual/".to_owned(),
        locale: None,
        version: None,
    };
    let evidence =
        mdbook_site_evidence::<NoExtensions, Renderers>(candidate, &site, input, &output).unwrap();
    let parsed = amiss_wire::semantic::parse(&evidence).unwrap();
    assert_eq!(parsed.payload.subject.candidate_identity_digest, candidate);
    assert!(
        parsed.payload.observations.contains(&Cow::Owned(
            site_observation(
                "/manual/intro.html",
                SiteObservation::Page("docs/src/intro.md", &["intro"]),
            )
            .unwrap()
        ))
    );
    let relocated = std::str::from_utf8(input)
        .unwrap()
        .replace("/operator/book", r"C:\\untrusted\\book");
    assert_ne!(relocated.as_bytes(), input);
    let relocated_evidence = mdbook_site_evidence::<NoExtensions, Renderers>(
        candidate,
        &site,
        relocated.as_bytes(),
        &output,
    )
    .unwrap();
    assert_eq!(
        std::str::from_utf8(&relocated_evidence).unwrap(),
        std::str::from_utf8(&evidence).unwrap()
    );

    let mut context: RenderContext<NoExtensions, Renderers> =
        amiss_wire::read_json(input, MDBOOK_RENDER_CONTEXT_BYTES).unwrap();
    context.config.output.additional.capture.command = "jq -cS .".to_owned();
    let changed = mdbook_site_evidence::<NoExtensions, Renderers>(
        candidate,
        &site,
        &serde_json::to_vec(&context).unwrap(),
        &output,
    )
    .unwrap();
    let changed = amiss_wire::semantic::parse(&changed).unwrap();
    assert_eq!(changed.payload.observations, parsed.payload.observations);
    assert_ne!(
        changed.payload.producer.input_digest,
        parsed.payload.producer.input_digest
    );
}
