use std::ffi::OsString;

use amiss::invocation::{CandidateSelector, Code, Outcome, OutputFormat, parse};
use amiss_wire::controls::Profile;
use amiss_wire::model::ForgeDialect;

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
        Outcome::Accepted(_) | Outcome::MalformedOutputSelection | Outcome::Version => {
            panic!("expected rejection, got {outcome:?}")
        }
    }
}

#[test]
fn accepts_the_commit_pair_grammar() {
    let Outcome::Accepted(command) = parse_tokens(&valid_pair()) else {
        panic!("expected acceptance");
    };
    let amiss::invocation::Command::Scan(invocation) = *command else {
        panic!("expected a scan command");
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
    let Outcome::Accepted(command) = parse_tokens(&tokens) else {
        panic!("expected acceptance");
    };
    let amiss::invocation::Command::Scan(invocation) = *command else {
        panic!("expected a scan command");
    };
    assert_eq!(invocation.candidate, CandidateSelector::Index);
    assert_eq!(invocation.format, OutputFormat::Json);
    assert!(invocation.explain_scope);
    let identity = invocation.identity.unwrap();
    assert_eq!(identity.repository.owner(), "acme");
    assert_eq!(identity.ref_name.as_str(), "refs/heads/main");
}

#[test]
fn rejects_structural_defects_as_invalid_invocation() {
    let cases: Vec<Vec<String>> = vec![
        vec![],
        argv_strings(&["scan"]),
        replace_value(&valid_pair(), "check", "Check"),
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
    let Outcome::Accepted(other_forge_command) = parse_tokens(&gitlab) else {
        panic!("an identity on another forge is a claim, not a refusal");
    };
    let amiss::invocation::Command::Scan(other_forge) = *other_forge_command else {
        panic!("expected a scan command");
    };
    assert_eq!(
        other_forge.identity.unwrap().repository.host(),
        "gitlab.com"
    );
    assert_eq!(
        other_forge.forge,
        Some(ForgeDialect::Gitlab),
        "the known-host table names the dialect"
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
        vec![Code::InvalidInvocation, Code::InvalidProfile]
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

    let starved = with(
        &without_value(&valid_pair(), "--profile"),
        &["--explain-scope"],
    );
    assert_eq!(
        rejected_codes(parse_tokens(&starved)),
        vec![Code::InvalidInvocation],
        "--profile that swallowed --explain-scope would name the flag as its profile instead"
    );
}

/// Each duplicate rule stands on its own, and an option that arrived without
/// its value is defective rather than absent.
#[test]
fn every_repetition_refuses_by_itself() {
    let index_only = without_option(&valid_pair(), "--candidate");
    let cases = [
        with(&index_only, &["--index", "--index"]),
        with(&valid_pair(), &["--explain-scope", "--explain-scope"]),
        with(&valid_pair(), &["--candidate", HEAD_B]),
        without_value(&valid_pair(), "--candidate"),
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
fn the_ramp_profile_parses_between_the_two() {
    let ramp = replace_value(&valid_pair(), "observe", "enforce-introduced");
    let Outcome::Accepted(command) = parse_tokens(&ramp) else {
        panic!("expected acceptance");
    };
    let amiss::invocation::Command::Scan(invocation) = *command else {
        panic!("expected a scan command");
    };
    assert_eq!(invocation.profile, Profile::EnforceIntroduced);
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

    for (value, expected) in [
        ("sarif", OutputFormat::Sarif),
        ("codequality", OutputFormat::CodeQuality),
    ] {
        let machine = with(&valid_pair(), &["--format", value]);
        let Outcome::Accepted(command) = parse_tokens(&machine) else {
            panic!("expected acceptance of {value}");
        };
        let amiss::invocation::Command::Scan(invocation) = *command else {
            panic!("expected a scan command for {value}");
        };
        assert_eq!(invocation.format, expected);
    }

    for malformed in [
        with(&valid_pair(), &["--format", "yaml"]),
        with(&valid_pair(), &["--format", "Json"]),
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

#[test]
fn the_render_form_requires_one_non_json_projection() {
    for (value, expected) in [
        ("human", OutputFormat::Human),
        ("sarif", OutputFormat::Sarif),
        ("codequality", OutputFormat::CodeQuality),
    ] {
        let Outcome::Accepted(command) = parse(&argv(&[
            "render",
            "--report",
            "report.json",
            "--format",
            value,
        ])) else {
            panic!("expected render {value} acceptance");
        };
        let amiss::invocation::Command::Render(render) = *command else {
            panic!("expected a render command");
        };
        assert_eq!(render.report, std::path::Path::new("report.json"));
        assert_eq!(render.format, expected);
    }

    for tokens in [
        argv(&["render", "--report", "report.json"]),
        argv(&["render", "--report", "report.json", "--format", "json"]),
        argv(&[
            "render",
            "--report",
            "report.json",
            "--format",
            "human",
            "--repo",
            ".",
        ]),
        argv(&[
            "external-plan",
            "--report",
            "report.json",
            "--format",
            "codequality",
        ]),
    ] {
        assert_eq!(
            rejected_codes(parse(&tokens)),
            vec![Code::InvalidInvocation],
            "tokens {tokens:?}"
        );
    }
}

#[test]
fn the_refs_form_accepts_text_or_canonical_path_bytes() {
    for (flag, value, expected) in [
        ("--target", "docs/a.md", b"docs/a.md".as_slice()),
        (
            "--target-bytes-hex",
            "646f63732fff2e6d64",
            b"docs/\xff.md".as_slice(),
        ),
    ] {
        let Outcome::Accepted(command) = parse(&argv(&[
            "refs",
            "--report",
            "report.json",
            flag,
            value,
            "--format",
            "json",
        ])) else {
            panic!("expected refs acceptance");
        };
        let amiss::invocation::Command::Refs(refs) = *command else {
            panic!("expected a refs command");
        };
        assert_eq!(refs.report, std::path::Path::new("report.json"));
        assert_eq!(refs.target.as_bytes(), expected);
        assert_eq!(refs.format, OutputFormat::Json);
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

fn without_value(base: &[String], option: &str) -> Vec<String> {
    let mut tokens = base.to_vec();
    if let Some(at) = tokens.iter().position(|token| token == option) {
        tokens.remove(at.saturating_add(1));
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

#[expect(clippy::panic, reason = "test harness unwrap")]
fn scan_of(outcome: Outcome) -> amiss::invocation::Invocation {
    let Outcome::Accepted(command) = outcome else {
        panic!("expected acceptance");
    };
    let amiss::invocation::Command::Scan(invocation) = *command else {
        panic!("expected a scan command");
    };
    *invocation
}

/// The dialect grammar: an explicit flag names a known dialect and rides the
/// identity triple; the known-host table fills the default; the github
/// An identity on a host outside the known table is refused, never
/// silently external; naming the dialect opens it, nested owners intact.
#[test]
fn refuses_an_unknown_host_without_a_dialect() {
    let identity = with(
        &valid_pair(),
        &[
            "--repository",
            "gitlab.example/group/subgroup/repo",
            "--ref",
            "refs/heads/main",
            "--default-branch-ref",
            "refs/heads/main",
        ],
    );
    assert_eq!(
        rejected_codes(parse_tokens(&identity)),
        vec![Code::InvalidEvent]
    );

    let flagged = scan_of(parse_tokens(&with(&identity, &["--forge", "gitlab"])));
    assert_eq!(
        flagged.identity.unwrap().repository.owner(),
        "group/subgroup"
    );
    assert_eq!(flagged.forge, Some(ForgeDialect::Gitlab));
}

/// dialect refuses a nested owner it could never match.
#[test]
fn classifies_the_forge_dialect_grammar() {
    let identity = |host_triple: &str| {
        with(
            &valid_pair(),
            &[
                "--repository",
                host_triple,
                "--ref",
                "refs/heads/main",
                "--default-branch-ref",
                "refs/heads/main",
            ],
        )
    };

    for (repository, expected) in [
        ("github.com/acme/repo", ForgeDialect::Github),
        ("codeberg.org/acme/repo", ForgeDialect::Gitea),
        ("bitbucket.org/acme/repo", ForgeDialect::BitbucketCloud),
    ] {
        assert_eq!(
            scan_of(parse_tokens(&identity(repository))).forge,
            Some(expected),
            "{repository} selects its known-host dialect"
        );
    }

    let explicit = with(
        &identity("ghes.corp.example/acme/repo"),
        &["--forge", "github"],
    );
    let ghes = scan_of(parse_tokens(&explicit));
    assert_eq!(ghes.forge, Some(ForgeDialect::Github));
    assert_eq!(
        ghes.identity.unwrap().repository.host(),
        "ghes.corp.example"
    );

    assert_eq!(
        rejected_codes(parse_tokens(&identity("github.com/group/subgroup/repo"))),
        vec![Code::InvalidEvent],
        "the github dialect cannot match a nested owner"
    );
    assert_eq!(
        rejected_codes(parse_tokens(&with(
            &identity("git.example.internal/group/sub/repo"),
            &["--forge", "gitea"],
        ))),
        vec![Code::InvalidEvent],
        "the gitea dialect cannot match a nested owner either"
    );
    assert_eq!(
        rejected_codes(parse_tokens(&with(
            &identity("bitbucket.example/group/sub/repo"),
            &["--forge", "bitbucket-cloud"],
        ))),
        vec![Code::InvalidEvent],
        "the Bitbucket Cloud dialect cannot match a nested owner"
    );
    assert_eq!(
        rejected_codes(parse_tokens(&with(
            &identity("ghes.corp.example/group/sub/repo"),
            &["--forge", "github"],
        ))),
        vec![Code::InvalidEvent],
        "the explicit github dialect refuses a nested owner too"
    );

    assert_eq!(
        rejected_codes(parse_tokens(&with(&valid_pair(), &["--forge", "github"]))),
        vec![Code::InvalidInvocation],
        "a dialect without an identity triple is orphaned"
    );
    assert_eq!(
        rejected_codes(parse_tokens(&with(
            &identity("github.com/acme/repo"),
            &["--forge", "sourcehut"],
        ))),
        vec![Code::InvalidInvocation],
        "an unknown dialect is a grammar violation"
    );
    assert_eq!(
        rejected_codes(parse_tokens(&with(
            &identity("github.com/acme/repo"),
            &["--forge", "github", "--forge", "github"],
        ))),
        vec![Code::InvalidInvocation]
    );
    assert_eq!(
        rejected_codes(parse_tokens(&with(
            &identity("github.com/acme/repo"),
            &["--forge"],
        ))),
        vec![Code::InvalidInvocation]
    );
}

/// A second form of the grammar, not an option: only the whole argv matches.
#[test]
fn the_version_form_is_the_entire_argument_vector() {
    assert_eq!(parse(&argv(&["--version"])), Outcome::Version);
    for tokens in [
        vec!["--version", "--version"],
        vec!["--version", "--format", "human"],
        vec!["check", "--version"],
        vec!["--Version"],
    ] {
        assert_ne!(
            parse(&argv(&tokens)),
            Outcome::Version,
            "{tokens:?} is not the version form"
        );
    }
}
