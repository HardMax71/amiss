use std::collections::BTreeMap;
use std::process::Command;

use rustdoc_types::Crate;

use super::super::{Normalized, function_declarations};

#[test]
#[ignore = "requires the separately pinned format-61 Rustdoc toolchain"]
fn cargo_features_preserve_configuration_boundaries() {
    let target = tempfile::tempdir().unwrap();
    let baseline = normalize(&[], target.path());
    let repeated = normalize(&[], target.path());
    let unrelated = normalize(&["unrelated"], target.path());
    let featured = normalize(&["api"], target.path());

    assert_eq!(baseline.records, repeated.records);
    assert_eq!(baseline.records, unrelated.records);
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
    let mut expected_featured = baseline.records;
    expected_featured.insert(
        "fn/symbol_fixture::feature_only".to_owned(),
        "pub fn symbol_fixture::feature_only(value: usize) -> usize".to_owned(),
    );
    assert_eq!(featured.records, expected_featured);
}

fn normalize(features: &[&str], target: &std::path::Path) -> Normalized {
    let toolchain = std::env::var("AMISS_RUSTDOC_TOOLCHAIN").unwrap();
    let manifest =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rustdoc/Cargo.toml");
    let mut cargo = Command::new("rustup");
    cargo.args(["run", &toolchain, "cargo", "rustdoc", "--manifest-path"]);
    cargo.arg(manifest).args(["--frozen", "--target-dir"]);
    cargo.arg(target).arg("--lib");
    if !features.is_empty() {
        cargo.args(["--features", &features.join(",")]);
    }
    cargo.args([
        "--",
        "--document-private-items",
        "-Z",
        "unstable-options",
        "--output-format",
        "json",
    ]);
    let executed = cargo.output().unwrap();
    assert!(
        executed.status.success(),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    let bytes = std::fs::read(target.join("doc/symbol_fixture.json")).unwrap();
    let crate_: Crate = serde_json::from_slice(&bytes).unwrap();
    function_declarations(
        &bytes,
        rustdoc_types::FORMAT_VERSION,
        "symbol_fixture",
        &crate_.target.triple,
    )
    .unwrap()
}
