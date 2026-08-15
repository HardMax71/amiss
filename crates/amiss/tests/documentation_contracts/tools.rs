#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions over the repository-owned tool bench"
)]

use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use crate::support::repository_root;

/// The workspace.metadata.tools tables in the root manifest.
fn bench() -> BTreeMap<String, String> {
    let raw = fs::read_to_string(repository_root().join("Cargo.toml"))
        .expect("the root manifest is readable");
    let manifest: toml::Table = raw.parse().expect("the root manifest parses as TOML");
    let tables = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("metadata"))
        .and_then(|metadata| metadata.get("tools"))
        .and_then(toml::Value::as_table)
        .expect("the manifest declares workspace.metadata.tools");
    let mut versions = BTreeMap::new();
    for (table_name, table) in tables {
        let rows = table
            .as_table()
            .unwrap_or_else(|| panic!("tools.{table_name} is not a table"));
        for (name, value) in rows {
            let version = value
                .as_str()
                .unwrap_or_else(|| panic!("tools.{table_name}.{name} is not a string"));
            assert!(
                versions.insert(name.clone(), version.to_owned()).is_none(),
                "the tools tables declare {name} twice"
            );
        }
    }
    assert!(versions.len() >= 10, "the bench lost tools: {versions:?}");
    versions
}

/// Cargo exposes the same tool table that the manifest parser sees.
#[test]
fn cargo_metadata_and_the_manifest_agree() {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(repository_root())
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "cargo metadata refuses the manifest"
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata emits JSON");
    let tables = metadata
        .pointer("/metadata/tools")
        .and_then(serde_json::Value::as_object)
        .expect("cargo exposes workspace.metadata.tools");
    let mut projected = BTreeMap::new();
    for (table_name, table) in tables {
        let rows = table
            .as_object()
            .unwrap_or_else(|| panic!("metadata.tools.{table_name} is not an object"));
        for (name, value) in rows {
            let version = value
                .as_str()
                .unwrap_or_else(|| panic!("metadata.tools.{table_name}.{name} is not a string"));
            assert!(
                projected.insert(name.clone(), version.to_owned()).is_none(),
                "cargo metadata declares {name} twice"
            );
        }
    }
    assert_eq!(projected, bench(), "cargo projects a different tool bench");
}

/// Every `tool:` pin in a workflow naming a declared tool must spell the
/// declared version.
#[test]
fn workflow_tool_pins_match_the_bench() {
    let bench = bench();
    let workflows = repository_root().join(".github/workflows");
    for entry in fs::read_dir(&workflows).expect("workflows directory is readable") {
        let path = entry.expect("workflow entry is readable").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("yml") {
            continue;
        }
        let raw = fs::read_to_string(&path).expect("workflow is readable");
        for line in raw.lines() {
            let Some(tools) = line.trim().strip_prefix("tool: ") else {
                continue;
            };
            for spec in tools.split(',') {
                let Some((name, version)) = spec.trim().split_once('@') else {
                    continue;
                };
                if version.starts_with("${{") {
                    continue;
                }
                if let Some(declared) = bench.get(name) {
                    assert_eq!(
                        version,
                        declared,
                        "{} pins {name}@{version} against the manifest's {declared}",
                        path.display()
                    );
                }
            }
        }
    }
}

