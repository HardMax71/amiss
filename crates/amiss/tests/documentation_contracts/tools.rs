#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions over the repository-owned tool bench"
)]

use std::collections::BTreeMap;
use std::fs;

use crate::support::repository_root;

/// tools.toml is the one authority for gate-tool versions: `name = "version"`
/// rows under its two tables, nothing else.
fn bench() -> BTreeMap<String, String> {
    let raw = fs::read_to_string(repository_root().join("tools.toml"))
        .expect("tools.toml is readable at the repository root");
    let mut versions = BTreeMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let (name, rest) = line
            .split_once(" = \"")
            .unwrap_or_else(|| panic!("tools.toml row is not name = \"version\": {line}"));
        let version = rest
            .strip_suffix('"')
            .unwrap_or_else(|| panic!("tools.toml row lacks its closing quote: {line}"));
        assert!(
            versions
                .insert(name.to_owned(), version.to_owned())
                .is_none(),
            "tools.toml declares {name} twice"
        );
    }
    assert!(versions.len() >= 10, "the bench lost tools: {versions:?}");
    versions
}

/// Every `tool:` pin anywhere under .github naming a declared tool must spell
/// the declared version, so no workflow can drift from tools.toml.
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
                if let Some(declared) = bench.get(name) {
                    assert_eq!(
                        version,
                        declared,
                        "{} pins {name}@{version} against tools.toml's {declared}",
                        path.display()
                    );
                }
            }
        }
    }
}

/// The composite carries no version of its own: it parses tools.toml and keys
/// its cache on the file's hash.
#[test]
fn the_tools_composite_reads_the_bench() {
    let raw = fs::read_to_string(repository_root().join(".github/actions/tools/action.yml"))
        .expect("the tools composite is readable");
    assert!(
        raw.contains("hashFiles('tools.toml')"),
        "the cache key must be the hash of tools.toml"
    );
    for (name, version) in bench() {
        assert!(
            !raw.contains(&format!("{name}@{version}")),
            "the composite hardcodes {name}@{version} beside the file it parses"
        );
    }
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

/// prek's floor in the hook config is the bench's prek, and the ratchet hooks
/// take their version from tools.toml rather than an inline spelling.
#[test]
fn the_hook_config_defers_to_the_bench() {
    let bench = bench();
    let raw = fs::read_to_string(repository_root().join(".pre-commit-config.yaml"))
        .expect("the hook config is readable");
    let floor = format!("minimum_prek_version: \"{}\"", bench["prek"]);
    assert!(
        raw.contains(&floor),
        "the prek floor does not match tools.toml: wanted {floor}"
    );
    assert_eq!(
        raw.matches("s/^similarity-rs = ").count(),
        2,
        "both ratchet hooks must read their pin out of tools.toml"
    );
    assert!(
        !raw.contains(&format!("similarity-rs {}", bench["similarity-rs"])),
        "a ratchet hook spells the similarity version inline"
    );
}

/// The gh-aw commands dispatcher runs with credentials, so its setup action
/// stays sha-pinned; the compiler emits a mutable tag and the pin is applied
/// by hand after every compile until upstream reads its own ledger.
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
