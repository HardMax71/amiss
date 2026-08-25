use std::fs;

use crate::support::{amiss, fixture, git, payload};

#[test]
fn policy_include_authors_one_row_and_previews_the_exact_staged_matches() {
    let (code, stdout, stderr) = amiss(&[
        "policy-include",
        "--path",
        "manual",
        "--suffix",
        ".txt",
        "--adapter",
        "rst",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        stdout,
        br#"{"adapter":"rst","kind":"tree","path":"manual","suffix":".txt"}
"#
    );

    let fx = fixture();
    let root = fx.root();
    fs::create_dir_all(root.join("manual/nested")).unwrap_or_default();
    fs::create_dir_all(root.join("manualish")).unwrap_or_default();
    fs::write(root.join("manual/a.txt"), "A\n").unwrap_or_default();
    fs::write(root.join("manual/nested/b.txt"), "B\n").unwrap_or_default();
    fs::write(root.join("manual/c.md"), "C\n").unwrap_or_default();
    fs::write(root.join("manualish/d.txt"), "D\n").unwrap_or_default();
    git(root, &["add", "."]);

    let (code, stdout, stderr) = amiss(&[
        "policy-include",
        "--path",
        "manual",
        "--suffix",
        ".txt",
        "--adapter",
        "rst",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--index",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        stdout,
        br#"["manual/a.txt","manual/nested/b.txt"]
"#,
        "the preview applies the production root and suffix boundaries"
    );

    let object = "0000000000000000000000000000000000000000";
    amiss_fixtures::index_file(root, &[(b"manual/\xff.txt", object)]).unwrap_or_default();
    let (code, stdout, stderr) = amiss(&[
        "policy-include",
        "--path",
        "manual",
        "--suffix",
        ".txt",
        "--adapter",
        "rst",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--index",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        stdout,
        br#"[{"bytes_hex":"6d616e75616c2fff2e747874"}]
"#,
        "raw Git paths keep the report's canonical bytes form"
    );
}

#[test]
fn repository_policy_includes_raises_and_weakening() {
    let fx = fixture();
    let root = fx.root();

    let strong_policy = r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"specs"}],"protected_inventory":["docs/guide.md"],"finding_dispositions":[{"finding_kind":"explicit-target-missing","disposition":"fail"}]}"#;
    fs::create_dir_all(root.join(".amiss")).unwrap_or_default();
    fs::create_dir_all(root.join("specs")).unwrap_or_default();
    fs::write(root.join(".amiss/scanner-policy.json"), strong_policy).unwrap_or_default();
    fs::write(root.join("specs/design.tex"), "included but unsupported\n").unwrap_or_default();
    fs::write(root.join("specs/design.rst"), "intrinsically unparsed\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "policy"]);
    let with_policy = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy","document_includes":[],"protected_inventory":["docs/guide.md"],"finding_dispositions":[]}"#,
    )
    .unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "weakened"]);
    let weakened = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &with_policy,
        "--candidate",
        &weakened,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "weakening is an unsuppressible fail");
    let payload = payload(&stdout);
    assert_eq!(payload["result"]["status"], "fail");
    assert!(
        payload["controls"]["base_repository_policy_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:"))
    );
    let rows: Vec<(String, String)> = payload["findings"]
        .as_array()
        .map(|findings| {
            findings
                .iter()
                .filter_map(|finding| {
                    Some((
                        finding["kind"].as_str()?.to_owned(),
                        finding["key_input"]["scope"]["rule_id"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(rows.contains(&(
        "policy-weakened".to_owned(),
        "policy/include-tree-removed".to_owned()
    )));
    assert!(rows.contains(&(
        "policy-weakened".to_owned(),
        "policy/disposition/explicit-target-missing".to_owned()
    )));
    let documents: Vec<(&str, &str)> = payload["documents"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| Some((row["path"].as_str()?, row["classification"].as_str()?)))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        documents.contains(&("specs/design.tex", "policy-included")),
        "the include is discovered without installing a parser: {documents:?}"
    );
    assert!(
        documents.contains(&("specs/design.rst", "structured-rst")),
        "reStructuredText is discovered without a policy include: {documents:?}"
    );
}

#[test]
fn a_raised_disposition_fails_a_passing_observe_run() {
    let fx = fixture();
    let root = fx.root();
    fs::create_dir_all(root.join(".amiss")).unwrap_or_default();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy","document_includes":[],"protected_inventory":[],"finding_dispositions":[{"finding_kind":"explicit-target-missing","disposition":"fail"}]}"#,
    )
    .unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "raise"]);
    let raised = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.candidate,
        "--candidate",
        &raised,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "the raise turns the missing target into fail");
    let payload = payload(&stdout);
    let missing = payload["findings"]
        .as_array()
        .and_then(|findings| {
            findings
                .iter()
                .find(|finding| finding["kind"] == "explicit-target-missing")
        })
        .cloned()
        .unwrap_or_default();
    assert_eq!(missing["effective_disposition"], "fail");
    assert_eq!(
        missing["configured_disposition"], "fail",
        "configured is the value after the repository and floor steps"
    );
    assert_eq!(missing["policy_trace"][0]["before"], "record");
    assert_eq!(missing["policy_trace"][0]["after"], "warn");
    assert_eq!(
        missing["policy_trace"][1]["rule_id"], "repository/explicit-target-missing",
        "the repository step follows the built-in step"
    );
    assert_eq!(missing["policy_trace"][1]["before"], "warn");
    assert_eq!(missing["policy_trace"][1]["after"], "fail");
}

#[test]
fn an_invalid_policy_is_fatal_with_unavailable_controls() {
    let fx = fixture();
    let root = fx.root();
    fs::create_dir_all(root.join(".amiss")).unwrap_or_default();
    fs::write(root.join(".amiss/scanner-policy.json"), "{not json").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "broken"]);
    let broken = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("touch.md"), "later\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "later"]);
    let later = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &broken,
        "--candidate",
        &later,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    let payload = payload(&stdout);
    assert_eq!(payload["controls"]["status"], "unavailable");
    assert_eq!(
        payload["controls"]["reasons"][0],
        "invalid-repository-policy"
    );
    let codes: Vec<&str> = payload["errors"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["code"].as_str()).collect())
        .unwrap_or_default();
    assert!(codes.contains(&"CONFIGURATION_INVALID"));
    assert!(
        payload["errors"][0]["path"] == ".amiss/scanner-policy.json"
            || payload["errors"][1]["path"] == ".amiss/scanner-policy.json"
    );
}

