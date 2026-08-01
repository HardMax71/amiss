#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration harness over asserted fixture shapes"
)]

amiss_fixtures::bounded_memory!();

use std::fs;
use std::path::Path;
use std::process::ExitStatus;

use amiss_bootstrap::supervise::{
    AcceptanceDefect, Defect, Expectations, SealedControlExpectation, SealedExpectations,
    Supervised, accept, settle,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, TrustedTimeStatement};
use amiss_wire::digest::hj;
use amiss_wire::json::{Value, canonical, parse};
use amiss_wire::model::RepositoryIdentity;
use amiss_wire::report::{MACHINE_JSON_BYTES, PAYLOAD_SCHEMA};
use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;

const CANDIDATE_REF: &str = "refs/heads/topic";
const TARGET_REF: &str = "refs/heads/main";
const INSTANT: &str = "2026-07-12T10:00:00Z";
const VALID_UNTIL: &str = "2026-07-12T10:05:00Z";
const PROVIDER: &str = "gitlab-ci";
const RUN_ID: &str = "pipeline/01J2Z9-7";
const ATTEMPT: u64 = 2;
const FLOOR_DIGEST: &str =
    "sha256:4444444444444444444444444444444444444444444444444444444444444444";
const TRUST_SOURCE: &str = "external-required-check";
const FOREIGN_DIGEST: &str =
    "sha256:5555555555555555555555555555555555555555555555555555555555555555";

fn example(name: &str) -> Value {
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/examples")
            .join(name),
    )
    .unwrap();
    parse(&bytes).unwrap()
}

fn entry<'value>(value: &'value mut Value, key: &str) -> &'value mut Value {
    let Value::Object(members) = value else {
        panic!("not an object");
    };
    members
        .iter_mut()
        .find(|(name, _)| name == key)
        .map(|(_, member)| member)
        .expect("a present member")
}

fn set(value: &mut Value, key: &str, member: Value) {
    let Value::Object(members) = value else {
        panic!("not an object");
    };
    if let Some(slot) = members.iter_mut().find(|(name, _)| name == key) {
        slot.1 = member;
        return;
    }
    let at = members
        .iter()
        .position(|(name, _)| name.as_str() > key)
        .unwrap_or(members.len());
    members.insert(at, (key.to_owned(), member));
}

fn text(value: &Value, key: &str) -> String {
    let Value::Object(members) = value else {
        panic!("not an object");
    };
    match members.iter().find(|(name, _)| name == key) {
        Some((_, Value::String(text))) => text.clone(),
        _ => panic!("no text member {key}"),
    }
}

fn string(raw: &str) -> Value {
    Value::String(raw.to_owned())
}

type Patch = Box<dyn FnOnce(&mut Value)>;
type ExpectationPatch = Box<dyn FnOnce(&mut SealedExpectations)>;

/// One deviation from the sealed golden. `pre` edits the payload before the
/// statement is bound to it, so bindings stay consistent with the edit;
/// `post` edits it afterwards, which is how a single internal binding is
/// broken; `expect` edits only the wrapper's captured side.
#[derive(Default)]
struct Deviation {
    pre: Option<Patch>,
    statement_ref: Option<&'static str>,
    statement_identity: Option<&'static str>,
    statement_instant: Option<&'static str>,
    post: Option<Patch>,
    expect: Option<ExpectationPatch>,
}

impl Deviation {
    fn pre(patch: impl FnOnce(&mut Value) + 'static) -> Self {
        Self {
            pre: Some(Box::new(patch)),
            ..Self::default()
        }
    }

    fn post(patch: impl FnOnce(&mut Value) + 'static) -> Self {
        Self {
            post: Some(Box::new(patch)),
            ..Self::default()
        }
    }

    fn expect(patch: impl FnOnce(&mut SealedExpectations) + 'static) -> Self {
        Self {
            expect: Some(Box::new(patch)),
            ..Self::default()
        }
    }
}

