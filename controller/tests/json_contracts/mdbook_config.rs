use amiss_controller::MDBOOK_RENDER_CONTEXT_BYTES;
use amiss_controller::mdbook::RenderContext;
use amiss_controller::mdbook::config::{
    Config, HtmlConfig, NoExtensions, PlaygroundConfig, RustEdition, TextDirection,
};
use amiss_wire::{JsonInputError, digest::CanonicalJsonError, read_json};

use super::mdbook::Renderers;

#[test]
fn html_map_constraints_belong_to_the_typed_fields() {
    for input in [
        "{}",
        r#"{"code":{},"search":{}}"#,
        r#"{"redirect":{},"code":{"hidelines":{}},"search":{"chapter":{}}}"#,
        include_str!("../fixtures/mdbook-html-config.json"),
    ] {
        let html: HtmlConfig = serde_json::from_str(input).unwrap();
        assert_eq!(
            read_json::<HtmlConfig>(input.as_bytes(), MDBOOK_RENDER_CONTEXT_BYTES).unwrap(),
            html
        );
    }
    let nulls = r#"{"redirect":null,"code":{"hidelines":null},"search":{"chapter":null}}"#;
    let html: HtmlConfig = serde_json::from_str(nulls).unwrap();
    assert_eq!(html.redirect, None);
    assert_eq!(html.code.unwrap().hidelines, None);
    assert_eq!(html.search.unwrap().chapter, None);
    assert!(read_json::<HtmlConfig>(nulls.as_bytes(), MDBOOK_RENDER_CONTEXT_BYTES).is_err());

    let input = include_str!("../fixtures/mdbook-html-config.json");
    let accepted: Vec<_> = [
        (
            r#""old.html":"new.html""#,
            r#""old.html":"new.html","old.html":"new.html""#,
        ),
        (
            r#""old.html":"new.html""#,
            r#""old.html":"other.html","\u006fld.html":"new.html""#,
        ),
        (r##""rust":"#""##, r##""rust":"#","rust":"#""##),
        (r##""rust":"#""##, r##""rust":"//","\u0072ust":"#""##),
        (
            r#""intro.md":{"enable":false}"#,
            r#""intro.md":{"enable":false},"intro.md":{"enable":false}"#,
        ),
        (
            r#""intro.md":{"enable":false}"#,
            r#""intro.md":{"enable":true},"\u0069ntro.md":{"enable":false}"#,
        ),
    ]
    .into_iter()
    .filter_map(|(original, replacement)| {
        assert_eq!(input.matches(original).count(), 1);
        let invalid = input.replacen(original, replacement, 1);
        assert!(read_json::<HtmlConfig>(invalid.as_bytes(), MDBOOK_RENDER_CONTEXT_BYTES).is_err());
        serde_json::from_str::<HtmlConfig>(&invalid)
            .is_ok()
            .then_some(replacement)
    })
    .collect();
    assert!(
        accepted.is_empty(),
        "typed maps accepted duplicates: {accepted:?}"
    );
}

#[test]
fn normalized_build_and_book_settings_remain_complete_and_typed() {
    let input = br#"{"book":{"title":null,"authors":[],"description":null,"language":"ar","text-direction":"rtl"},"build":{"build-dir":"C:\\operator\\book","create-missing":false,"use-default-preprocessors":true,"extra-watch-dirs":["assets"]},"rust":{"edition":"2024"},"output":{"html":{}}}"#;
    let config: Config<NoExtensions, NoExtensions> =
        read_json(input, MDBOOK_RENDER_CONTEXT_BYTES).unwrap();
    assert_eq!(config.book.text_direction, Some(TextDirection::RightToLeft));
    assert_eq!(config.book.src, None);
    assert_eq!(
        config.rust.as_ref().unwrap().edition,
        Some(RustEdition::E2024)
    );
    assert_eq!(
        config.build.as_ref().unwrap().build_dir,
        r"C:\operator\book"
    );
    for (original, replacement) in [
        (r#""text-direction":"rtl""#, r#""text-direction":"other""#),
        (r#""edition":"2024""#, r#""edition":"2027""#),
        (r#""create-missing":false,"#, ""),
        (r#""edition":"2024""#, ""),
        (r#""build-dir":"#, r#""future":true,"build-dir":"#),
        (r#""edition":"2024""#, r#""edition":"2024","future":true"#),
        (
            r#""extra-watch-dirs":["assets"]"#,
            r#""extra-watch-dirs":null"#,
        ),
    ] {
        let invalid = std::str::from_utf8(input)
            .unwrap()
            .replace(original, replacement);
        assert_ne!(invalid.as_bytes(), input);
        assert!(
            matches!(
                read_json::<Config<NoExtensions, NoExtensions>>(
                    invalid.as_bytes(),
                    MDBOOK_RENDER_CONTEXT_BYTES
                ),
                Err(JsonInputError::Shape(_))
            ),
            "{invalid}"
        );
    }
}

#[test]
fn captured_config_requires_declared_renderers_and_preserves_their_identity() {
    let input = include_bytes!("../fixtures/mdbook-render-context.json");
    let context: RenderContext<NoExtensions, Renderers> =
        read_json(input, MDBOOK_RENDER_CONTEXT_BYTES).unwrap();
    assert_eq!(context.config.output.additional.capture.command, "jq -c .");
    assert!(matches!(
        read_json::<RenderContext<NoExtensions, NoExtensions>>(input, MDBOOK_RENDER_CONTEXT_BYTES),
        Err(JsonInputError::Canonical(CanonicalJsonError::InputChanged))
    ));
    let expected = br#"{"book":{"authors":["Amiss"],"description":null,"language":"en","text-direction":null,"title":"Typed context probe"},"output":{"capture":{"command":"jq -c ."},"html":{"additional-css":[]}}}"#;
    assert_eq!(
        amiss_wire::digest::hj_serde("amiss/controller-mdbook-config-v1", |mut writer| {
            serde_json_canonicalizer::to_writer(&context.config, &mut writer)
        })
        .unwrap(),
        amiss_wire::digest::hb("amiss/controller-mdbook-config-v1", expected)
    );
    for (original, replacement) in [
        (r#""root": "/operator/book","#, ""),
        (
            r#""version": "0.5.4","#,
            r#""version": "0.5.4", "extra": true,"#,
        ),
        (r#""items": ["#, r#""extra": true, "items": ["#),
        (
            r#""name": "Introduction","#,
            r#""name": "Introduction", "extra": true,"#,
        ),
        (r#""output": {"#, r#""future": true, "output": {"#),
        (
            r#""title": "Typed context probe","#,
            r#""future": true, "title": "Typed context probe","#,
        ),
        (r#""capture": {"#, r#""other-renderer": {}, "capture": {"#),
        (
            r#""command": "jq -c .""#,
            r#""command": "jq -c .", "future": true"#,
        ),
        (
            r#""additional-css": []"#,
            r#""additional-css": [], "future": true"#,
        ),
        (r#""additional-css": []"#, r#""additional-css": null"#),
        (r#""authors": ["#, r#""src": null, "authors": ["#),
        (r#""description": null,"#, ""),
    ] {
        let invalid = std::str::from_utf8(input)
            .unwrap()
            .replace(original, replacement);
        assert_ne!(invalid.as_bytes(), input);
        let error = read_json::<RenderContext<NoExtensions, Renderers>>(
            invalid.as_bytes(),
            MDBOOK_RENDER_CONTEXT_BYTES,
        )
        .err()
        .unwrap();
        assert_eq!(
            matches!(error, JsonInputError::Shape(_)),
            serde_json::from_str::<RenderContext<NoExtensions, Renderers>>(&invalid).is_err(),
            "{invalid}"
        );
        assert!(
            matches!(
                error,
                JsonInputError::Shape(_)
                    | JsonInputError::Canonical(CanonicalJsonError::InputChanged)
            ),
            "{invalid}: {error}"
        );
    }
}

#[test]
fn html_settings_keep_presence_aliases_and_typed_numeric_limits() {
    let input = include_bytes!("../fixtures/mdbook-html-config.json");
    let html: HtmlConfig = read_json(input, MDBOOK_RENDER_CONTEXT_BYTES).unwrap();
    assert_eq!(html.fold.as_ref().unwrap().level, Some(u8::MAX));
    assert_eq!(html.search.as_ref().unwrap().limit_results, Some(u32::MAX));
    assert!(matches!(
        html.playground,
        Some(PlaygroundConfig::Playpen(_))
    ));
    let modern = std::str::from_utf8(input)
        .unwrap()
        .replace("playpen", "playground");
    let modern: HtmlConfig = read_json(modern.as_bytes(), MDBOOK_RENDER_CONTEXT_BYTES).unwrap();
    assert!(matches!(
        modern.playground,
        Some(PlaygroundConfig::Playground(_))
    ));
    for (original, replacement) in [
        (r#""level":255"#, r#""level":256"#),
        (
            r#""limit-results":4294967295"#,
            r#""limit-results":4294967296"#,
        ),
        (r#""level":255"#, r#""level":null"#),
        (r#""playpen":{"#, r#""playground":{},"playpen":{"#),
        (r#""runnable":true"#, r#""runnable":true,"future":true"#),
        (r#""level":255"#, r#""level":255,"future":true"#),
        (
            r#""page-break":false"#,
            r#""page-break":false,"future":true"#,
        ),
        (
            r#""limit-results":4294967295"#,
            r#""limit-results":4294967295,"future":true"#,
        ),
        (r#""intro.md":{"#, r#""intro.md":{"future":true,"#),
        (r#""hidelines":{"#, r#""future":true,"hidelines":{"#),
        (r#""theme":"theme""#, r#""theme":null"#),
    ] {
        let invalid = std::str::from_utf8(input)
            .unwrap()
            .replace(original, replacement);
        assert_ne!(invalid.as_bytes(), input);
        let error =
            read_json::<HtmlConfig>(invalid.as_bytes(), MDBOOK_RENDER_CONTEXT_BYTES).unwrap_err();
        assert_eq!(
            matches!(error, JsonInputError::Shape(_)),
            serde_json::from_str::<HtmlConfig>(&invalid).is_err(),
            "{invalid}"
        );
        assert!(
            matches!(
                error,
                JsonInputError::Shape(_)
                    | JsonInputError::Canonical(CanonicalJsonError::InputChanged)
            ),
            "{invalid}: {error}"
        );
    }
}
