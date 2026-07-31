use crate::support::{amiss, fixture, payload};

/// The ramp between observe and enforce: the built-in enforce column, then a
/// trace-recorded lowering of pre-existing findings, so a backlog is carried
/// visibly while an introduced break still blocks.
#[test]
fn enforce_introduced_blocks_new_findings_and_carries_the_backlog() {
    let run = |profile: &str, fx: &amiss_fixtures::CommitPair| {
        amiss(&[
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
            profile,
            "--format",
            "json",
        ])
    };

    let carried = amiss_fixtures::commit_pair(
        &[
            ("README.md", "[gone](missing.md)\n"),
            ("notes.md", "steady\n"),
        ],
        &[("notes.md", "changed\n")],
    )
    .unwrap();
    let (enforce_code, _out, _err) = run("enforce", &carried);
    assert_eq!(enforce_code, 1, "enforce blocks the backlog");

    let (ramp_code, out, stderr) = run("enforce-introduced", &carried);
    assert_eq!((ramp_code, stderr.as_str()), (0, ""), "the ramp carries it");
    let body = payload(&out);
    assert_eq!(
        body.pointer("/controls/profile").unwrap(),
        "enforce-introduced"
    );
    let findings = body.get("findings").unwrap().as_array().unwrap();
    let carried_row = findings
        .iter()
        .find(|row| row.get("kind").unwrap() == "explicit-target-missing")
        .unwrap();
    assert_eq!(carried_row.get("attribution").unwrap(), "pre-existing");
    assert_eq!(carried_row.get("configured_disposition").unwrap(), "fail");
    assert_eq!(carried_row.get("effective_disposition").unwrap(), "warn");
    let lowered = carried_row
        .get("policy_trace")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|step| {
            step.get("rule_id")
                .and_then(|rule| rule.as_str())
                .is_some_and(|rule| rule.ends_with("/enforce-introduced"))
        });
    assert!(lowered, "the lowering leaves a trace step: {carried_row}");

    let introduced = fixture();
    let (code, out, _err) = run("enforce-introduced", &introduced);
    assert_eq!(code, 1, "an introduced break still blocks");
    let body = payload(&out);
    let findings = body.get("findings").unwrap().as_array().unwrap();
    let introduced_row = findings
        .iter()
        .find(|row| row.get("kind").unwrap() == "explicit-target-missing")
        .unwrap();
    assert_eq!(introduced_row.get("attribution").unwrap(), "introduced");
    assert_eq!(introduced_row.get("effective_disposition").unwrap(), "fail");
}
