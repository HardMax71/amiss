use amiss_wire::report::AnalysisErrorCode;

use crate::support::{amiss, fixture, payload, report};

/// `${{ github.repository }}` is `Owner/Name`, capitals and all, and the engine
/// requires the canonical lowercase identity. It will not fold the value itself:
/// the CLI's repository is a claim it cannot authenticate, the report has no
/// field to record what was actually typed, and the wrapper that folds an
/// authenticated identity is the layer allowed to do that. What the engine owes
/// instead is a refusal that can be acted on, because a bare error code is not
/// documentation.
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

#[test]
fn a_help_seeker_is_taught_the_closed_grammar() {
    let (code, stdout, stderr) = amiss(&["--help"]);
    assert_eq!(code, 0, "help is a successful query: {stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    let (_, _, rejected) = amiss(&["--help", "--help"]);
    assert!(
        rejected.as_bytes().ends_with(&stdout),
        "help and rejection project the same complete grammar: {rejected}"
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

/// The standalone queries are not flags and carry nothing of their own.
#[test]
fn standalone_queries_accept_no_other_token() {
    let (_, grammar, _) = amiss(&["--help"]);
    let grammar = String::from_utf8(grammar).unwrap();
    for (argv, reason) in [
        (["--help", "--help"].as_slice(), "--help stands alone"),
        (&["--version", "--version"], "--version stands alone"),
        (&["check", "--version"], "unknown option \"--version\""),
    ] {
        let (code, stdout, stderr) = amiss(argv);
        assert_eq!(code, 2, "{argv:?} is not a standalone query");
        assert!(stdout.is_empty(), "{argv:?} produced stdout");
        assert!(
            stderr.contains(reason),
            "{argv:?} names the defect: {stderr}"
        );
        assert!(
            stderr.contains(grammar.trim_end()),
            "{argv:?} refusal carries the whole grammar: {stderr}"
        );
    }
    for flag in ["--help", "--version"] {
        let (code, stdout, _) = amiss(&[flag, "--format", "json"]);
        assert_eq!(code, 2, "a second token makes it an invalid invocation");
        let envelope: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert!(
            envelope.get("payload").is_some(),
            "a selected format still refuses through the envelope"
        );
    }
}

/// `<verb> --help` and `<verb> -h` cut that verb's lines out of the one
/// grammar; `-h` and `-V` are the short spellings of the standalone queries.
#[test]
fn a_verb_help_prints_only_that_verb() {
    let (_, whole, _) = amiss(&["--help"]);
    let whole = String::from_utf8(whole).unwrap();
    let (short_code, short, short_stderr) = amiss(&["-h"]);
    assert_eq!((short_code, short_stderr.as_str()), (0, ""));
    assert_eq!(String::from_utf8(short).unwrap(), whole, "-h is --help");
    for (verb, flag, owned, foreign) in [
        ("check", "--help", "--explain-scope", "--full"),
        ("check", "-h", "--semantic-template", "--report"),
        ("render", "--help", "--full", "--profile"),
        ("refs", "-h", "--target-bytes-hex", "--evidence"),
    ] {
        let (code, stdout, stderr) = amiss(&[verb, flag]);
        assert_eq!((code, stderr.as_str()), (0, ""), "{verb} {flag}");
        let text = String::from_utf8(stdout).unwrap();
        assert!(
            text.starts_with(&format!("amiss {verb} ")),
            "{verb}: {text}"
        );
        assert!(text.contains(owned), "{verb} keeps its own option: {text}");
        assert!(
            !text.contains(foreign),
            "{verb} drops the other forms: {text}"
        );
        assert!(
            whole.contains(text.trim_end()),
            "the verb block is cut from the whole grammar: {text}"
        );
        assert_eq!(
            text.lines()
                .filter(|line| line.starts_with("amiss "))
                .count(),
            1,
            "one form only: {text}"
        );
    }
    let (long_code, long, _) = amiss(&["--version"]);
    let (short_code, short, short_stderr) = amiss(&["-V"]);
    assert_eq!((short_code, short_stderr.as_str()), (long_code, ""));
    assert_eq!(short, long, "-V is --version");
}

/// Every refusal class names the option it refused and, where it had one,
/// the value it got, before the grammar; the machine lane carries only the
/// code, as it always did.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one table over every refusal class the grammar has"
)]
fn every_refusal_class_names_its_option() {
    let fx = fixture();
    let (_, grammar, _) = amiss(&["--help"]);
    let grammar = String::from_utf8(grammar).unwrap();
    let base_args = [
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
    let with = |extra: &[&str]| -> Vec<String> {
        base_args
            .iter()
            .chain(extra)
            .map(|token| (*token).to_owned())
            .collect()
    };
    let edited = |from: &str, to: &str| -> Vec<String> {
        base_args
            .iter()
            .map(|token| {
                if *token == from {
                    to.to_owned()
                } else {
                    (*token).to_owned()
                }
            })
            .collect()
    };
    let without = |option: &str, values: usize| -> Vec<String> {
        let mut args = with(&[]);
        let at = args.iter().position(|token| token == option).unwrap();
        args.drain(at..=at.saturating_add(values));
        args
    };
    let uppercase = fx.base.to_uppercase();
    let short = fx.base.get(..7).unwrap().to_owned();
    let mut equals_form = without("--repo", 1);
    equals_form.push("--repo=.".to_owned());
    let mut starved = without("--base", 1);
    starved.insert(1, "--base".to_owned());
    let identity = [
        "--repository",
        "example.internal/acme/widget",
        "--ref",
        "refs/heads/main",
        "--default-branch-ref",
        "refs/heads/main",
    ];
    let bad_ref = [
        "--repository",
        "github.com/acme/widget",
        "--ref",
        "main",
        "--default-branch-ref",
        "refs/heads/main",
    ];
    let cases: Vec<(Vec<String>, AnalysisErrorCode, String)> = vec![
        (
            with(&["--verbose"]),
            AnalysisErrorCode::InvalidInvocation,
            "unknown option \"--verbose\"".to_owned(),
        ),
        (
            edited(&fx.base, "main"),
            AnalysisErrorCode::InvalidInvocation,
            "--base must be the full 40-character lowercase hex id of a sha1 commit, got \"main\""
                .to_owned(),
        ),
        (
            edited(&fx.base, &short),
            AnalysisErrorCode::InvalidInvocation,
            format!(
                "--base must be the full 40-character lowercase hex id of a sha1 commit, got \"{short}\""
            ),
        ),
        (
            edited(&fx.base, &uppercase),
            AnalysisErrorCode::InvalidInvocation,
            format!(
                "--base must be the full 40-character lowercase hex id of a sha1 commit, got \"{uppercase}\""
            ),
        ),
        (
            edited(&fx.candidate, &fx.base),
            AnalysisErrorCode::InvalidInvocation,
            "--candidate and --base name the same commit".to_owned(),
        ),
        (
            with(&["--repository", "github.com/acme/widget"]),
            AnalysisErrorCode::InvalidInvocation,
            "--repository needs --ref and --default-branch-ref".to_owned(),
        ),
        (
            equals_form,
            AnalysisErrorCode::InvalidInvocation,
            "options take a separate value, not \"--repo=.\"".to_owned(),
        ),
        (
            without("--profile", 1),
            AnalysisErrorCode::InvalidInvocation,
            "--profile is required".to_owned(),
        ),
        (
            edited("observe", "enfor"),
            AnalysisErrorCode::InvalidProfile,
            "--profile must be observe, enforce-introduced, or enforce, got \"enfor\"".to_owned(),
        ),
        (
            with(&["--index"]),
            AnalysisErrorCode::InvalidInvocation,
            "--candidate and --index are exclusive".to_owned(),
        ),
        (
            without("--candidate", 1),
            AnalysisErrorCode::InvalidInvocation,
            "one of --candidate or --index is required".to_owned(),
        ),
        (
            with(&["--report", "x.json"]),
            AnalysisErrorCode::InvalidInvocation,
            "--report is not an option of check".to_owned(),
        ),
        (
            with(&["--format", "junit"]),
            AnalysisErrorCode::InvalidInvocation,
            "--format junit is only for render".to_owned(),
        ),
        (
            with(&["--profile", "observe"]),
            AnalysisErrorCode::InvalidInvocation,
            "--profile appears more than once".to_owned(),
        ),
        (
            starved,
            AnalysisErrorCode::InvalidInvocation,
            "--base needs a value".to_owned(),
        ),
        (
            with(&["--forge", "github"]),
            AnalysisErrorCode::InvalidInvocation,
            "--forge needs --repository, --ref, and --default-branch-ref".to_owned(),
        ),
        (
            with(&identity),
            AnalysisErrorCode::InvalidEvent,
            "--forge must name the dialect of \"example.internal\", a host outside the known table"
                .to_owned(),
        ),
        (
            with(&bad_ref),
            AnalysisErrorCode::InvalidEvent,
            "--ref must be refs/heads/<name>, got \"main\"".to_owned(),
        ),
        (
            edited("sha1", "sha9"),
            AnalysisErrorCode::InvalidInvocation,
            "--object-format must be sha1 or sha256, got \"sha9\"".to_owned(),
        ),
        (
            Vec::new(),
            AnalysisErrorCode::InvalidInvocation,
            "a verb must come first".to_owned(),
        ),
        (
            vec!["scan".to_owned()],
            AnalysisErrorCode::InvalidInvocation,
            "unknown verb \"scan\"".to_owned(),
        ),
        (
            ["render", "--report", "x.json"]
                .iter()
                .map(|token| (*token).to_owned())
                .collect(),
            AnalysisErrorCode::InvalidInvocation,
            "--format is required by render".to_owned(),
        ),
        (
            ["refs", "--report", "x.json"]
                .iter()
                .map(|token| (*token).to_owned())
                .collect(),
            AnalysisErrorCode::InvalidInvocation,
            "one of --target or --target-bytes-hex is required".to_owned(),
        ),
        (
            [
                "claim",
                "--repo",
                ".",
                "--path",
                "README.md",
                "--line",
                "0",
                "--name",
                "x",
            ]
            .iter()
            .map(|token| (*token).to_owned())
            .collect(),
            AnalysisErrorCode::InvalidInvocation,
            "--line must be a one-based line number without leading zeros, got \"0\"".to_owned(),
        ),
    ];
    for (argv, code, reason) in cases {
        let shown: Vec<&str> = argv.iter().map(String::as_str).collect();
        let (exit, stdout, stderr) = amiss(&shown);
        assert_eq!(exit, 2, "{shown:?}: {stderr}");
        assert!(stdout.is_empty(), "{shown:?} produced stdout");
        assert!(
            stderr.starts_with(&format!("amiss: {}\n", code.as_ref())),
            "{shown:?} opens with its code: {stderr}"
        );
        assert!(
            stderr.contains(&format!("\n  {reason}\n")),
            "{shown:?} names the contract it refused: {stderr}"
        );
        assert!(
            stderr.ends_with(grammar.as_str()),
            "{shown:?} closes with the whole grammar: {stderr}"
        );
    }

    let (exit, stdout, stderr) = amiss(
        &with(&["--format", "yaml"])
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    assert_eq!((exit, stdout.as_slice()), (2, b"".as_slice()));
    assert_eq!(
        stderr,
        "amiss: invalid invocation: --format must be one of human, json, sarif, codequality, junit, got \"yaml\"\n"
    );

    let machine = with(&["--verbose", "--format", "json"]);
    let shown: Vec<&str> = machine.iter().map(String::as_str).collect();
    let (exit, stdout, stderr) = amiss(&shown);
    assert_eq!((exit, stderr.as_str()), (2, ""));
    let refused = report(&stdout);
    assert_eq!(
        refused.payload.errors[0].code,
        AnalysisErrorCode::InvalidInvocation
    );
    assert_eq!(
        refused.payload.errors[0].description,
        AnalysisErrorCode::InvalidInvocation.meaning(),
        "the wire keeps the fixed sentence and never the reason line"
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