fn identity_digest(evaluation: &Value) -> String {
    let Value::Object(members) = evaluation else {
        panic!("not an object");
    };
    let mut identity: Vec<(String, Value)> = members
        .iter()
        .filter(|(name, _)| name != "evaluation_instant" && name != "trusted_time")
        .cloned()
        .collect();
    identity.push((
        "schema".to_owned(),
        Value::String(CANDIDATE_IDENTITY_DOMAIN.to_owned()),
    ));
    hj(CANDIDATE_IDENTITY_DOMAIN, &Value::Object(identity)).to_string()
}

fn statement_value(repository: &Value, ties: &StatementTies, identity: &str) -> Value {
    let mut statement = Value::Object(Vec::new());
    set(
        &mut statement,
        "schema",
        string("amiss/scanner-trusted-time-statement"),
    );
    set(
        &mut statement,
        "controller",
        string("external-required-check-clock"),
    );
    set(&mut statement, "repository", repository.clone());
    set(
        &mut statement,
        "ref",
        string(ties.ref_name.unwrap_or(TARGET_REF)),
    );
    set(
        &mut statement,
        "candidate_identity_digest",
        string(ties.identity.unwrap_or(identity)),
    );
    set(&mut statement, "provider", string(PROVIDER));
    set(&mut statement, "provider_run_id", string(RUN_ID));
    set(
        &mut statement,
        "provider_run_attempt",
        Value::Integer(i64::try_from(ATTEMPT).unwrap()),
    );
    set(
        &mut statement,
        "evaluation_instant",
        string(ties.instant.unwrap_or(INSTANT)),
    );
    set(&mut statement, "valid_until", string(VALID_UNTIL));
    statement
}

struct StatementTies {
    ref_name: Option<&'static str>,
    identity: Option<&'static str>,
    instant: Option<&'static str>,
}

fn verified_control(digest: &str) -> Value {
    let mut control = Value::Object(Vec::new());
    set(&mut control, "status", string("verified"));
    set(&mut control, "digest", string(digest));
    set(&mut control, "trust_source", string(TRUST_SOURCE));
    control
}

/// The sealed golden with one deviation applied, and the expectations the
/// wrapper would hold against it. With no deviation, `accept` admits it.
fn golden(deviation: Deviation) -> (Vec<u8>, Expectations) {
    let Deviation {
        pre,
        statement_ref,
        statement_identity,
        statement_instant,
        post,
        expect,
    } = deviation;
    let ties = StatementTies {
        ref_name: statement_ref,
        identity: statement_identity,
        instant: statement_instant,
    };
    let mut envelope = example("scanner-report.json");
    let payload = entry(&mut envelope, "payload");
    let evaluation = entry(payload, "evaluation");
    set(evaluation, "candidate_ref", string(CANDIDATE_REF));
    set(evaluation, "target_ref", string(TARGET_REF));
    set(evaluation, "trusted_time", Value::Bool(true));
    set(evaluation, "evaluation_instant", string(INSTANT));
    if let Some(patch) = pre {
        patch(payload);
    }

    let evaluation = entry(payload, "evaluation");
    let identity = identity_digest(evaluation);
    let repository_value = entry(evaluation, "repository").clone();
    let statement = statement_value(&repository_value, &ties, &identity);
    let statement_digest = TrustedTimeStatement::parse(&canonical(&statement))
        .expect("a valid statement fixture")
        .digest
        .to_string();

    let descriptor = example("scanner-execution-constraint.json");
    let constraint_digest = ExecutionConstraintDescriptor::parse(&canonical(&descriptor))
        .expect("a valid constraint fixture")
        .digest
        .to_string();

    seal_controls(
        payload,
        descriptor,
        &constraint_digest,
        statement,
        &statement_digest,
    );
    if let Some(patch) = post {
        patch(payload);
    }

    let engine_digest = text(entry(payload, "engine"), "engine_digest");
    let evaluation = entry(payload, "evaluation");
    let base_commit = text(entry(evaluation, "base"), "commit_oid");
    let candidate_commit = text(entry(evaluation, "candidate"), "commit_oid");
    let repository = RepositoryIdentity::new(
        text(&repository_value, "host"),
        text(&repository_value, "owner"),
        text(&repository_value, "name"),
    )
    .unwrap();

    let digest = hj(PAYLOAD_SCHEMA, entry(&mut envelope, "payload")).to_string();
    set(&mut envelope, "payload_digest", string(&digest));
    let mut wire = canonical(&envelope);
    wire.push(b'\n');

    let mut sealed = sealed_expectations(repository, identity, constraint_digest, statement_digest);
    if let Some(patch) = expect {
        patch(&mut sealed);
    }
    let expectations = Expectations {
        engine_digest,
        base_commit,
        candidate_commit: Some(candidate_commit),
        sealed: Some(sealed),
    };
    (wire, expectations)
}

