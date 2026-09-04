use crate::support::{amiss, claim_fixture, fixture, payload};

/// The SARIF lane is a projection, so every claim it makes is checked against
/// the canonical report from the same evaluation rather than asserted twice.
#[test]
fn the_sarif_projection_mirrors_the_report_and_stays_deterministic() {
    let fx = fixture();
    let args = [
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
        "enforce",
        "--format",
        "sarif",
    ];
    let (code, first, stderr) = amiss(&args);
    assert_eq!((code, stderr.as_str()), (1, ""));
    let (again, second, _stderr) = amiss(&args);
    assert_eq!(again, 1);
    assert_eq!(first, second, "identical inputs, identical SARIF bytes");

    let log: serde_json::Value = serde_json::from_slice(&first).unwrap();
    let mut canonical = serde_json::to_vec(&log).unwrap();
    canonical.push(b'\n');
    assert_eq!(first, canonical, "SARIF remains one canonical JSON line");
    assert_eq!(log.get("version").unwrap(), "2.1.0");
    let run = log.pointer("/runs/0").unwrap();
    assert_eq!(run.pointer("/tool/driver/name").unwrap(), "amiss");
    assert_eq!(
        run.pointer("/invocations/0/executionSuccessful").unwrap(),
        true
    );
    assert_eq!(run.pointer("/invocations/0/exitCode").unwrap(), 1);

    let wire_args = [
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
        "enforce",
        "--format",
        "json",
    ];
    let (_code, wire, _stderr) = amiss(&wire_args);
    let wire_payload = payload(&wire);
    let findings = wire_payload["findings"].as_array().unwrap();
    let results = run["results"].as_array().unwrap();
    assert_eq!(results.len(), findings.len());
    let rules = run
        .pointer("/tool/driver/rules")
        .unwrap()
        .as_array()
        .unwrap();
    for (result, finding) in results.iter().zip(findings) {
        assert_eq!(result.get("ruleId"), finding.get("kind"));
        assert_eq!(
            result
                .get("partialFingerprints")
                .and_then(|prints| prints.get("amissFindingKey/v1")),
            finding.get("finding_key"),
        );
        assert_eq!(
            result
                .get("message")
                .and_then(|message| message.get("text")),
            finding.get("description")
        );
        let level = result.get("level").unwrap().as_str().unwrap();
        let expected = match finding
            .get("effective_disposition")
            .unwrap()
            .as_str()
            .unwrap()
        {
            "fail" => "error",
            "warn" => "warning",
            _ => "note",
        };
        assert_eq!(level, expected);
        let rule_id = result.get("ruleId").unwrap();
        assert!(rules.iter().any(|rule| rule.get("id") == Some(rule_id)));
    }
    let blocking = results
        .iter()
        .find(|result| result.get("level").unwrap() == "error")
        .unwrap();
    assert!(
        blocking
            .pointer("/locations/0/physicalLocation/region/startLine")
            .is_some(),
        "the blocking finding carries its candidate location"
    );
}

/// The rules table lists exactly the kinds the findings carry, and every
/// result's ruleIndex points back at its own rule rather than a neighbour.
#[test]
fn sarif_rules_are_exactly_the_present_kinds_and_indexed() {
    let fx = fixture();
    let (_code, stdout, _stderr) = amiss(&[
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
        "enforce",
        "--format",
        "sarif",
    ]);
    let log: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let run = log.pointer("/runs/0").unwrap();
    let rules = run
        .pointer("/tool/driver/rules")
        .unwrap()
        .as_array()
        .unwrap();
    let results = run.get("results").unwrap().as_array().unwrap();
    let rule_ids: Vec<&str> = rules
        .iter()
        .map(|rule| rule.get("id").unwrap().as_str().unwrap())
        .collect();
    let mut present: Vec<&str> = results
        .iter()
        .map(|result| result.get("ruleId").unwrap().as_str().unwrap())
        .collect();
    present.sort_unstable();
    present.dedup();
    assert_eq!(
        rule_ids.len(),
        present.len(),
        "rules are exactly the kinds present: {rule_ids:?}"
    );
    for kind in &present {
        assert!(rule_ids.contains(kind), "{kind} has a rule");
    }
    for result in results {
        let rule_index =
            usize::try_from(result.get("ruleIndex").unwrap().as_i64().unwrap()).unwrap();
        assert_eq!(
            rules.get(rule_index).unwrap().get("id"),
            result.get("ruleId"),
            "ruleIndex points at its own rule"
        );
    }
}