#[test]
fn reserved_directives_are_boundary_incomplete_with_full_details() {
    let fx = fixture();
    let root = fx.root();
    fs::write(
        root.join("docs/governed.md"),
        "A claim [here][amiss:claim] and [fine](guide.md).\n\n\
         [amiss:claim]: ./subject.md \"claim\"\n\
         [amiss:claim]: ./subject.md \"claim\"\n",
    )
    .unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "governed"]);
    let governed = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.candidate,
        "--candidate",
        &governed,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2, "governed syntax exits two under either profile");
    let payload = payload(&stdout);
    assert_eq!(payload["result"]["status"], "incomplete");
    assert_eq!(payload["result"]["complete"], false);
    assert!(
        !payload["documents"]
            .as_array()
            .unwrap_or(&Vec::new())
            .is_empty(),
        "boundary-incomplete keeps complete detail arrays"
    );
    assert_eq!(payload["errors"][0]["code"], "UNSUPPORTED_CAPABILITY");
    assert_eq!(payload["errors"][0]["path"], "docs/governed.md");
    assert_eq!(payload["errors"][0]["phase"], "policy");

    let finding = payload["findings"]
        .as_array()
        .and_then(|findings| {
            findings
                .iter()
                .find(|finding| finding["kind"] == "unsupported-capability")
        })
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        finding["key_input"]["scope"]["rule_id"],
        "unsupported/governed-claim"
    );
    assert_eq!(
        finding["key_input"]["scope"]["control_path"],
        "docs/governed.md"
    );
    assert_eq!(
        finding["aggregation"]["member_count"], 2,
        "two nodes, one duplicated source"
    );
    assert_eq!(finding["effective_disposition"], "fail");
    let sources = &finding["candidate_fact"]["evidence"]["candidate_control_state"]["sources"];
    assert_eq!(
        sources.as_array().map(Vec::len),
        Some(1),
        "equal digests group"
    );
    assert_eq!(sources[0]["multiplicity"], 2);

    let suppressed: Vec<&str> = payload["observations"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["candidate"]["document"].as_str())
                .filter(|document| *document == "docs/governed.md")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        suppressed.len(),
        1,
        "only the ordinary link is an observation; the governed consumer is suppressed"
    );
}

