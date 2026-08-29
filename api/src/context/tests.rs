#![cfg(test)]

use super::{Error, parse};

fn context(features: &str) -> Vec<u8> {
    format!(
        r#"{{"cfg":[],"compiler":"rustc 1.100.0-nightly","dependencies_digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","features":{features},"name":"rust/example/local-free-functions","package":"example","rustdoc_format":61,"schema":"amiss/rust-public-api-context","target":"example","target_triple":"x86_64-unknown-linux-gnu"}}"#,
    )
    .into_bytes()
}

#[test]
fn context_binds_the_complete_normalization_configuration() {
    let parsed = parse(&context(r#"["default","serde"]"#)).unwrap();

    assert_eq!(parsed.name.as_str(), "rust/example/local-free-functions");
    assert_eq!(parsed.rustdoc_format, 61);
    assert_eq!(parsed.target, "example");
    assert_eq!(parsed.target_triple, "x86_64-unknown-linux-gnu");
}

#[test]
fn context_refuses_ambiguous_sets_and_unscoped_names() {
    assert!(matches!(
        parse(&context(r#"["serde","default"]"#)),
        Err(Error::Shape(_))
    ));
    let unscoped = String::from_utf8(context("[]"))
        .unwrap()
        .replace("rust/example/local-free-functions", "rust/example");
    assert!(matches!(parse(unscoped.as_bytes()), Err(Error::Shape(_))));
}
