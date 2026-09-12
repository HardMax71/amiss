#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration harness over asserted fixture shapes"
)]

use sha2::Digest as _;
use std::fs;
use std::path::Path;
use std::process::ExitStatus;

use amiss_bootstrap::supervise::{
    AcceptanceDefect, Defect, Expectations, SealedControlExpectation, SealedExpectations,
    Supervised, accept, settle,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, TrustedTimeStatement};

use amiss_wire::model::RepositoryIdentity;
use amiss_wire::report::model::{BaseSnapshot, Evaluation, ReportEnvelope, ReportStatus, Snapshot};
use amiss_wire::report::{MACHINE_JSON_BYTES, PAYLOAD_SCHEMA};
use amiss_wire::requests::{CANDIDATE_IDENTITY_DOMAIN, CandidateSnapshot, RequestTrust};
use serde_json::Value;

mod controls;
mod identity;
mod semantic;

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
    serde_json::from_slice::<Value>(&bytes).unwrap()
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
        .filter(|(name, _)| {
            name.as_str() != "evaluation_instant" && name.as_str() != "trusted_time"
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    identity.push(("schema".to_owned(), Value::from(CANDIDATE_IDENTITY_DOMAIN)));
    amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(CANDIDATE_IDENTITY_DOMAIN)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&Value::from_iter(identity)).unwrap())
            .finalize()
            .0,
    )
    .to_string()
}

fn statement_value(repository: &Value, ties: &StatementTies, identity: &str) -> Value {
    serde_json::json!({
        "schema": "amiss/scanner-trusted-time-statement",
        "controller": "external-required-check-clock",
        "repository": repository,
        "ref": ties.ref_name.unwrap_or(TARGET_REF),
        "candidate_identity_digest": ties.identity.unwrap_or(identity),
        "provider": PROVIDER,
        "provider_run_id": RUN_ID,
        "provider_run_attempt": ATTEMPT,
        "evaluation_instant": ties.instant.unwrap_or(INSTANT),
        "valid_until": VALID_UNTIL,
    })
}

struct StatementTies {
    ref_name: Option<&'static str>,
    identity: Option<&'static str>,
    instant: Option<&'static str>,
}

fn verified_control(digest: &str) -> Value {
    serde_json::json!({
        "status": "verified",
        "digest": digest,
        "trust_source": TRUST_SOURCE,
    })
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
    let report: ReportEnvelope = serde::Deserialize::deserialize(&envelope).unwrap();
    let payload = envelope.get_mut("payload").expect("fixture member exists");
    let evaluation = (payload)
        .get_mut("evaluation")
        .expect("fixture member exists");
    (evaluation)["candidate_ref"] = Value::from(CANDIDATE_REF);
    (evaluation)["target_ref"] = Value::from(TARGET_REF);
    (evaluation)["trusted_time"] = Value::Bool(true);
    (evaluation)["evaluation_instant"] = Value::from(INSTANT);
    if let Some(patch) = pre {
        patch(payload);
    }

    let evaluation = (payload)
        .get_mut("evaluation")
        .expect("fixture member exists");
    let identity = identity_digest(evaluation);
    let repository_value = (evaluation)
        .get_mut("repository")
        .expect("fixture member exists")
        .clone();
    let statement = statement_value(&repository_value, &ties, &identity);
    let parsed_statement: TrustedTimeStatement =
        serde::Deserialize::deserialize(&statement).expect("a valid statement fixture");
    parsed_statement.validate().unwrap();
    let statement_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/scanner-trusted-time-statement")
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&parsed_statement).unwrap())
            .finalize()
            .0,
    )
    .to_string();

    let descriptor = example("scanner-execution-constraint.json");
    let constraint: ExecutionConstraintDescriptor =
        serde::Deserialize::deserialize(&descriptor).expect("a valid constraint fixture");
    constraint.validate().unwrap();
    let constraint_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/scanner-execution-constraint")
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&constraint).unwrap())
            .finalize()
            .0,
    )
    .to_string();

    seal_controls(
        payload,
        &descriptor,
        &constraint_digest,
        &statement,
        &statement_digest,
    );
    if let Some(patch) = post {
        patch(payload);
    }

    let digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(payload).unwrap())
            .finalize()
            .0,
    )
    .to_string();
    (&mut envelope)["payload_digest"] = Value::from((digest).as_str());
    let mut wire = serde_json_canonicalizer::to_vec(&envelope).unwrap();
    wire.push(b'\n');

    let mut expectations =
        sealed_expectations(report, &identity, &constraint_digest, &statement_digest);
    if let Some(patch) = expect {
        patch(expectations.sealed.as_mut().unwrap());
    }
    (wire, expectations)
}