/// When no external controls are supplied, the report must say so in the exact
/// vocabulary reserved for that: `none`, with no trust source and no digest.
/// The row this pins is not the absence, it is the labeling. A report that
/// described an unsupplied floor as anything but none, or dressed the local
/// process up as a verified sandbox, would be lending itself trust nobody
/// granted, and every consumer downstream of the report would inherit the lie.
#[test]
fn unsupplied_controls_report_none_and_claim_no_trust() {
    let fx = fixture();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0);
    let payload = payload(&stdout);
    assert_eq!(
        payload["result"]["complete"], true,
        "none is a complete answer"
    );

    let controls = &payload["controls"];
    for control in ["organization_floor", "debt_snapshot", "waiver_bundle"] {
        assert_eq!(controls[control]["status"], "none", "{control}");
        assert_eq!(controls[control]["trust_source"], "none", "{control}");
        assert!(controls[control]["digest"].is_null(), "{control}");
    }
    assert_eq!(controls["execution_constraint"]["status"], "none");
    assert_eq!(controls["trusted_time_source"]["status"], "none");
    assert!(controls["base_repository_policy_digest"].is_null());
    assert!(controls["candidate_repository_policy_digest"].is_null());

    let sandbox = &controls["sandbox"];
    assert_eq!(sandbox["assurance"], "self-asserted");
    assert_eq!(sandbox["enforcement_source"], "local-process");
    assert!(
        sandbox["verification"].is_null(),
        "a local process does not get to claim it was verified"
    );
}

/// A tree include may bind one built-in grammar: the bound `.txt` parses as
/// reStructuredText, its broken `:doc:` blocks, and the documents row says
/// which adapter read it, while an unbound include stays inert.
#[test]
fn a_bound_include_parses_under_the_named_grammar() {
    let fx = fixture();
    let root = fx.root();
    let bound = r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"tree","path":"manual"},{"kind":"document","path":"notes.tex"},{"adapter":"markdown","kind":"document","path":"tour.guide"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    fs::create_dir_all(root.join(".amiss")).unwrap_or_default();
    fs::create_dir_all(root.join("manual")).unwrap_or_default();
    fs::write(root.join(".amiss/scanner-policy.json"), bound).unwrap_or_default();
    fs::write(root.join("notes.tex"), "inert include\n").unwrap_or_default();
    fs::write(root.join("tour.guide"), "# Tour\n").unwrap_or_default();
    fs::write(root.join("manual/guide.txt"), "start\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "bound"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("manual/guide.txt"), "see :doc:`gone`\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "broken"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "enforce",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "the bound grammar finds the broken :doc:");
    let payload = payload(&stdout);
    assert!(
        payload["findings"].as_array().is_some_and(|rows| {
            rows.iter().any(|row| {
                row["kind"] == "explicit-target-missing"
                    && row["key_input"]["scope"]["document"] == "manual/guide.txt"
            })
        }),
        "{}",
        payload["findings"]
    );
    let documents = payload["documents"].as_array().cloned().unwrap_or_default();
    let guide = documents
        .iter()
        .find(|row| row["path"] == "manual/guide.txt")
        .unwrap();
    assert_eq!(guide["classification"], "policy-included");
    assert_eq!(guide["candidate"]["adapter_id"], "rst");
    let notes = documents
        .iter()
        .find(|row| row["path"] == "notes.tex")
        .unwrap();
    assert_eq!(notes["classification"], "policy-included");
    assert_eq!(
        notes["candidate"]["adapter_id"],
        serde_json::Value::Null,
        "an unbound include still installs no parser"
    );
    let tour = documents
        .iter()
        .find(|row| row["path"] == "tour.guide")
        .unwrap();
    assert_eq!(tour["classification"], "policy-included");
    assert_eq!(
        tour["candidate"]["adapter_id"], "markdown",
        "an exact document binding answers without a tree"
    );
}

#[test]
fn a_suffix_binding_selects_only_the_exact_tail_and_keeps_native_precedence() {
    let fx = fixture();
    let root = fx.root();
    let selected = r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"tree","path":"manual","suffix":".txt"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    for directory in [".amiss", "manual/nested", "manual-old"] {
        fs::create_dir_all(root.join(directory)).unwrap_or_default();
    }
    for (path, body) in [
        (".amiss/scanner-policy.json", selected),
        ("manual/guide.txt", "Guide\n=====\n\nsteady\n"),
        ("manual/nested/detail.txt", "Detail\n======\n"),
        ("manual/guide.TXT", "not selected\n"),
        ("manual/guide.txt.bak", "not selected\n"),
        ("manual-old/outside.txt", "not selected\n"),
        ("manual/llms.txt", "plain advisory\n"),
        ("manual/README.md", "# Native Markdown\n"),
        ("manual/tool.py", "print('not selected')\n"),
    ] {
        fs::write(root.join(path), body).unwrap_or_default();
    }
    fs::write(root.join("manual/image.bin"), [0, 0xff, 0x80]).unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "suffix base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("manual/guide.txt"), "see :doc:`gone`\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "suffix candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "enforce",
        "--format",
        "json",
    ]);
    assert_eq!(
        code, 1,
        "the selected RST document carries its missing target"
    );
    let payload = payload(&stdout);
    assert!(payload["findings"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["kind"] == "explicit-target-missing"
                && row["key_input"]["scope"]["document"] == "manual/guide.txt"
        })
    }));
    let documents = payload["documents"].as_array().cloned().unwrap_or_default();
    let row = |path: &str| documents.iter().find(|row| row["path"] == path);
    assert_eq!(
        row("manual/guide.txt").and_then(|row| row["candidate"]["adapter_id"].as_str()),
        Some("rst")
    );
    assert_eq!(
        row("manual/nested/detail.txt").and_then(|row| row["candidate"]["adapter_id"].as_str()),
        Some("rst")
    );
    assert_eq!(
        row("manual/llms.txt").and_then(|row| row["classification"].as_str()),
        Some("plain-advisory"),
        "a built-in classification wins even when its path matches the selector"
    );
    assert_eq!(
        row("manual/README.md").and_then(|row| row["classification"].as_str()),
        Some("structured-markdown")
    );
    for outside in [
        "manual/guide.TXT",
        "manual/guide.txt.bak",
        "manual-old/outside.txt",
        "manual/image.bin",
        "manual/tool.py",
    ] {
        assert!(
            row(outside).is_none(),
            "{outside} must remain outside the document set"
        );
    }
}

