#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned manifests"
)]

use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn lint_table(path: &Path, header: &str) -> Vec<String> {
    fs::read_to_string(path)
        .expect("manifest is readable")
        .split_once(header)
        .expect("manifest contains the lint table")
        .1
        .lines()
        .take_while(|line| !line.starts_with('['))
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn cargo_lints_are_repo_wide() {
    let root = repository_root();
    let manifests = [
        (root.join("Cargo.toml"), "workspace."),
        (root.join("controller/Cargo.toml"), "workspace."),
        (root.join("fuzz/Cargo.toml"), ""),
    ];

    for lint_group in ["rust", "clippy"] {
        let expected = lint_table(
            &manifests[0].0,
            &format!("[{}lints.{lint_group}]", manifests[0].1),
        );
        for (manifest, prefix) in &manifests[1..] {
            assert_eq!(
                lint_table(manifest, &format!("[{prefix}lints.{lint_group}]")),
                expected,
                "{} {lint_group} lints differ from the repository root",
                manifest.display(),
            );
        }
    }
}
