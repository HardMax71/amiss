use std::collections::BTreeMap;
use std::process::Command;

use rustdoc_types::Crate;

use super::super::{Normalized, function_declarations};

const HOST: &str = "x86_64-unknown-linux-gnu";
const WASM: &str = "wasm32-unknown-unknown";

#[test]
#[ignore = "requires the separately pinned format-61 Rustdoc toolchain and wasm target"]
fn cargo_configurations_preserve_record_boundaries() {
    let target = tempfile::tempdir().unwrap();
    let baseline = normalize(&[], HOST, target.path());
    let repeated = normalize(&[], HOST, target.path());
    let unrelated = normalize(&["unrelated"], HOST, target.path());
    let featured = normalize(&["api"], HOST, target.path());
    let wasm = normalize(&[], WASM, target.path());

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
                "fn/symbol_fixture::target_word".to_owned(),
                "pub fn symbol_fixture::target_word(value: u64) -> u64".to_owned(),
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
    let mut expected_wasm = repeated.records;
    expected_wasm.insert(
        "fn/symbol_fixture::target_word".to_owned(),
        "pub fn symbol_fixture::target_word(value: u32) -> u32".to_owned(),
    );
    assert_eq!(wasm.records, expected_wasm);
}

fn normalize(features: &[&str], triple: &str, target: &std::path::Path) -> Normalized {
    let toolchain = std::env::var("AMISS_RUSTDOC_TOOLCHAIN").unwrap();
    let manifest =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rustdoc/Cargo.toml");
    let mut cargo = Command::new("rustup");
    cargo.args(["run", &toolchain, "cargo", "rustdoc", "--manifest-path"]);
    cargo.arg(manifest).args(["--frozen", "--target-dir"]);
    cargo.arg(target).args(["--lib", "--target", triple]);
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
    let bytes = std::fs::read(target.join(triple).join("doc/symbol_fixture.json")).unwrap();
    let crate_: Crate = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(crate_.target.triple, triple);
    function_declarations(
        &bytes,
        rustdoc_types::FORMAT_VERSION,
        "symbol_fixture",
        triple,
    )
    .unwrap()
}
