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
    let (summary_filter, annotation_filter) = action_filters(runtime);
    assert!(
        !summary_filter.contains("findings"),
        "the summary must consume the feedback projection instead of raw findings"
    );
    assert!(
        annotation_filter
            .contains("select(.location.path == $at.path and .location.span == $at.span)"),
        "an annotation reads a finding row only at its own location, for the resolution detail"
    );
    for presentation_contract in [
        "$p.feedback.existing_count",
        "amiss \\($p.result.status): scan failed",
        "(($p.feedback.items | length) - 10",
        "tojson | .[1:-1] | html",
        "<code>bytes ",
        ":\\(.annotation.span.start_line)</code>",
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

fn action_finding_row(
    annotation: &serde_json::Value,
    resolution: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "explicit-target-missing",
        "location": {
            "path": annotation.get("path"),
            "side": "candidate",
            "span": annotation.get("span")
        },
        "candidate_fact": { "evidence": { "kind": "reference", "resolution": resolution } }
    })
}

fn available_action_payload() -> serde_json::Value {
    let mut items = Vec::new();
    let mut findings = Vec::new();
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
        match index {
            0 => findings.push(action_finding_row(
                &annotation,
                &serde_json::json!({
                    "kind": "missing",
                    "reason": "heading-anchor-not-found",
                    "path": "docs/guide.md",
                    "near": "near\nheading"
                }),
            )),
            1 => findings.push(action_finding_row(
                &annotation,
                &serde_json::json!({
                    "kind": "missing",
                    "reason": "path-not-found",
                    "path": { "bytes_hex": "ff" },
                    "near": { "bytes_hex": "fe" },
                    "same_object_at": "docs/moved.md"
                }),
            )),
            2 => findings.push(action_finding_row(
                &action_annotation("docs/fix-2.md#elsewhere"),
                &serde_json::json!({ "kind": "missing", "reason": "label-not-declared" }),
            )),
            _ => {}
        }
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
            "findings": findings,
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
        summary.contains(
            "- **Fix** <code>docs/&lt;/code&gt;`x&amp;%\\n::error::forged.md</code> explicit-target-missing at <code>docs/a%:,\\r\\n.md:1</code>, 2 affected places"
        ),
        "{summary}"
    );
    assert!(summary.contains("<code>bytes ff</code> explicit-target-missing at <code>docs/fix-1.md:1</code>, 3 affected places"));
    assert!(summary.contains("- **Check** <code>check-must-not-annotate.md</code> dependency-changed-subject-unchanged, 1 affected place"));
    for forbidden in [
        "overflow-must-not-display.md",
        "INVALID_JSON",
        "docs/</code>",
        "\n::error::forged",
        "&quot;",
        "\u{2014}",
    ] {
        assert!(
            !summary.contains(forbidden),
            "unsafe summary output: {summary}"
        );
    }

    let annotations = run_action_jq(annotation_filter, &payload);
    assert_eq!(annotations.lines().count(), 8, "{annotations}");
    assert!(
        annotations.contains(
            "::error file=docs/a%25%3A%2C%0D%0A.md,line=1,endLine=1,col=1,endColumn=3,title=amiss Fix::Fix explicit-target-missing: target docs/</code>`x&%25%0A::error::forged.md; heading-anchor-not-found, near near%0Aheading; 2 affected places"
        ),
        "{annotations}"
    );
    assert!(
        annotations.contains(
            "::warning file=docs/fix-1.md,line=1,endLine=1,col=1,endColumn=3,title=amiss Fix::Fix explicit-target-missing: target bytes ff; path-not-found, near bytes fe, same bytes at docs/moved.md; 3 affected places"
        ),
        "{annotations}"
    );
    assert!(
        annotations.contains(
            "::warning file=docs/fix-2.md,line=1,endLine=1,col=1,endColumn=3,title=amiss Fix::Fix explicit-target-missing: target docs/target-2.md; 4 affected places"
        ),
        "a finding row at another location lends no detail: {annotations}"
    );
    for forbidden in [
        "dependency-changed-subject-unchanged",
        "label-not-declared",
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
fn action_summary_labels_the_existing_backlog() {
    let runtime = fs::read_to_string(repository_root().join("crates/amiss/action/runtime.yml"))
        .expect("packaged Action runtime is readable");
    let (summary_filter, _annotation_filter) = action_filters(&runtime);
    let payload = serde_json::json!({
        "payload": {
            "result": { "status": "fail", "error_count": 0, "exit_code": 1 },
            "feedback": {
                "status": "available",
                "items": [
                    action_feedback_item(
                        "fix",
                        &serde_json::json!("docs/new.md"),
                        1,
                        "fail",
                        &serde_json::Value::Null,
                    ),
                    action_feedback_item(
                        "existing",
                        &serde_json::json!("docs/old.md"),
                        2,
                        "fail",
                        &serde_json::Value::Null,
                    ),
                ],
                "existing_count": 1
            },
            "errors": []
        }
    });
    let summary = run_action_jq(summary_filter, &payload);
    assert!(
        summary.starts_with("amiss fail: 1 Fix, 0 Check, 1 Existing, exit class 1\n"),
        "{summary}"
    );
    assert!(
        summary.contains(
            "- **Existing** <code>docs/old.md</code> dependency-changed-subject-unchanged, 2 affected places"
        ),
        "the backlog item is labeled as existing, not check: {summary}"
    );
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

fn assert_derive_contract(runtime: &str) {
    for event_contract in [
        "PR_HEAD: ${{ github.event.pull_request.head.sha }}",
        "pull_request_target) candidate=\"$PR_HEAD\" ;;",
        "repo cat-file -e \"$1^{commit}\"",
        "for oid in \"$base\" \"$candidate\"; do\n          if ! present \"$oid\"; then",
    ] {
        assert!(
            runtime.contains(event_contract),
            "the runtime must select and require the pull_request_target head commit"
        );
    }
    let fetches = runtime
        .lines()
        .filter(|line| {
            !line.trim_start().starts_with('#')
                && line.contains("fetch")
                && !line.contains("fetch-depth")
        })
        .count();
    assert_eq!(
        fetches, 1,
        "the one fetch deepens the checkout; nothing else acquires objects"
    );
    for deepening_contract in [
        "[ -z \"$deepened\" ] && [ \"$EVENT_NAME\" != pull_request_target ] || return 1",
        "[ \"$(repo rev-parse --is-shallow-repository)\" = true ] || return 1",
        "repo fetch --quiet --deepen=1 origin \"$(repo rev-parse HEAD)\"",
    ] {
        assert!(
            runtime.contains(deepening_contract),
            "the runtime deepens a shallow checkout once, from its own head, and never on pull_request_target"
        );
    }
    for hint in [
        "give actions/checkout fetch-depth: 2\"\n          exit 2",
        "pull_request) hint=\"give actions/checkout fetch-depth: 2\" ;;",
        "pull_request_target) hint=\"a pull_request_target checkout is the base branch, so give actions/checkout ref: the pull request head sha (github.event.pull_request.head.sha) with fetch-depth: 0\" ;;",
        "*) hint=\"give actions/checkout fetch-depth: 0\" ;;",
    ] {
        assert!(
            runtime.contains(hint),
            "a missing commit names the checkout setting that supplies it: {hint}"
        );
    }
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
    assert_derive_contract(&runtime);
    assert!(
        runtime.contains("if [[ ! \"$WATCHDOG_SECONDS\" =~ ^[0-9]*[1-9][0-9]*$ ]]; then"),
        "the watchdog input must contain a nonzero digit"
    );
    assert!(
        runtime.contains("if [ -s \"$report\" ]; then\n            printf 'report=%s\\n' \"$report\"\n          else\n            printf 'report=\\n'"),
        "the scan step hands the annotate step an empty path when no report exists"
    );
    assert!(
        runtime.contains("    - id: verdict\n      if: always()\n")
            && runtime.contains("status=\"${STATUS:-2}\"")
            && runtime.matches("report=\"${RUNNER_TEMP}/amiss-report.json\"").count() == 2
            && runtime.contains("[ -s \"$report\" ] || rm -f \"$report\"\n        {\n          printf 'exit-class=%s\\n' \"$status\"\n          printf 'report=%s\\n' \"$report\"\n        } >> \"$GITHUB_OUTPUT\""),
        "the verdict step always runs, exports exit class 2 when the scan never wrote one, and always names the one report path with an empty file removed"
    );
    assert!(
        runtime.contains("if [ \"${ANNOTATIONS,,}\" != \"true\" ]"),
        "the annotations input is read case-insensitively"
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
        let exported = format!("value: ${{{{ steps.verdict.outputs.{output} }}}}");
        assert_eq!(
            runtime.matches(&exported).count(),
            1,
            "the runtime exports {output} from the step that always runs"
        );
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
