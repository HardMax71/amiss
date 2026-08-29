#![cfg(test)]

use std::ffi::OsString;

use super::{Failure, invocation};

#[test]
fn invocation_is_closed_and_positional() {
    let accepted = invocation(
        ["--context", "context.json", "--rustdoc", "crate.json"]
            .into_iter()
            .map(OsString::from),
    )
    .unwrap();
    assert_eq!(accepted.context, std::path::Path::new("context.json"));
    assert_eq!(accepted.rustdoc, std::path::Path::new("crate.json"));

    for rejected in [
        Vec::new(),
        vec!["--rustdoc", "crate.json", "--context", "context.json"],
        vec!["--context", "context.json", "--rustdoc"],
        vec!["--context", "", "--rustdoc", "crate.json"],
        vec![
            "--context",
            "context.json",
            "--rustdoc",
            "crate.json",
            "extra",
        ],
    ] {
        assert!(matches!(
            invocation(rejected.into_iter().map(OsString::from)),
            Err(Failure::Invocation)
        ));
    }
}
