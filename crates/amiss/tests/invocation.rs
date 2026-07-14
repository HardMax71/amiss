use std::ffi::OsString;

use amiss::invocation::{CandidateSelector, Code, Outcome, OutputFormat, parse};

const BASE_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn argv(tokens: &[&str]) -> Vec<OsString> {
    tokens.iter().map(OsString::from).collect()
}

fn valid_pair() -> Vec<String> {
    [
        "check",
        "--repo",
        ".",
        "--object-format",
        "sha1",
        "--base",
        BASE_A,
        "--candidate",
        HEAD_B,
        "--profile",
        "observe",
    ]
    .iter()
    .map(|token| (*token).to_owned())
    .collect()
}

fn parse_tokens(tokens: &[String]) -> Outcome {
    let argv: Vec<OsString> = tokens.iter().map(OsString::from).collect();
    parse(&argv)
}

#[expect(clippy::panic, reason = "test helper asserts the rejected shape")]
fn rejected_codes(outcome: Outcome) -> Vec<Code> {
    match outcome {
        Outcome::Rejected { codes, .. } => codes.into_iter().collect(),
        Outcome::Accepted(_) | Outcome::MalformedOutputSelection => {
            panic!("expected rejection, got {outcome:?}")
        }
    }
}

#[test]
fn accepts_the_commit_pair_grammar() {
    let Outcome::Accepted(invocation) = parse_tokens(&valid_pair()) else {
        panic!("expected acceptance");
    };
    assert_eq!(invocation.base.as_str(), BASE_A);
    match &invocation.candidate {
        CandidateSelector::Commit(oid) => assert_eq!(oid.as_str(), HEAD_B),
        CandidateSelector::Index => panic!("expected a commit candidate"),
    }
    assert_eq!(invocation.format, OutputFormat::Human);
    assert!(!invocation.explain_scope);
    assert!(invocation.identity.is_none());
}

#[test]
fn accepts_index_mode_with_identity_and_flags() {
    let mut tokens = valid_pair();
    let candidate_at = tokens
        .iter()
        .position(|token| token == "--candidate")
        .unwrap();
    tokens.drain(candidate_at..=candidate_at + 1);
    tokens.push("--index".to_owned());
    tokens.extend(
        [
            "--repository",
            "github.com/acme/spec-to-rest",
            "--ref",
            "refs/heads/main",
            "--default-branch-ref",
            "refs/heads/main",
            "--explain-scope",
            "--format",
            "json",
        ]
        .iter()
        .map(|token| (*token).to_owned()),
    );
    let Outcome::Accepted(invocation) = parse_tokens(&tokens) else {
        panic!("expected acceptance");
    };
    assert_eq!(invocation.candidate, CandidateSelector::Index);
    assert_eq!(invocation.format, OutputFormat::Json);
    assert!(invocation.explain_scope);
    let identity = invocation.identity.unwrap();
    assert_eq!(identity.repository.owner, "acme");
    assert_eq!(identity.ref_name.as_str(), "refs/heads/main");
}

#[test]
fn rejects_structural_defects_as_invalid_invocation() {
    let cases: Vec<Vec<String>> = vec![
        vec![],
        argv_strings(&["scan"]),
        argv_strings(&["check", "extra"]),
        with(&valid_pair(), &["--unknown"]),
        with(&valid_pair(), &["--"]),
        with(&valid_pair(), &["--base=abc"]),
        with(&valid_pair(), &["--worktree"]),
        with(&valid_pair(), &["--profile", "observe"]),
        without_option(&valid_pair(), "--profile"),
        without_option(&valid_pair(), "--candidate"),
        with(&valid_pair(), &["--index"]),
        replace_value(&valid_pair(), BASE_A, HEAD_B),
        replace_value(&valid_pair(), BASE_A, &BASE_A.to_uppercase()),
        replace_value(
            &valid_pair(),
            BASE_A,
            &BASE_A.chars().take(39).collect::<String>(),
        ),
        replace_value(&valid_pair(), ".", ""),
        with(&valid_pair(), &["--repository", "github.com/acme/repo"]),
    ];
    for tokens in cases {
        assert_eq!(
            rejected_codes(parse_tokens(&tokens)),
            vec![Code::InvalidInvocation],
            "tokens {tokens:?}"
        );
    }
}