fn seal_controls(
    payload: &mut Value,
    descriptor: Value,
    constraint_digest: &str,
    statement: Value,
    statement_digest: &str,
) {
    let controls = entry(payload, "controls");
    set(
        controls,
        "organization_floor",
        verified_control(FLOOR_DIGEST),
    );
    let mut constraint = Value::Object(Vec::new());
    set(&mut constraint, "status", string("verified"));
    set(&mut constraint, "descriptor", descriptor);
    set(
        &mut constraint,
        "descriptor_digest",
        string(constraint_digest),
    );
    set(&mut constraint, "trust_source", string(TRUST_SOURCE));
    set(controls, "execution_constraint", constraint);
    let mut trusted = Value::Object(Vec::new());
    set(&mut trusted, "status", string("verified"));
    set(
        &mut trusted,
        "trust_source",
        string("external-required-check"),
    );
    set(&mut trusted, "statement", statement);
    set(&mut trusted, "statement_digest", string(statement_digest));
    set(controls, "trusted_time_source", trusted);
}

fn sealed_expectations(
    repository: RepositoryIdentity,
    identity: String,
    constraint_digest: String,
    statement_digest: String,
) -> SealedExpectations {
    SealedExpectations {
        profile: "observe".to_owned(),
        candidate_ref: CANDIDATE_REF.to_owned(),
        target_ref: TARGET_REF.to_owned(),
        repository,
        provider: PROVIDER.to_owned(),
        provider_run_id: RUN_ID.to_owned(),
        provider_run_attempt: ATTEMPT,
        candidate_identity_digest: identity,
        organization_floor: Some(SealedControlExpectation {
            digest: FLOOR_DIGEST.to_owned(),
            trust_source: TRUST_SOURCE.to_owned(),
        }),
        debt_snapshot: None,
        waiver_bundle: None,
        execution_constraint: SealedControlExpectation {
            digest: constraint_digest,
            trust_source: TRUST_SOURCE.to_owned(),
        },
        trusted_time_digest: statement_digest,
    }
}

fn refused(deviation: Deviation) -> AcceptanceDefect {
    let (wire, expectations) = golden(deviation);
    accept(&wire, &expectations).expect_err("one deviation must refuse the envelope")
}

#[cfg(unix)]
fn exited(code: i32) -> ExitStatus {
    std::os::unix::process::ExitStatusExt::from_raw(code << 8)
}

#[cfg(windows)]
fn exited(code: i32) -> ExitStatus {
    std::os::windows::process::ExitStatusExt::from_raw(u32::try_from(code).unwrap())
}

#[test]
fn the_sealed_golden_clears_acceptance_and_settlement() {
    let (wire, expectations) = golden(Deviation::default());
    assert_eq!(accept(&wire, &expectations), Ok(0));
    assert_eq!(
        settle(&Supervised::Completed(exited(0)), &wire, &expectations),
        Ok(0)
    );
}

#[test]
fn a_complete_block_report_is_accepted_at_class_one() {
    let (wire, expectations) = golden(Deviation::pre(|payload| {
        set(entry(payload, "result"), "exit_code", Value::Integer(1));
        set(entry(payload, "result"), "status", string("block"));
    }));
    assert_eq!(accept(&wire, &expectations), Ok(1));
}