fn seal_controls(
    payload: &mut Value,
    descriptor: &Value,
    constraint_digest: &str,
    statement: &Value,
    statement_digest: &str,
) {
    let controls = (payload)
        .get_mut("controls")
        .expect("fixture member exists");
    (controls)["semantic_evidence"] = Value::Array(Vec::new());
    (controls)["organization_floor"] = verified_control(FLOOR_DIGEST);
    controls["execution_constraint"] = serde_json::json!({
        "status": "verified",
        "descriptor": descriptor,
        "descriptor_digest": constraint_digest,
        "trust_source": TRUST_SOURCE,
    });
    controls["trusted_time_source"] = serde_json::json!({
        "status": "verified",
        "trust_source": TRUST_SOURCE,
        "statement": statement,
        "statement_digest": statement_digest,
    });
}

fn sealed_expectations(
    report: ReportEnvelope,
    identity: &str,
    constraint_digest: &str,
    statement_digest: &str,
) -> Expectations {
    let Evaluation::Resolved(evaluation) = report.payload.evaluation else {
        panic!("a resolved fixture evaluation");
    };
    let BaseSnapshot::Git(base) = evaluation.base else {
        panic!("a fixture base commit");
    };
    let Snapshot::Available(CandidateSnapshot::Git(candidate)) = evaluation.candidate else {
        panic!("a fixture candidate commit");
    };
    let sealed = SealedExpectations {
        profile: amiss_wire::controls::Profile::Observe,
        candidate_ref: CANDIDATE_REF.parse().unwrap(),
        target_ref: TARGET_REF.parse().unwrap(),
        repository: evaluation.repository.unwrap(),
        provider: PROVIDER.to_owned(),
        provider_run_id: RUN_ID.to_owned(),
        provider_run_attempt: ATTEMPT,
        candidate_identity_digest: identity.parse().unwrap(),
        organization_floor: Some(SealedControlExpectation {
            digest: FLOOR_DIGEST.parse().unwrap(),
            trust_source: RequestTrust::ExternalRequiredCheck,
        }),
        debt_snapshot: None,
        waiver_bundle: None,
        execution_constraint: SealedControlExpectation {
            digest: constraint_digest.parse().unwrap(),
            trust_source: RequestTrust::ExternalRequiredCheck,
        },
        trusted_time_digest: statement_digest.parse().unwrap(),
        semantic_evidence: Vec::new(),
    };
    Expectations {
        engine_digest: report.payload.engine.engine_digest,
        base_commit: base.commit_oid,
        candidate_commit: Some(candidate.commit_oid),
        sealed: Some(sealed),
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
        ((payload).get_mut("result").expect("fixture member exists"))["exit_code"] = Value::from(1);
        ((payload).get_mut("result").expect("fixture member exists"))["status"] =
            Value::from(ReportStatus::Fail.as_ref());
    }));
    assert_eq!(accept(&wire, &expectations), Ok(1));
}

#[test]
fn the_sealed_identity_binds_refs_time_and_candidate() {
    assert_eq!(
        refused(Deviation::expect(|sealed| {
            sealed.candidate_ref = "refs/heads/other".parse().unwrap();
        })),
        AcceptanceDefect::SealedIdentity
    );
    assert_eq!(
        refused(Deviation::expect(|sealed| {
            sealed.target_ref = "refs/heads/other".parse().unwrap();
        })),
        AcceptanceDefect::SealedIdentity
    );
    assert_eq!(
        refused(Deviation::post(|payload| {
            ((payload)
                .get_mut("evaluation")
                .expect("fixture member exists"))["trusted_time"] = Value::Bool(false);
        })),
        AcceptanceDefect::SealedIdentity
    );
    assert_eq!(
        refused(Deviation::pre(|payload| {
            let candidate = ((payload)
                .get_mut("evaluation")
                .expect("fixture member exists"))
            .get_mut("candidate")
            .expect("fixture member exists");
            (candidate)["kind"] = Value::from("git-tag");
        })),
        AcceptanceDefect::Shape,
        "an unknown kind is rejected before its claimed bindings are evaluated"
    );
}

#[test]
fn the_constraint_echo_binds_status_digest_source_and_descriptor() {
    let cases: [(&str, Patch, AcceptanceDefect); 4] = [
        (
            "status",
            Box::new(|constraint| (constraint)["status"] = Value::from("unverified")),
            AcceptanceDefect::Shape,
        ),
        (
            "digest text",
            Box::new(|constraint| (constraint)["descriptor_digest"] = Value::from(FOREIGN_DIGEST)),
            AcceptanceDefect::SealedControls,
        ),
        (
            "trust source",
            Box::new(|constraint| {
                (constraint)["trust_source"] =
                    Value::from(RequestTrust::OrganizationPolicy.as_ref());
            }),
            AcceptanceDefect::SealedControls,
        ),
        (
            "embedded descriptor",
            Box::new(|constraint| {
                ((constraint)
                    .get_mut("descriptor")
                    .expect("fixture member exists"))["required_status_name"] =
                    Value::from("amiss / other");
            }),
            AcceptanceDefect::SealedControls,
        ),
    ];
    for (reason, patch, expected) in cases {
        let deviation = Deviation::post(move |payload| {
            patch(
                ((payload)
                    .get_mut("controls")
                    .expect("fixture member exists"))
                .get_mut("execution_constraint")
                .expect("fixture member exists"),
            );
        });
        assert_eq!(refused(deviation), expected, "{reason}");
    }
}