/// Tool consumers ask cargo for the manifest metadata directly; the composite
/// holds no version or cache hash of its own.
#[test]
fn the_tools_composite_reads_the_bench() {
    let root = repository_root();
    assert!(
        !root.join("scripts/tools.sh").exists(),
        "the cargo metadata shim has been recreated"
    );
    let raw = fs::read_to_string(root.join(".github/actions/tools/action.yml"))
        .expect("the tools composite is readable");
    assert!(
        raw.contains("cargo metadata --locked --format-version 1 --no-deps --offline")
            && raw.contains(".prebuilt | to_entries")
            && raw.contains(".source | to_entries")
            && raw.contains("hashFiles('Cargo.toml')")
            && raw.contains("runner.arch")
            && raw.contains("fallback: none"),
        "the composite must query cargo and use GitHub's manifest hash"
    );
    assert!(
        !raw.contains("scripts/tools.sh"),
        "the composite calls the removed metadata shim"
    );
    for (name, version) in bench() {
        assert!(
            !raw.contains(&format!("{name}@{version}")),
            "the composite hardcodes {name}@{version} beside the manifest"
        );
    }
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml"))
        .expect("the CI workflow is readable");
    assert!(
        ci.contains("cargo metadata --locked --format-version 1 --no-deps --offline")
            && ci.contains(r#".metadata.tools.prebuilt["cargo-nextest"]"#)
            && ci.contains("fallback: none"),
        "the platform lane must query cargo for the nextest pin"
    );
    assert!(
        !ci.contains("scripts/tools.sh"),
        "the platform lane calls the removed metadata shim"
    );
}

/// The agent lanes install through the composite, never through their own pins.
#[test]
fn the_agent_lanes_ride_the_composite() {
    for lane in ["agent-review", "agent-mention", "agent-triage"] {
        for suffix in ["md", "lock.yml"] {
            let path = repository_root().join(format!(".github/workflows/{lane}.{suffix}"));
            let raw = fs::read_to_string(&path).expect("lane file is readable");
            assert!(
                raw.contains("./.github/actions/tools"),
                "{lane}.{suffix} does not use the tools composite"
            );
            assert!(
                !raw.contains("cargo-nextest@"),
                "{lane}.{suffix} pins nextest beside the composite"
            );
        }
    }
}

/// The hook config defers to the bench: the prek floor matches, and the
/// ratchet hooks ask cargo instead of spelling a version.
#[test]
fn the_hook_config_defers_to_the_bench() {
    let bench = bench();
    let raw = fs::read_to_string(repository_root().join(".pre-commit-config.yaml"))
        .expect("the hook config is readable");
    let floor = format!("minimum_prek_version: \"{}\"", bench["prek"]);
    assert!(
        raw.contains(&floor),
        "the prek floor does not match the manifest: wanted {floor}"
    );
    assert_eq!(
        raw.matches("cargo metadata --locked --format-version 1 --no-deps --offline")
            .count(),
        2,
        "both ratchet hooks must ask cargo for their pin"
    );
    assert_eq!(
        raw.matches(".metadata.tools.source").count(),
        2,
        "both ratchet hooks must read the source tool table"
    );
    assert!(
        !raw.contains("scripts/tools.sh"),
        "a ratchet hook calls the removed metadata shim"
    );
    assert!(
        !raw.contains(&format!("similarity-rs {}", bench["similarity-rs"])),
        "a ratchet hook spells the similarity version inline"
    );
}

/// The credentialed dispatcher reaches its setup action by sha; the compiler
/// re-emits a mutable tag on every compile until upstream reads its ledger.
#[test]
fn the_dispatcher_setup_action_is_sha_pinned() {
    let raw = fs::read_to_string(repository_root().join(".github/workflows/agentic_commands.yml"))
        .expect("the dispatcher is readable");
    let uses = raw
        .lines()
        .find(|line| line.contains("gh-aw-actions/setup@"))
        .expect("the dispatcher uses the setup action");
    let reference = uses
        .split("setup@")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .expect("the setup reference is readable");
    assert!(
        reference.len() == 40 && reference.chars().all(|symbol| symbol.is_ascii_hexdigit()),
        "the dispatcher reaches its setup action by mutable reference: {reference}"
    );
}

/// Every agent lane's copilot version is one fact: the env literal, both
/// engine blocks, and every installer argument in the lock spell it alike,
/// and the three lanes spell the same version.
#[test]
fn the_agent_lanes_spell_one_copilot_version() {
    let root = repository_root();
    let mut versions: Vec<String> = Vec::new();
    for lane in ["agent-review", "agent-mention", "agent-triage"] {
        let source = fs::read_to_string(root.join(format!(".github/workflows/{lane}.md")))
            .expect("the lane is readable");
        let spellings: Vec<&str> = source
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                ["version: \"", "COPILOT_CLI_VERSION: \""]
                    .iter()
                    .find_map(|key| trimmed.strip_prefix(key))
            })
            .filter_map(|tail| tail.strip_suffix('"'))
            .collect();
        let version = spellings.first().copied().expect("the lane pins a version");
        assert_eq!(
            spellings,
            vec![version; 3],
            "{lane} spells more than one copilot version"
        );
        let lock = fs::read_to_string(root.join(format!(".github/workflows/{lane}.lock.yml")))
            .expect("the lock is readable");
        assert_eq!(
            lock.matches(&format!("install_copilot_cli.sh\" {version}"))
                .count(),
            lock.matches("install_copilot_cli.sh").count(),
            "a {lane} lock installer call carries another copilot version"
        );
        assert!(
            lock.contains(&format!("COPILOT_CLI_VERSION: {version}")),
            "the {lane} lock's workflow env does not carry the pinned version"
        );
        versions.push(version.to_owned());
    }
    versions.dedup();
    assert_eq!(
        versions.len(),
        1,
        "the lanes disagree on the copilot version"
    );
}