#[test]
fn the_sealed_identity_binds_refs_time_and_candidate() {
    assert_eq!(
        refused(Deviation::expect(|sealed| {
            sealed.candidate_ref = "refs/heads/other".to_owned();
        })),
        AcceptanceDefect::SealedIdentity
    );
    assert_eq!(
        refused(Deviation::expect(|sealed| {
            sealed.target_ref = "refs/heads/other".to_owned();
        })),
        AcceptanceDefect::SealedIdentity
    );
    assert_eq!(
        refused(Deviation::post(|payload| {
            set(
                entry(payload, "evaluation"),
                "trusted_time",
                Value::Bool(false),
            );
        })),
        AcceptanceDefect::SealedIdentity
    );
    assert_eq!(
        refused(Deviation::pre(|payload| {
            let candidate = entry(entry(payload, "evaluation"), "candidate");
            set(candidate, "kind", string("git-tag"));
        })),
        AcceptanceDefect::CandidateIdentity,
        "a wrong kind is a candidate defect even when every binding is consistent with it"
    );
}

#[test]
fn the_constraint_echo_binds_status_digest_source_and_descriptor() {
    let cases: [(&str, Patch); 4] = [
        (
            "status",
            Box::new(|constraint| set(constraint, "status", string("unverified"))),
        ),
        (
            "digest text",
            Box::new(|constraint| set(constraint, "descriptor_digest", string(FOREIGN_DIGEST))),
        ),
        (
            "trust source",
            Box::new(|constraint| set(constraint, "trust_source", string("none"))),
        ),
        (
            "embedded descriptor",
            Box::new(|constraint| {
                set(
                    entry(constraint, "descriptor"),
                    "required_status_name",
                    string("amiss / other"),
                );
            }),
        ),
    ];
    for (reason, patch) in cases {
        let deviation = Deviation::post(move |payload| {
            patch(entry(entry(payload, "controls"), "execution_constraint"));
        });
        assert_eq!(
            refused(deviation),
            AcceptanceDefect::SealedControls,
            "{reason}"
        );
    }
}

#[test]
fn the_time_echo_binds_every_statement_fact() {
    let post_cases: [(&str, Patch); 3] = [
        (
            "status",
            Box::new(|trusted| set(trusted, "status", string("unverified"))),
        ),
        (
            "trust source",
            Box::new(|trusted| set(trusted, "trust_source", string("provider"))),
        ),
        (
            "digest text",
            Box::new(|trusted| set(trusted, "statement_digest", string(FOREIGN_DIGEST))),
        ),
    ];
    for (reason, patch) in post_cases {
        let deviation = Deviation::post(move |payload| {
            patch(entry(entry(payload, "controls"), "trusted_time_source"));
        });
        assert_eq!(
            refused(deviation),
            AcceptanceDefect::SealedControls,
            "{reason}"
        );
    }

    let mut agree_on_wrong = Deviation::post(|payload| {
        let trusted = entry(entry(payload, "controls"), "trusted_time_source");
        set(trusted, "statement_digest", string(FOREIGN_DIGEST));
    });
    agree_on_wrong.expect = Some(Box::new(|sealed| {
        sealed.trusted_time_digest = FOREIGN_DIGEST.to_owned();
    }));
    assert_eq!(
        refused(agree_on_wrong),
        AcceptanceDefect::SealedControls,
        "the statement's own digest must recompute, not merely match the echoed text"
    );

    let expect_cases: [(&str, ExpectationPatch); 4] = [
        (
            "provider",
            Box::new(|sealed| sealed.provider = "github".to_owned()),
        ),
        (
            "run id",
            Box::new(|sealed| sealed.provider_run_id = "pipeline/other".to_owned()),
        ),
        (
            "attempt",
            Box::new(|sealed| sealed.provider_run_attempt = ATTEMPT + 1),
        ),
        (
            "repository",
            Box::new(|sealed| {
                sealed.repository = RepositoryIdentity::new(
                    "git.example.internal".to_owned(),
                    "group/subgroup".to_owned(),
                    "other".to_owned(),
                )
                .unwrap();
            }),
        ),
    ];
    for (reason, patch) in expect_cases {
        let deviation = Deviation {
            expect: Some(patch),
            ..Deviation::default()
        };
        assert_eq!(
            refused(deviation),
            AcceptanceDefect::SealedControls,
            "{reason}"
        );
    }

    assert_eq!(
        refused(Deviation {
            statement_ref: Some(CANDIDATE_REF),
            ..Deviation::default()
        }),
        AcceptanceDefect::SealedControls,
        "the statement must name the target ref"
    );
    assert_eq!(
        refused(Deviation {
            statement_identity: Some(FOREIGN_DIGEST),
            ..Deviation::default()
        }),
        AcceptanceDefect::SealedControls,
        "the statement must bind the recomputed candidate identity"
    );
    assert_eq!(
        refused(Deviation {
            statement_instant: Some("2026-07-12T10:01:00Z"),
            ..Deviation::default()
        }),
        AcceptanceDefect::SealedControls,
        "the report's instant must be the statement's"
    );
}

