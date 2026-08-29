use std::collections::BTreeMap;
use std::process::Command;

use rustdoc_types::Crate;

use super::super::{Normalized, function_declarations};

#[test]
#[ignore = "requires the separately pinned format-61 Rustdoc toolchain"]
fn rustdoc_keeps_public_records_stable_across_unrelated_items() {
    let baseline = normalize(None);
    let repeated = normalize(None);
    let shifted = normalize(Some("unrelated"));

    assert_eq!(baseline.records, repeated.records);
    assert_eq!(baseline.records, shifted.records);
    assert_eq!(
        baseline.records,
        BTreeMap::from([
            (
                "fn/symbol_fixture::alias".to_owned(),
                "pub fn symbol_fixture::alias(value: u64) -> bool".to_owned(),
            ),
            (
                "fn/symbol_fixture::generated".to_owned(),
                "pub fn symbol_fixture::generated<T>(value: T) -> T where T: Clone".to_owned(),
            ),
            (
                "inherent-fn/symbol_fixture::PublicOwner::create".to_owned(),
                "pub fn symbol_fixture::PublicOwner::create() -> Self".to_owned(),
            ),
            (
                "inherent-fn/symbol_fixture::PublicOwner::generic".to_owned(),
                "pub fn symbol_fixture::PublicOwner::generic<T>(value: T) -> T".to_owned(),
            ),
            (
                "trait-fn/symbol_fixture::PublicService::execute".to_owned(),
                "pub fn symbol_fixture::PublicService::execute(self: &Self) -> bool".to_owned(),
            ),
        ])
    );
}

fn normalize(configuration: Option<&str>) -> Normalized {
    let output = tempfile::tempdir().unwrap();
    let toolchain = std::env::var("AMISS_RUSTDOC_TOOLCHAIN").unwrap();
    let mut rustdoc = Command::new("rustup");
    rustdoc.args([
        "run",
        &toolchain,
        "rustdoc",
        "--edition",
        "2024",
        "--crate-type",
        "lib",
        "--document-private-items",
        "-Z",
        "unstable-options",
        "--output-format",
        "json",
        "-o",
    ]);
    rustdoc.arg(output.path());
    if let Some(configuration) = configuration {
        rustdoc.args(["--cfg", configuration]);
    }
    rustdoc.arg(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/rustdoc/public_functions.rs"),
    );
    let executed = rustdoc.output().unwrap();
    assert!(
        executed.status.success(),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    let bytes = std::fs::read(output.path().join("symbol_fixture.json")).unwrap();
    let crate_: Crate = serde_json::from_slice(&bytes).unwrap();
    function_declarations(
        &bytes,
        rustdoc_types::FORMAT_VERSION,
        "symbol_fixture",
        &crate_.target.triple,
    )
    .unwrap()
}
