#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned documentation and fixtures"
)]

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use crate::support::repository_root;

fn assert_action_feedback_contract(dispatcher: &str, runtime: &str) {
    assert_eq!(
        runtime.matches("$p.feedback.items[:10][]").count(),
        2,
        "both the summary and annotations must share the combined ten-item display window"
    );
    assert!(
        runtime.contains("select(.action == \"fix\" and .annotation != null)"),
        "annotations must come only from displayed Fix items"
    );
    assert!(
        runtime.contains("$p.errors[:10][]"),
        "an unavailable feedback projection must cap error annotations"
    );
    assert!(
        !runtime.contains(".payload.findings") && !runtime.contains("$p.findings"),
        "the Action must consume the feedback projection instead of raw findings"
    );
    for presentation_contract in [
        "$p.feedback.existing_count",
        "amiss \\($p.result.status): scan failed",
        "(($p.feedback.items | length) - 10",
        "tojson | html",
        "<code>bytes ",
    ] {
        assert!(
            runtime.contains(presentation_contract),
            "the Action presentation contract is missing {presentation_contract}"
        );
    }
    for source in [&dispatcher, &runtime] {
        assert!(
            source.contains(
                "description: emit candidate-located displayed Fixes and scan errors as file annotations"
            )
        );
    }
}

fn action_jq_filter<'a>(runtime: &'a str, opening: &str, closing: &str) -> &'a str {
    runtime
        .split_once(opening)
        .and_then(|(_before, tail)| tail.split_once(closing))
        .map(|(filter, _after)| filter)
        .expect("Action jq filter is extractable")
}