#[test]
fn the_sandbox_echo_admits_only_the_self_asserted_row() {
    let cases: [(&str, Patch); 3] = [
        (
            "assurance",
            Box::new(|sandbox| set(sandbox, "assurance", string("external"))),
        ),
        (
            "enforcement source",
            Box::new(|sandbox| set(sandbox, "enforcement_source", string("remote"))),
        ),
        (
            "verification",
            Box::new(|sandbox| set(sandbox, "verification", string("attested"))),
        ),
    ];
    for (reason, patch) in cases {
        let deviation = Deviation::post(move |payload| {
            patch(entry(entry(payload, "controls"), "sandbox"));
        });
        assert_eq!(
            refused(deviation),
            AcceptanceDefect::SealedControls,
            "{reason}"
        );
    }
}

#[test]
fn an_optional_control_matches_its_expectation_on_every_fact() {
    let supplied: [(&str, Patch); 3] = [
        (
            "status",
            Box::new(|floor| set(floor, "status", string("none"))),
        ),
        (
            "digest",
            Box::new(|floor| set(floor, "digest", string(FOREIGN_DIGEST))),
        ),
        (
            "trust source",
            Box::new(|floor| set(floor, "trust_source", string("none"))),
        ),
    ];
    for (reason, patch) in supplied {
        let deviation = Deviation::post(move |payload| {
            patch(entry(entry(payload, "controls"), "organization_floor"));
        });
        assert_eq!(
            refused(deviation),
            AcceptanceDefect::SealedControls,
            "{reason}"
        );
    }

    let absent: [(&str, Patch); 3] = [
        (
            "status",
            Box::new(|debt| set(debt, "status", string("verified"))),
        ),
        (
            "digest",
            Box::new(|debt| set(debt, "digest", string(FLOOR_DIGEST))),
        ),
        (
            "trust source",
            Box::new(|debt| set(debt, "trust_source", string("provider"))),
        ),
    ];
    for (reason, patch) in absent {
        let deviation = Deviation::post(move |payload| {
            patch(entry(entry(payload, "controls"), "debt_snapshot"));
        });
        assert_eq!(
            refused(deviation),
            AcceptanceDefect::SealedControls,
            "an unsupplied control must echo as none: {reason}"
        );
    }
}

#[test]
fn the_wire_ceiling_is_exclusive_and_checked_before_acceptance() {
    let (_, expectations) = golden(Deviation::default());
    let mut stdout = vec![b'x'; usize::try_from(MACHINE_JSON_BYTES).unwrap()];
    assert_eq!(
        settle(&Supervised::Completed(exited(0)), &stdout, &expectations),
        Err(Defect::Acceptance(AcceptanceDefect::Noncanonical)),
        "bytes exactly at the ceiling reach acceptance"
    );
    stdout.push(b'x');
    assert_eq!(
        settle(&Supervised::Completed(exited(0)), &stdout, &expectations),
        Err(Defect::Oversize),
        "one byte past the ceiling never reaches acceptance"
    );
}
