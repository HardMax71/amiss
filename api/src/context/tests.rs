#![cfg(test)]

use super::{Error, parse};

fn context(features: &str, target_triple: &str) -> Vec<u8> {
    format!(
        r#"{{"cfg":[],"compiler":"rustc 1.100.0-nightly","dependencies_digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","features":{features},"name":"rust/example/local-function-declarations","package":"example","rustdoc_format":61,"schema":"amiss/rust-public-api-context","target":"example","target_triple":"{target_triple}"}}"#,
    )
    .into_bytes()
}

#[test]
fn context_binds_the_complete_normalization_configuration() {
    let parsed = parse(&context(
        r#"["default","serde"]"#,
        "x86_64-unknown-linux-gnu",
    ))
    .unwrap();
    let differently_featured =
        parse(&context(r#"["default"]"#, "x86_64-unknown-linux-gnu")).unwrap();
    let differently_targeted =
        parse(&context(r#"["default","serde"]"#, "wasm32-unknown-unknown")).unwrap();

    assert_ne!(parsed.digest, differently_featured.digest);
    assert_ne!(parsed.digest, differently_targeted.digest);
    assert_eq!(
        parsed.name.as_str(),
        "rust/example/local-function-declarations"
    );
    assert_eq!(parsed.rustdoc_format, 61);
    assert_eq!(parsed.target, "example");
    assert_eq!(parsed.target_triple, "x86_64-unknown-linux-gnu");
}

#[test]
fn context_refuses_ambiguous_sets_and_unscoped_names() {
    assert!(matches!(
        parse(&context(
            r#"["serde","default"]"#,
            "x86_64-unknown-linux-gnu"
        )),
        Err(Error::Shape(_))
    ));
    let unscoped = String::from_utf8(context("[]", "x86_64-unknown-linux-gnu"))
        .unwrap()
        .replace("rust/example/local-function-declarations", "rust/example");
    assert!(matches!(parse(unscoped.as_bytes()), Err(Error::Shape(_))));
}