fn run_action_jq(filter: &str, payload: &serde_json::Value) -> String {
    let mut child = Command::new("jq")
        .args(["-r", filter])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the Action runtime dependency jq is available");
    child
        .stdin
        .take()
        .expect("jq stdin is piped")
        .write_all(payload.to_string().as_bytes())
        .expect("the test payload reaches jq");
    let output = child.wait_with_output().expect("jq completes");
    assert!(
        output.status.success(),
        "jq rejected the Action filter: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("jq emits UTF-8")
        .replace("\r\n", "\n")
}

fn action_filters(runtime: &str) -> (&str, &str) {
    (
        action_jq_filter(
            runtime,
            "\n          jq -r '\n",
            "\n          ' \"$report\"",
        ),
        action_jq_filter(runtime, "\n        jq -r '\n", "\n        ' \"$REPORT\""),
    )
}

fn action_annotation(path: &str) -> serde_json::Value {
    serde_json::json!({
        "path": path,
        "span": {
            "start_byte": 0,
            "end_byte": 2,
            "start_line": 1,
            "end_line": 1,
            "start_column": 1,
            "end_column": 3
        }
    })
}

fn action_feedback_item(
    action: &str,
    target: &serde_json::Value,
    location_count: usize,
    disposition: &str,
    annotation: &serde_json::Value,
) -> serde_json::Value {
    let kind = if action == "fix" {
        "explicit-target-missing"
    } else {
        "dependency-changed-subject-unchanged"
    };
    serde_json::json!({
        "action": action,
        "target": target,
        "finding_kinds": [kind],
        "location_count": location_count,
        "effective_disposition": disposition,
        "annotation": annotation
    })
}

fn available_action_payload() -> serde_json::Value {
    let mut items = Vec::new();
    for index in 0_usize..8 {
        let target = match index {
            0 => serde_json::json!("docs/</code>`x&%\n::error::forged.md"),
            1 => serde_json::json!({ "bytes_hex": "ff" }),
            _ => serde_json::json!(format!("docs/target-{index}.md")),
        };
        let path = if index == 0 {
            "docs/a%:,\r\n.md".to_owned()
        } else {
            format!("docs/fix-{index}.md")
        };
        let annotation = action_annotation(&path);
        items.push(action_feedback_item(
            "fix",
            &target,
            index.saturating_add(2),
            if index == 0 { "fail" } else { "warn" },
            &annotation,
        ));
    }
    items.push(action_feedback_item(
        "fix",
        &serde_json::json!("null-annotation-target.md"),
        1,
        "warn",
        &serde_json::Value::Null,
    ));
    for target in ["check-must-not-annotate.md", "overflow-must-not-display.md"] {
        items.push(action_feedback_item(
            "check",
            &serde_json::json!(target),
            1,
            "warn",
            &serde_json::Value::Null,
        ));
    }
    serde_json::json!({
        "payload": {
            "result": { "status": "pass", "error_count": 1, "exit_code": 0 },
            "feedback": { "status": "available", "items": items, "existing_count": 4 },
            "errors": [{
                "phase": "parse",
                "code": "INVALID_JSON",
                "description": "available errors stay out of annotations",
                "path": "docs/error.md",
                "path_bytes_hex": null,
                "resource": null,
                "configured_limit": null,
                "observed_lower_bound": null
            }]
        }
    })
}

#[test]
fn action_feedback_filters_execute_the_combined_window_safely() {
    let runtime = fs::read_to_string(repository_root().join("crates/amiss/action/runtime.yml"))
        .expect("packaged Action runtime is readable");
    let (summary_filter, annotation_filter) = action_filters(&runtime);
    let payload = available_action_payload();

    let summary = run_action_jq(summary_filter, &payload);
    assert!(summary.starts_with("amiss pass: 9 Fix, 2 Check, 4 Existing, exit class 0\n"));
    assert_eq!(
        summary
            .lines()
            .filter(|line| line.starts_with("- **"))
            .count(),
        10
    );
    assert!(summary.contains("- 1 more item in report."));
    assert!(
        summary
            .contains("<code>&quot;docs/&lt;/code&gt;`x&amp;%\\n::error::forged.md&quot;</code>")
    );
    assert!(summary.contains("<code>bytes ff</code>"));
    for forbidden in [
        "explicit-target-missing",
        "dependency-changed-subject-unchanged",
        "overflow-must-not-display.md",
        "INVALID_JSON",
        "docs/</code>",
        "\n::error::forged",
    ] {
        assert!(
            !summary.contains(forbidden),
            "unsafe summary output: {summary}"
        );
    }

    let annotations = run_action_jq(annotation_filter, &payload);
    assert_eq!(annotations.lines().count(), 8, "{annotations}");
    assert!(annotations.contains(
        "::error file=docs/a%25%3A%2C%0D%0A.md,line=1,endLine=1,col=1,endColumn=3,title=amiss Fix::Fix target docs/</code>`x&%25%0A::error::forged.md; 2 affected places"
    ));
    assert!(annotations.contains("target bytes ff"));
    for forbidden in [
        "explicit-target-missing",
        "dependency-changed-subject-unchanged",
        "check-must-not-annotate.md",
        "null-annotation-target.md",
        "overflow-must-not-display.md",
        "INVALID_JSON",
        "\r",
    ] {
        assert!(
            !annotations.contains(forbidden),
            "unsafe annotation output: {annotations}"
        );
    }
}

#[test]
fn action_unavailable_feedback_groups_errors_and_caps_annotations() {
    let runtime = fs::read_to_string(repository_root().join("crates/amiss/action/runtime.yml"))
        .expect("packaged Action runtime is readable");
    let (summary_filter, annotation_filter) = action_filters(&runtime);
    let errors: Vec<_> = (0..12)
        .map(|index| {
            serde_json::json!({
                "phase": "parse",
                "code": "INVALID_JSON",
                "description": "the input is invalid JSON",
                "path": match index {
                    0 => serde_json::json!("docs/e%:,\r\n.md"),
                    1 => serde_json::Value::Null,
                    _ => serde_json::json!(format!("docs/error-{index}.md")),
                },
                "path_bytes_hex": if index == 1 {
                    serde_json::json!("ff")
                } else {
                    serde_json::Value::Null
                },
                "resource": null,
                "configured_limit": null,
                "observed_lower_bound": null
            })
        })
        .collect();
    let payload = serde_json::json!({
        "payload": {
            "result": { "status": "incomplete", "error_count": 12, "exit_code": 2 },
            "feedback": { "status": "unavailable" },
            "errors": errors
        }
    });

    let summary = run_action_jq(summary_filter, &payload);
    assert_eq!(
        summary,
        "amiss incomplete: scan failed, 12 errors, exit class 2\n\n- `INVALID_JSON` x12: the input is invalid JSON\n"
    );

    let annotations = run_action_jq(annotation_filter, &payload);
    assert_eq!(annotations.lines().count(), 10, "{annotations}");
    assert!(annotations.contains("at docs/e%25:,%0D%0A.md"));
    assert!(annotations.contains("at bytes ff"));
    assert!(!annotations.contains("docs/error-10.md"));
    assert!(!annotations.contains("docs/error-11.md"));
    assert!(!annotations.contains('\r'));
}

#[test]
fn action_dispatcher_tracks_the_packaged_runtime() {
    let root = repository_root();
    let dispatcher = fs::read_to_string(root.join("action.yml")).expect("dispatcher is readable");
    let runtime = fs::read_to_string(root.join("crates/amiss/action/runtime.yml"))
        .expect("packaged Action runtime is readable");
    let versioned_ref = format!(
        "      uses: HardMax71/amiss@action/v{}",
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        dispatcher.matches(&versioned_ref).count(),
        1,
        "the source dispatcher must make one immutable same-version hop"
    );
    assert!(
        !runtime.contains("uses: HardMax71/amiss@"),
        "the generated runtime must never delegate back to the dispatcher"
    );
    for event_contract in [
        "PR_HEAD: ${{ github.event.pull_request.head.sha }}",
        "pull_request_target) candidate=\"$PR_HEAD\" ;;",
        "git -C \"$INPUT_REPO\" cat-file -e \"${oid}^{commit}\"",
    ] {
        assert!(
            runtime.contains(event_contract),
            "the runtime must select and require the pull_request_target head commit"
        );
    }
    assert!(
        !runtime.contains("git fetch"),
        "the runtime must not acquire untrusted pull request objects"
    );
    assert!(
        runtime.contains("if [[ ! \"$WATCHDOG_SECONDS\" =~ ^[0-9]*[1-9][0-9]*$ ]]; then"),
        "the watchdog input must contain a nonzero digit"
    );
    assert!(
        runtime.contains("if [ -s \"$report\" ]; then\n            printf 'report=%s\\n' \"$report\"\n          else\n            printf 'report=\\n'"),
        "the runtime must not export a missing or empty report"
    );
    assert_action_feedback_contract(&dispatcher, &runtime);

    for input in [
        "profile",
        "base",
        "candidate",
        "repo",
        "object-format",
        "annotations",
        "watchdog-seconds",
    ] {
        let declaration = format!("  {input}:");
        assert_eq!(
            dispatcher
                .lines()
                .filter(|line| *line == declaration)
                .count(),
            1
        );
        assert_eq!(
            runtime.lines().filter(|line| *line == declaration).count(),
            1
        );
        let forwarding = format!("        {input}: ${{{{ inputs.{input} }}}}");
        assert_eq!(
            dispatcher.matches(&forwarding).count(),
            1,
            "the dispatcher must forward {input} exactly once"
        );
    }
    for output in ["exit-class", "report"] {
        let forwarding = format!("value: ${{{{ steps.amiss.outputs.{output} }}}}");
        assert_eq!(dispatcher.matches(&forwarding).count(), 1);
    }

    for workflow in [
        root.join(".github/workflows/ci.yml"),
        root.join(".github/workflows/release.yml"),
    ] {
        let source = fs::read_to_string(&workflow).expect("Action assembly workflow is readable");
        assert!(
            source
                .contains("install -m 0644 crates/amiss/action/runtime.yml action-tree/action.yml")
        );
        assert!(source.contains("cp LICENSE.md action-tree/LICENSE.md"));
        assert!(
            source
                .contains("bash scripts/release-licenses.sh action-tree/THIRD_PARTY_LICENSES.txt")
        );
        assert!(!source.contains("install -m 0644 action.yml action-tree/action.yml"));
    }
}
