use crate::support::{amiss, fixture, payload};

/// `${{ github.repository }}` is `Owner/Name`, capitals and all, and the engine
/// requires the canonical lowercase identity. It will not fold the value itself:
/// the CLI's repository is a claim it cannot authenticate, the report has no
/// field to record what was actually typed, and the wrapper that folds an
/// authenticated identity is the layer allowed to do that. What the engine owes
/// instead is a refusal that can be acted on, because there is no `--help` and a
/// bare error code is not documentation.
#[test]
fn a_noncanonical_repository_owner_is_refused_in_terms_the_caller_can_act_on() {
    let fx = fixture();
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
        "--repository",
        "github.com/HardMax71/amiss",
        "--ref",
        "refs/heads/main",
        "--default-branch-ref",
        "refs/heads/main",
        "--profile",
        "observe",
    ]);
    assert_eq!(code, 2, "an event it cannot trust is never a result");
    assert!(stdout.is_empty(), "a refusal is not a report");
    assert!(stderr.contains("INVALID_EVENT"), "{stderr}");
    assert!(
        stderr.contains("lowercase"),
        "the refusal names the contract it enforced: {stderr}"
    );
}

/// The first thing a person or an agent tries is `--help`, and the docs live
/// in a repository they are not standing in. The refusal is the only channel
/// the binary owns at that moment, so it teaches the entire closed grammar.
#[test]
fn a_help_seeker_is_taught_the_closed_grammar() {
    let (code, stdout, stderr) = amiss(&["--help"]);
    assert_eq!(code, 2, "there is no help lane, only the refusal");
    assert!(stdout.is_empty(), "a refusal is not a report");
    assert!(stderr.contains("INVALID_INVOCATION"), "{stderr}");
    assert!(
        stderr.contains(amiss::invocation::GRAMMAR),
        "the refusal carries the whole grammar: {stderr}"
    );
}

/// A version alone would not answer the question the manifest asks. The digest
/// line is checked against a real report rather than against itself, because
/// its whole value is being the same `engine_digest` the report stamps.
#[test]
fn the_version_query_names_the_engine_that_writes_the_reports() {
    let (code, stdout, stderr) = amiss(&["--version"]);
    assert_eq!(code, 0, "an identity query is not a refusal: {stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    let printed = String::from_utf8_lossy(&stdout).into_owned();
    let mut lines = printed.lines();
    let named = lines.next().unwrap_or_default().to_owned();
    let engine = lines.next().unwrap_or_default().to_owned();
    assert_eq!(lines.next(), None, "the query prints two lines and stops");

    let fx = fixture();
    let (_, report, _) = amiss(&[
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
    let stamped = payload(&report).get("engine").cloned().unwrap();
    let version = stamped
        .get("engine_version")
        .and_then(|v| v.as_str())
        .unwrap();
    let digest = stamped
        .get("engine_digest")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(named, format!("amiss {version}"));
    assert_eq!(engine, format!("engine {digest}"));
}

/// The grammar stays closed around the new form: it is not a flag `check`
/// accepts, and it carries nothing of its own.
#[test]
fn the_version_query_stands_alone() {
    for argv in [
        ["--version", "--version"].as_slice(),
        ["check", "--version"].as_slice(),
    ] {
        let (code, stdout, stderr) = amiss(argv);
        assert_eq!(code, 2, "{argv:?} is not a version query");
        assert!(stdout.is_empty(), "{argv:?} produced stdout");
        assert!(
            stderr.contains(amiss::invocation::GRAMMAR),
            "{argv:?} refusal carries the whole grammar: {stderr}"
        );
    }
    let (code, stdout, _) = amiss(&["--version", "--format", "json"]);
    assert_eq!(code, 2, "a second token makes it an invalid invocation");
    let envelope: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert!(
        envelope.get("payload").is_some(),
        "a selected format still refuses through the envelope, never a version"
    );
}

#[test]
fn explain_scope_adds_the_deterministic_block() {
    let fx = fixture();
    let run = |extra: &[&str]| {
        let mut args = vec![
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
        ];
        args.extend_from_slice(extra);
        amiss(&args)
    };
    let (_c, plain, _e) = run(&[]);
    let (_c, explained, _e) = run(&["--explain-scope"]);
    let plain = String::from_utf8_lossy(&plain);
    let explained = String::from_utf8_lossy(&explained);
    assert!(!plain.contains("scope:"));
    assert!(explained.contains("scope: built-in documents"));
    assert!(explained.contains("scope: this run discovered"));
}