#[test]
fn classifies_profile_host_and_event_rows() {
    let bogus_profile = replace_value(&valid_pair(), "observe", "audit");
    assert_eq!(
        rejected_codes(parse_tokens(&bogus_profile)),
        vec![Code::InvalidProfile]
    );

    let empty_profile = replace_value(&valid_pair(), "observe", "");
    assert_eq!(
        rejected_codes(parse_tokens(&empty_profile)),
        vec![Code::InvalidProfile]
    );

    let gitlab = with(
        &valid_pair(),
        &[
            "--repository",
            "gitlab.com/acme/repo",
            "--ref",
            "refs/heads/main",
            "--default-branch-ref",
            "refs/heads/main",
        ],
    );
    assert_eq!(
        rejected_codes(parse_tokens(&gitlab)),
        vec![Code::UnsupportedProviderHost]
    );

    let uppercase_owner = with(
        &valid_pair(),
        &[
            "--repository",
            "github.com/Acme/repo",
            "--ref",
            "refs/heads/main",
            "--default-branch-ref",
            "refs/heads/main",
        ],
    );
    assert_eq!(
        rejected_codes(parse_tokens(&uppercase_owner)),
        vec![Code::InvalidEvent]
    );

    let bad_ref = with(
        &valid_pair(),
        &[
            "--repository",
            "github.com/acme/repo",
            "--ref",
            "refs/heads/a..b",
            "--default-branch-ref",
            "refs/heads/main",
        ],
    );
    assert_eq!(
        rejected_codes(parse_tokens(&bad_ref)),
        vec![Code::InvalidEvent]
    );

    let two_component = with(
        &valid_pair(),
        &[
            "--repository",
            "gitlab.com/acme",
            "--ref",
            "refs/heads/main",
            "--default-branch-ref",
            "refs/heads/main",
        ],
    );
    assert_eq!(
        rejected_codes(parse_tokens(&two_component)),
        vec![Code::InvalidInvocation],
        "an incomplete value is not guessed into a lower row"
    );
}

#[test]
fn emits_every_applicable_row_together() {
    let mut tokens = replace_value(&valid_pair(), "observe", "audit");
    tokens.extend(
        [
            "--repository",
            "gitlab.com/acme/repo",
            "--ref",
            "refs/heads/main",
            "--default-branch-ref",
            "refs/heads/main",
            "--unknown",
        ]
        .iter()
        .map(|token| (*token).to_owned()),
    );
    assert_eq!(
        rejected_codes(parse_tokens(&tokens)),
        vec![
            Code::InvalidInvocation,
            Code::InvalidProfile,
            Code::UnsupportedProviderHost
        ]
    );
}

#[test]
fn option_shaped_tokens_are_not_values() {
    let mut tokens = valid_pair();
    let base_at = tokens.iter().position(|token| token == "--base").unwrap();
    tokens.remove(base_at + 1);
    assert_eq!(
        rejected_codes(parse_tokens(&tokens)),
        vec![Code::InvalidInvocation],
        "--base consumes --candidate as an option, not as a value"
    );
}

#[test]
fn output_selection_follows_the_format_law() {
    let json_with_errors = with(
        &replace_value(&valid_pair(), "observe", "audit"),
        &["--format", "json"],
    );
    let Outcome::Rejected { format, .. } = parse_tokens(&json_with_errors) else {
        panic!("expected rejection");
    };
    assert_eq!(format, OutputFormat::Json);

    for malformed in [
        with(&valid_pair(), &["--format", "yaml"]),
        with(&valid_pair(), &["--format"]),
        with(&valid_pair(), &["--format", "json", "--format", "json"]),
        with(&valid_pair(), &["--format", "--explain-scope"]),
    ] {
        assert_eq!(
            parse_tokens(&malformed),
            Outcome::MalformedOutputSelection,
            "tokens {malformed:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn rejects_non_unicode_argv_before_lossy_conversion() {
    use std::os::unix::ffi::OsStringExt as _;

    let mut tokens = argv(&[
        "check",
        "--repo",
        ".",
        "--object-format",
        "sha1",
        "--base",
        BASE_A,
        "--candidate",
        HEAD_B,
        "--profile",
        "observe",
    ]);
    tokens.push(OsString::from_vec(vec![0xff, 0xfe]));
    assert_eq!(
        rejected_codes(parse(&tokens)),
        vec![Code::InvalidInvocation]
    );
}

#[cfg(windows)]
#[test]
fn rejects_unpaired_surrogate_argv_before_lossy_conversion() {
    use std::os::windows::ffi::OsStringExt as _;

    let mut tokens = argv(&[
        "check",
        "--repo",
        ".",
        "--object-format",
        "sha1",
        "--base",
        BASE_A,
        "--candidate",
        HEAD_B,
        "--profile",
        "observe",
    ]);
    tokens.push(OsString::from_wide(&[0xD800]));
    assert_eq!(
        rejected_codes(parse(&tokens)),
        vec![Code::InvalidInvocation]
    );
}

fn argv_strings(tokens: &[&str]) -> Vec<String> {
    tokens.iter().map(|token| (*token).to_owned()).collect()
}

fn with(base: &[String], extra: &[&str]) -> Vec<String> {
    let mut tokens = base.to_vec();
    tokens.extend(extra.iter().map(|token| (*token).to_owned()));
    tokens
}

fn without_option(base: &[String], option: &str) -> Vec<String> {
    let mut tokens = base.to_vec();
    if let Some(at) = tokens.iter().position(|token| token == option) {
        tokens.drain(at..=at.saturating_add(1));
    }
    tokens
}

fn replace_value(base: &[String], from: &str, to: &str) -> Vec<String> {
    base.iter()
        .map(|token| {
            if token == from {
                to.to_owned()
            } else {
                token.clone()
            }
        })
        .collect()
}