/// Keeping the include while dropping its binding stops reading the tree, so
/// it is policy weakening under its own rule.
#[test]
fn dropping_a_binding_is_policy_weakening() {
    let fx = fixture();
    let root = fx.root();
    fs::create_dir_all(root.join(".amiss")).unwrap_or_default();
    fs::create_dir_all(root.join("manual")).unwrap_or_default();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"tree","path":"manual"}],"protected_inventory":[],"finding_dispositions":[]}"#,
    )
    .unwrap_or_default();
    fs::write(root.join("manual/guide.txt"), "steady\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "bound"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"manual"}],"protected_inventory":[],"finding_dispositions":[]}"#,
    )
    .unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "unbound"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "binding removal is an unsuppressible fail");
    let payload = payload(&stdout);
    assert!(
        payload["findings"].as_array().is_some_and(|rows| {
            rows.iter().any(|row| {
                row["kind"] == "policy-weakened"
                    && row["key_input"]["scope"]["rule_id"] == "policy/include-binding-removed"
            })
        }),
        "{}",
        payload["findings"]
    );
}

#[test]
fn a_broader_replacement_does_not_erase_the_suffix_selector_identity() {
    let fx = fixture();
    let root = fx.root();
    fs::create_dir_all(root.join(".amiss")).unwrap_or_default();
    fs::create_dir_all(root.join("manual")).unwrap_or_default();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"tree","path":"manual","suffix":".txt"}],"protected_inventory":[],"finding_dispositions":[]}"#,
    )
    .unwrap_or_default();
    fs::write(root.join("manual/guide.txt"), "steady\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "selected"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"tree","path":"manual"}],"protected_inventory":[],"finding_dispositions":[]}"#,
    )
    .unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "broader"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(
        code, 1,
        "selector replacement is an unsuppressible weakening"
    );
    let payload = payload(&stdout);
    assert!(payload["findings"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["kind"] == "policy-weakened"
                && row["key_input"]["scope"]["rule_id"] == "policy/include-suffix-removed"
                && row["key_input"]["scope"]["control_path"] == "manual"
        })
    }));
}

/// A fragment into a bound target answers under the bound grammar: the rst
/// heading publishes its docutils id, and a wrong fragment is a missing
/// target rather than a code-fragment refusal.
#[test]
fn an_anchor_into_a_bound_target_resolves_under_its_grammar() {
    let fx = fixture();
    let root = fx.root();
    fs::create_dir_all(root.join(".amiss")).unwrap_or_default();
    fs::create_dir_all(root.join("manual")).unwrap_or_default();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"tree","path":"manual"}],"protected_inventory":[],"finding_dispositions":[]}"#,
    )
    .unwrap_or_default();
    fs::write(root.join("manual/guide.txt"), "Guide\n=====\n\nsteady\n").unwrap_or_default();
    fs::write(root.join("README.md"), "# R\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "bound"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join("README.md"),
        "# R\n\n[ok](manual/guide.txt#guide)\n[gone](manual/guide.txt#missing)\n",
    )
    .unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "anchors"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "enforce",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "the wrong fragment blocks under the bound grammar");
    let payload = payload(&stdout);
    let into_guide = payload["findings"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|row| {
                    row["key_input"]["scope"]["normalized_target_intent"]["path"]
                        == "manual/guide.txt"
                })
                .count()
        })
        .unwrap_or_default();
    assert_eq!(
        into_guide, 1,
        "the published rst id resolves without a row of any kind and the absent one is the only miss: {}",
        payload["findings"]
    );
}
