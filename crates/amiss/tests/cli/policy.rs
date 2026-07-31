use std::fs;

use crate::support::{amiss, fixture, git, payload};

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