/// A path that needs escaping is exactly the case the artifact URI encoder
/// exists for, so one hostile filename pins every byte of its output.
#[test]
fn a_sarif_artifact_uri_is_percent_encoded() {
    let fx = amiss_fixtures::commit_pair(
        &[("docs/a b%c#d.md", "# A\n")],
        &[("docs/a b%c#d.md", "# A\n\n[gone](missing.md)\n")],
    )
    .unwrap();
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
        "enforce",
        "--format",
        "sarif",
    ]);
    assert_eq!(code, 1);
    let log: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let results = log.pointer("/runs/0/results").unwrap().as_array().unwrap();
    let located = results
        .iter()
        .find_map(|result| result.pointer("/locations/0/physicalLocation/artifactLocation/uri"))
        .unwrap();
    assert_eq!(located, "docs/a%20b%25c%23d.md");
}

/// A rejected machine invocation answers on the channel it was asked on.
#[test]
fn a_sarif_refusal_is_still_sarif() {
    let (code, refusal, stderr) = amiss(&["check", "--format", "sarif"]);
    assert_eq!((code, stderr.as_str()), (2, ""));
    let refusal: serde_json::Value = serde_json::from_slice(&refusal).unwrap();
    assert_eq!(
        refusal
            .pointer("/runs/0/invocations/0/executionSuccessful")
            .unwrap(),
        false,
    );
    assert!(
        refusal
            .pointer("/runs/0/invocations/0/toolExecutionNotifications/0/descriptor/id")
            .is_some(),
        "the refusal names its codes in the notification rows"
    );
}

/// A claim finding rides the projection like any other kind: its rule row
/// carries the fixed meaning, and the result points at the definition line.
#[test]
fn a_claim_finding_carries_its_rule_into_sarif() {
    let fx = claim_fixture();
    let (code, stdout, stderr) = amiss(&[
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
        "enforce",
        "--format",
        "sarif",
    ]);
    assert_eq!((code, stderr.as_str()), (1, ""));
    let log: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let run = log.pointer("/runs/0").unwrap();
    let result = run
        .get("results")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result.get("ruleId") == Some(&serde_json::json!("claim-broken")))
        .unwrap();
    assert_eq!(result.get("level").unwrap(), "error");
    assert_eq!(
        result
            .pointer("/locations/0/physicalLocation/artifactLocation/uri")
            .unwrap(),
        "docs/claims.md"
    );
    assert_eq!(
        result
            .pointer("/locations/0/physicalLocation/region/startLine")
            .unwrap(),
        3,
        "the result points at the reference definition, not the consumer"
    );
    let rule = run
        .pointer("/tool/driver/rules")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .find(|rule| rule.get("id") == Some(&serde_json::json!("claim-broken")))
        .unwrap();
    assert!(
        rule.pointer("/shortDescription/text")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|text| text.contains("no longer says what the document claims")),
        "the rule carries the fixed meaning sentence"
    );
}

/// A finding's wire fix rides into SARIF as the byte region to delete and
/// the replacement text, and rows without one carry no fixes array.
#[test]
fn a_claim_fix_projects_as_a_sarif_fix() {
    let fx = claim_fixture();
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
        "enforce",
        "--format",
        "sarif",
    ]);
    assert_eq!(code, 1);
    let log: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let results = log.pointer("/runs/0/results").unwrap().as_array().unwrap();
    let claim = results
        .iter()
        .find(|result| result.get("ruleId") == Some(&serde_json::json!("claim-broken")))
        .unwrap();
    let (_wire_code, wire, _stderr) = amiss(&[
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
        "enforce",
        "--format",
        "json",
    ]);
    let wire_fix = payload(&wire)["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "claim-broken")
        .unwrap()["fix"]
        .clone();
    let replacement = claim
        .pointer("/fixes/0/artifactChanges/0/replacements/0")
        .unwrap();
    assert_eq!(
        replacement.pointer("/insertedContent/text"),
        wire_fix.get("replacement"),
        "the projection mirrors the wire"
    );
    assert_eq!(
        replacement.pointer("/deletedRegion/byteOffset"),
        wire_fix.pointer("/span/start_byte"),
    );
    let length = wire_fix["span"]["end_byte"].as_u64().unwrap()
        - wire_fix["span"]["start_byte"].as_u64().unwrap();
    assert_eq!(
        replacement
            .pointer("/deletedRegion/byteLength")
            .and_then(serde_json::Value::as_u64),
        Some(length),
    );
    assert_eq!(
        claim.pointer("/fixes/0/description/text"),
        wire_fix.get("description"),
    );
    for result in results {
        if result.get("ruleId") != Some(&serde_json::json!("claim-broken")) {
            assert!(result.get("fixes").is_none(), "{result}");
        }
    }
}