#[test]
fn the_time_echo_binds_every_statement_fact() {
    let post_cases: [(&str, Patch, AcceptanceDefect); 3] = [
        (
            "status",
            Box::new(|trusted| trusted["status"] = Value::from("unverified")),
            AcceptanceDefect::Shape,
        ),
        (
            "trust source",
            Box::new(|trusted| trusted["trust_source"] = Value::from("provider")),
            AcceptanceDefect::Shape,
        ),
        (
            "digest text",
            Box::new(|trusted| trusted["statement_digest"] = Value::from(FOREIGN_DIGEST)),
            AcceptanceDefect::SealedControls,
        ),
    ];
    for (reason, patch, expected) in post_cases {
        let deviation = Deviation::post(move |payload| {
            patch(
                payload
                    .pointer_mut("/controls/trusted_time_source")
                    .expect("fixture member exists"),
            );
        });
        assert_eq!(refused(deviation), expected, "{reason}");
    }

    let mut agree_on_wrong = Deviation::post(|payload| {
        let trusted = payload
            .pointer_mut("/controls/trusted_time_source")
            .expect("fixture member exists");
        trusted["statement_digest"] = Value::from(FOREIGN_DIGEST);
    });
    agree_on_wrong.expect = Some(Box::new(|sealed| {
        sealed.trusted_time_digest = FOREIGN_DIGEST.parse().unwrap();
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

    for deviation in [
        Deviation {
            statement_ref: Some(CANDIDATE_REF),
            ..Deviation::default()
        },
        Deviation {
            statement_identity: Some(FOREIGN_DIGEST),
            ..Deviation::default()
        },
        Deviation {
            statement_instant: Some("2026-07-12T10:01:00Z"),
            ..Deviation::default()
        },
    ] {
        assert_eq!(refused(deviation), AcceptanceDefect::SealedControls);
    }
}

#[test]
fn the_sandbox_echo_admits_only_the_self_asserted_row() {
    let cases: [(&str, Patch, AcceptanceDefect); 3] = [
        (
            "assurance",
            Box::new(|sandbox| {
                (sandbox)["assurance"] = Value::from(
                    amiss_wire::report::model::SandboxAssurance::ProviderVerified.as_ref(),
                );
            }),
            AcceptanceDefect::SealedControls,
        ),
        (
            "enforcement source",
            Box::new(|sandbox| {
                (sandbox)["enforcement_source"] = Value::from(
                    amiss_wire::report::model::SandboxEnforcementSource::ExternalRequiredCheck
                        .as_ref(),
                );
            }),
            AcceptanceDefect::SealedControls,
        ),
        (
            "verification",
            Box::new(|sandbox| (sandbox)["verification"] = Value::from("attested")),
            AcceptanceDefect::Shape,
        ),
    ];
    for (reason, patch, expected) in cases {
        let deviation = Deviation::post(move |payload| {
            patch(
                ((payload)
                    .get_mut("controls")
                    .expect("fixture member exists"))
                .get_mut("sandbox")
                .expect("fixture member exists"),
            );
        });
        assert_eq!(refused(deviation), expected, "{reason}");
    }
}

#[test]
fn an_optional_control_matches_its_expectation_on_every_fact() {
    let supplied: [(&str, Patch); 3] = [
        (
            "status",
            Box::new(|floor| (floor)["status"] = Value::from("none")),
        ),
        (
            "digest",
            Box::new(|floor| (floor)["digest"] = Value::from(FOREIGN_DIGEST)),
        ),
        (
            "trust source",
            Box::new(|floor| (floor)["trust_source"] = Value::from("none")),
        ),
    ];
    for (reason, patch) in supplied {
        let deviation = Deviation::post(move |payload| {
            patch(
                ((payload)
                    .get_mut("controls")
                    .expect("fixture member exists"))
                .get_mut("organization_floor")
                .expect("fixture member exists"),
            );
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
            Box::new(|debt| (debt)["status"] = Value::from("verified")),
        ),
        (
            "digest",
            Box::new(|debt| (debt)["digest"] = Value::from(FLOOR_DIGEST)),
        ),
        (
            "trust source",
            Box::new(|debt| {
                (debt)["trust_source"] = Value::from(RequestTrust::ExternalRequiredCheck.as_ref());
            }),
        ),
    ];
    for (reason, patch) in absent {
        let deviation = Deviation::post(move |payload| {
            patch(
                ((payload)
                    .get_mut("controls")
                    .expect("fixture member exists"))
                .get_mut("debt_snapshot")
                .expect("fixture member exists"),
            );
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
