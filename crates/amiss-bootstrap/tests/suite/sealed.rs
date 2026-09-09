#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration harness over asserted fixture shapes"
)]

use std::process::ExitStatus;
use std::sync::LazyLock;

use amiss_bootstrap::supervise::{
    AcceptanceDefect, Defect, Expectations, SealedControlExpectation, SealedExpectations,
    Supervised, accept, settle,
};
use amiss_fixtures::{SCANNER_REPORT, corrupt, report_bytes};
use amiss_wire::controls::{
    Profile, TrustedTimeController, TrustedTimeSchema, TrustedTimeStatement,
    canonical_execution_constraint, canonical_trusted_time, parse_execution_constraint,
};
use amiss_wire::digest::{Digest, hb, hj_serde};
use amiss_wire::model::RepositoryIdentity;
use amiss_wire::report::{MACHINE_JSON_BYTES, PAYLOAD_SCHEMA, model};
use amiss_wire::requests::{
    CANDIDATE_IDENTITY_DOMAIN, CandidateIdentitySchema, CandidateSnapshot, RequestTrust,
};

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
const FOREIGN_DIGEST: &str =
    "sha256:5555555555555555555555555555555555555555555555555555555555555555";
const GOLDEN_VECTOR: &str =
    "sha256:0af385972adb277af614d3f9b147e09886e3cfd9d03bf3411e72d15412308c2a";

static GOLDEN: LazyLock<(model::ReportEnvelope, Expectations)> = LazyLock::new(|| {
    let mut report: model::ReportEnvelope = serde_json::from_slice(SCANNER_REPORT).unwrap();
    let descriptor = parse_execution_constraint(include_bytes!(
        "../../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    let descriptor_digest = canonical_execution_constraint(&descriptor).unwrap().1;
    let model::Evaluation::Resolved(evaluation) = &mut report.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    evaluation.candidate_ref = Some(CANDIDATE_REF.parse().unwrap());
    evaluation.target_ref = Some(TARGET_REF.parse().unwrap());
    evaluation.trusted_time = true;
    evaluation.evaluation_instant = Some(INSTANT.to_owned().try_into().unwrap());
    let preimage = model::IdentityPreimage {
        evaluation,
        schema: CandidateIdentitySchema::Current,
    };
    let candidate_identity_digest = hj_serde(CANDIDATE_IDENTITY_DOMAIN, |mut writer| {
        serde_json_canonicalizer::to_writer(&preimage, &mut writer)
    })
    .unwrap();
    let repository = evaluation.repository.clone().unwrap();
    let statement = TrustedTimeStatement {
        candidate_identity_digest,
        controller: TrustedTimeController::ExternalRequiredCheckClock,
        evaluation_instant: evaluation.evaluation_instant.clone().unwrap(),
        provider: PROVIDER.to_owned(),
        provider_run_attempt: ATTEMPT,
        provider_run_id: RUN_ID.to_owned(),
        ref_name: TARGET_REF.parse().unwrap(),
        repository: repository.clone(),
        schema: TrustedTimeSchema::Current,
        valid_until: VALID_UNTIL.to_owned().try_into().unwrap(),
    };
    let statement_digest = canonical_trusted_time(&statement).unwrap().1;
    let (
        model::BaseSnapshot::Git(base),
        model::Snapshot::Available(CandidateSnapshot::Git(candidate)),
    ) = (&evaluation.base, &evaluation.candidate)
    else {
        panic!("the fixture describes a Git commit pair");
    };
    let expectations = Expectations {
        engine_digest: report.payload.engine.engine_digest,
        base_commit: base.commit_oid.clone(),
        candidate_commit: Some(candidate.commit_oid.clone()),
        sealed: Some(SealedExpectations {
            profile: Profile::Observe,
            candidate_ref: CANDIDATE_REF.to_owned(),
            target_ref: TARGET_REF.to_owned(),
            repository,
            provider: PROVIDER.to_owned(),
            provider_run_id: RUN_ID.to_owned(),
            provider_run_attempt: ATTEMPT,
            candidate_identity_digest,
            organization_floor: Some(SealedControlExpectation {
                digest: FLOOR_DIGEST.parse().unwrap(),
                trust_source: RequestTrust::ExternalRequiredCheck,
            }),
            debt_snapshot: None,
            waiver_bundle: None,
            execution_constraint: SealedControlExpectation {
                digest: descriptor_digest,
                trust_source: RequestTrust::ExternalRequiredCheck,
            },
            trusted_time_digest: statement_digest,
            semantic_evidence: Vec::new(),
        }),
    };
    let model::Controls::Resolved(controls) = &mut report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    controls.semantic_evidence = Some(Vec::new());
    controls.organization_floor = model::ControlProvenance {
        digest: Some(FLOOR_DIGEST.parse().unwrap()),
        status: model::ControlStatus::Verified,
        trust_source: model::ControlTrustSource::ExternalRequiredCheck,
    };
    controls.execution_constraint = model::ExecutionConstraintProvenance::Verified(Box::new(
        model::VerifiedExecutionConstraint {
            descriptor,
            descriptor_digest,
            status: model::VerifiedControlStatus::Verified,
            trust_source: RequestTrust::ExternalRequiredCheck,
        },
    ));
    controls.trusted_time_source =
        model::TrustedTimeProvenance::Verified(Box::new(model::VerifiedTrustedTime {
            statement,
            statement_digest,
            status: model::VerifiedControlStatus::Verified,
            trust_source: model::TrustedTimeTrustSource::ExternalRequiredCheck,
        }));
    report.payload_digest = hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&report.payload, &mut writer)
    })
    .unwrap();
    (report, expectations)
});

fn bind(candidate_identity_digest: Digest) -> (model::ReportEnvelope, Expectations) {
    let (mut report, mut expectations) = GOLDEN.clone();
    let model::Controls::Resolved(controls) = &mut report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    let model::TrustedTimeProvenance::Verified(trusted) = &mut controls.trusted_time_source else {
        panic!("the fixture has verified time");
    };
    trusted.statement.candidate_identity_digest = candidate_identity_digest;
    trusted.statement_digest = canonical_trusted_time(&trusted.statement).unwrap().1;
    let sealed = expectations.sealed.as_mut().unwrap();
    sealed.candidate_identity_digest = candidate_identity_digest;
    sealed.trusted_time_digest = trusted.statement_digest;
    (report, expectations)
}

fn check(
    wire: &[u8],
    expectations: &Expectations,
    outcome: Result<i64, AcceptanceDefect>,
    identity: &str,
) {
    assert_eq!(accept(wire, expectations), outcome);
    let mut captured = wire.to_vec();
    let sealed = expectations.sealed.as_ref().unwrap();
    captured.extend_from_slice(sealed.candidate_identity_digest.to_string().as_bytes());
    captured.extend_from_slice(sealed.trusted_time_digest.to_string().as_bytes());
    assert_eq!(
        hb("amiss/test-sealed-golden", &captured).to_string(),
        identity
    );
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
    let (report, expectations) = &*GOLDEN;
    let wire = report_bytes(report.clone()).unwrap();
    check(&wire, expectations, Ok(0), GOLDEN_VECTOR);
    assert_eq!(
        settle(&Supervised::Completed(exited(0)), &wire, expectations),
        Ok(0)
    );
}

#[test]
fn a_complete_block_report_is_accepted_at_class_one() {
    let (mut report, expectations) = GOLDEN.clone();
    report.payload.result.exit_code = 1;
    report.payload.result.status = model::ReportStatus::Fail;
    check(
        &report_bytes(report).unwrap(),
        &expectations,
        Ok(1),
        "sha256:c8546e1076618c9ac89e2653c3a573108b53f01560ab628b2fbe4c5b26251091",
    );
}

#[test]
fn the_sealed_identity_binds_refs_time_and_candidate() {
    let (report, expectations) = &*GOLDEN;
    let wire = report_bytes(report.clone()).unwrap();
    let mut cases: [_; 2] = std::array::from_fn(|_| expectations.clone());
    cases[0].sealed.as_mut().unwrap().candidate_ref = "refs/heads/other".to_owned();
    cases[1].sealed.as_mut().unwrap().target_ref = "refs/heads/other".to_owned();
    for expected in cases {
        check(
            &wire,
            &expected,
            Err(AcceptanceDefect::SealedIdentity),
            GOLDEN_VECTOR,
        );
    }
    let mut changed = report.clone();
    let model::Evaluation::Resolved(evaluation) = &mut changed.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    evaluation.trusted_time = false;
    check(
        &report_bytes(changed).unwrap(),
        expectations,
        Err(AcceptanceDefect::SealedIdentity),
        "sha256:bf1193ad31c8bfc876a9c4dbe256723f0e40446f6b220cce543624a546af003b",
    );
    let model::Evaluation::Resolved(evaluation) = &report.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    let model::Snapshot::Available(CandidateSnapshot::Git(snapshot)) = &evaluation.candidate else {
        panic!("the fixture has a Git candidate");
    };
    let candidate = serde_json_canonicalizer::to_string(&evaluation.candidate).unwrap();
    let original = format!(r#""candidate":{candidate}"#);
    let kind = format!(
        r#""kind":{}"#,
        serde_json::to_string(&snapshot.kind).unwrap()
    );
    assert_eq!(candidate.matches(&kind).count(), 1);
    let replacement = format!(
        r#""candidate":{}"#,
        candidate.replacen(&kind, r#""kind":"git-tag""#, 1)
    );
    let preimage = serde_json_canonicalizer::to_string(&model::IdentityPreimage {
        evaluation,
        schema: CandidateIdentitySchema::Current,
    })
    .unwrap();
    assert_eq!(preimage.matches(&original).count(), 1);
    let changed = preimage.replacen(&original, &replacement, 1);
    let (report, expectations) = bind(hb(CANDIDATE_IDENTITY_DOMAIN, changed.as_bytes()));
    check(
        &corrupt(&report, &original, &replacement).unwrap(),
        &expectations,
        Err(AcceptanceDefect::Shape),
        "sha256:fc6dd43bc7f3fc26f830208d4edfe83306193cbcaa5ab4f5ff3177e8121e66bb",
    );
}

#[test]
fn the_constraint_echo_binds_status_digest_source_and_descriptor() {
    let (report, expectations) = &*GOLDEN;
    let model::Controls::Resolved(controls) = &report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    let model::ExecutionConstraintProvenance::Verified(constraint) = &controls.execution_constraint
    else {
        panic!("the fixture has a verified constraint");
    };
    let object = serde_json_canonicalizer::to_string(constraint).unwrap();
    let member = format!(
        r#""status":{}"#,
        serde_json::to_string(&constraint.status).unwrap()
    );
    assert_eq!(object.matches(&member).count(), 1);
    let invalid = object.replacen(&member, r#""status":"unverified""#, 1);
    check(
        &corrupt(report, &object, &invalid).unwrap(),
        expectations,
        Err(AcceptanceDefect::Shape),
        "sha256:e714a8e34146f45f8a763f86c4bbce4cb8aaa5fc5561b4c2ca0c23782081f67d",
    );
    let mut cases: [_; 3] = std::array::from_fn(|_| constraint.clone());
    cases[0].descriptor_digest = FOREIGN_DIGEST.parse().unwrap();
    cases[1].trust_source = RequestTrust::OrganizationPolicy;
    cases[2].descriptor.required_status_name = "amiss / other".to_owned();
    for (constraint, identity) in cases.into_iter().zip([
        "sha256:adeed39f721694f55c09ec90e506abb56ed8b64f4ab3f42c5e8696b72193488a",
        "sha256:78edf14464fd86101098547b7a51bc66f1eccca018c9f40c3944f5cb6f17aedb",
        "sha256:fa7feda0348904b09017ceeb4c3f9f1289d5f3e2733de48c4e2fd3f4ac4947dd",
    ]) {
        let mut changed = report.clone();
        let model::Controls::Resolved(controls) = &mut changed.payload.controls else {
            panic!("the fixture has resolved controls");
        };
        controls.execution_constraint = model::ExecutionConstraintProvenance::Verified(constraint);
        check(
            &report_bytes(changed).unwrap(),
            expectations,
            Err(AcceptanceDefect::SealedControls),
            identity,
        );
    }
}

#[test]
fn the_time_echo_rejects_invalid_provenance_and_stale_digests() {
    let (report, expectations) = &*GOLDEN;
    let model::Controls::Resolved(controls) = &report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    let model::TrustedTimeProvenance::Verified(trusted) = &controls.trusted_time_source else {
        panic!("the fixture has verified time");
    };
    let object = serde_json_canonicalizer::to_string(trusted).unwrap();
    for (original, replacement, identity) in [
        (
            format!(
                r#""status":{}"#,
                serde_json::to_string(&trusted.status).unwrap()
            ),
            r#""status":"unverified""#,
            "sha256:2144b3eccf05a96ee9413263549398b72a575e1912ef75eb75658110193a5d87",
        ),
        (
            format!(
                r#""trust_source":{}"#,
                serde_json::to_string(&trusted.trust_source).unwrap()
            ),
            r#""trust_source":"provider""#,
            "sha256:6f0c5dd77db5dbbab18b9ae56dd59129dd58c787a3d4b474fd9d7c60f71cdc80",
        ),
    ] {
        assert_eq!(object.matches(&original).count(), 1);
        let invalid = object.replacen(&original, replacement, 1);
        check(
            &corrupt(report, &object, &invalid).unwrap(),
            expectations,
            Err(AcceptanceDefect::Shape),
            identity,
        );
    }
    let (mut changed, mut agreed) = GOLDEN.clone();
    let model::Controls::Resolved(controls) = &mut changed.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    let model::TrustedTimeProvenance::Verified(echo) = &mut controls.trusted_time_source else {
        panic!("the fixture has verified time");
    };
    echo.statement_digest = FOREIGN_DIGEST.parse().unwrap();
    let wire = report_bytes(changed).unwrap();
    check(
        &wire,
        expectations,
        Err(AcceptanceDefect::SealedControls),
        "sha256:065c187db2a2c01a92d438c205b0b5131f56542bbc0b8195fda94cc21215610c",
    );
    agreed.sealed.as_mut().unwrap().trusted_time_digest = FOREIGN_DIGEST.parse().unwrap();
    check(
        &wire,
        &agreed,
        Err(AcceptanceDefect::SealedControls),
        "sha256:597b0169798a518f7f95ce5d3d621a4c8c123fd9ac48a981465c219f1e4d831e",
    );
}

#[test]
fn the_time_echo_binds_every_statement_fact() {
    let (report, expectations) = &*GOLDEN;
    let model::Controls::Resolved(controls) = &report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    let model::TrustedTimeProvenance::Verified(trusted) = &controls.trusted_time_source else {
        panic!("the fixture has verified time");
    };
    let mut cases: [_; 4] = std::array::from_fn(|_| expectations.clone());
    cases[0].sealed.as_mut().unwrap().provider = "github".to_owned();
    cases[1].sealed.as_mut().unwrap().provider_run_id = "pipeline/other".to_owned();
    cases[2].sealed.as_mut().unwrap().provider_run_attempt = ATTEMPT + 1;
    cases[3].sealed.as_mut().unwrap().repository = RepositoryIdentity::new(
        "git.example.internal".to_owned(),
        "group/subgroup".to_owned(),
        "other".to_owned(),
    )
    .unwrap();
    let wire = report_bytes(report.clone()).unwrap();
    for expected in cases {
        check(
            &wire,
            &expected,
            Err(AcceptanceDefect::SealedControls),
            GOLDEN_VECTOR,
        );
    }
    let mut statements: [_; 3] = std::array::from_fn(|_| trusted.statement.clone());
    statements[0].ref_name = CANDIDATE_REF.parse().unwrap();
    statements[1].candidate_identity_digest = FOREIGN_DIGEST.parse().unwrap();
    statements[2].evaluation_instant = "2026-07-12T10:01:00Z".to_owned().try_into().unwrap();
    for (statement, identity) in statements.into_iter().zip([
        "sha256:2ce794466928a6d41c23780c88de1add35689e4815fd39391d80825acd0faa3e",
        "sha256:e80a1115c44ed6b90b26d566813c6307f92f157135e5509297e8e78b4c1f9207",
        "sha256:da9ecc82fac4d3ac243d44209537f04c3fcf90ae5fd9f5e041d8b3a805045b28",
    ]) {
        let (mut changed, mut expected) = GOLDEN.clone();
        let model::Controls::Resolved(controls) = &mut changed.payload.controls else {
            panic!("the fixture has resolved controls");
        };
        let model::TrustedTimeProvenance::Verified(echo) = &mut controls.trusted_time_source else {
            panic!("the fixture has verified time");
        };
        echo.statement_digest = canonical_trusted_time(&statement).unwrap().1;
        expected.sealed.as_mut().unwrap().trusted_time_digest = echo.statement_digest;
        echo.statement = statement;
        check(
            &report_bytes(changed).unwrap(),
            &expected,
            Err(AcceptanceDefect::SealedControls),
            identity,
        );
    }
}

#[test]
fn the_sandbox_echo_admits_only_the_self_asserted_row() {
    let (report, expectations) = &*GOLDEN;
    let model::Controls::Resolved(controls) = &report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    let mut cases: [_; 2] = std::array::from_fn(|_| controls.clone());
    cases[0].sandbox.assurance = model::SandboxAssurance::ProviderVerified;
    cases[1].sandbox.enforcement_source = model::SandboxEnforcementSource::ExternalRequiredCheck;
    for (controls, identity) in cases.into_iter().zip([
        "sha256:8e339eaa693fab7d87693bd2950c653ba8ada7642a2cbd535330d1ce31707861",
        "sha256:e42c5a7678b253c7390ad0791fe232ca544c8422b2dff08cceb56d691243000f",
    ]) {
        let mut changed = report.clone();
        changed.payload.controls = model::Controls::Resolved(controls);
        check(
            &report_bytes(changed).unwrap(),
            expectations,
            Err(AcceptanceDefect::SealedControls),
            identity,
        );
    }
    let object = serde_json_canonicalizer::to_string(&controls.sandbox).unwrap();
    let member = format!(
        r#""verification":{}"#,
        serde_json::to_string(&controls.sandbox.verification).unwrap()
    );
    assert_eq!(object.matches(&member).count(), 1);
    let invalid = object.replacen(&member, r#""verification":"attested""#, 1);
    check(
        &corrupt(report, &object, &invalid).unwrap(),
        expectations,
        Err(AcceptanceDefect::Shape),
        "sha256:4751e3ede09edfc484f4d593d178a5ebe1241b2ae4d13c6c8f49c3a26e964bfe",
    );
}

#[test]
fn an_optional_control_matches_its_expectation_on_every_fact() {
    let (report, expectations) = &*GOLDEN;
    let model::Controls::Resolved(controls) = &report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    let mut cases: [_; 6] = std::array::from_fn(|_| controls.clone());
    cases[0].organization_floor.status = model::ControlStatus::None;
    cases[1].organization_floor.digest = Some(FOREIGN_DIGEST.parse().unwrap());
    cases[2].organization_floor.trust_source = model::ControlTrustSource::None;
    cases[3].debt_snapshot.status = model::ControlStatus::Verified;
    cases[4].debt_snapshot.digest = Some(FLOOR_DIGEST.parse().unwrap());
    cases[5].debt_snapshot.trust_source = model::ControlTrustSource::ExternalRequiredCheck;
    for (controls, identity) in cases.into_iter().zip([
        "sha256:50f8538e0c41f249e45506262abea67d9b3ea2d9c915d9f58d8545dafa7ba895",
        "sha256:a91a51c0144baa5604e841d3fd6e2856d68b0ee9ac374f03e454baf776b16ed1",
        "sha256:874f7d04cdc44b8f2b904202a2b51c70cfd50fc3c860325474819a76c69ac747",
        "sha256:d5659916d4c54d21bb9e644a534f7f4eba6e20b4be527d8773c0b43a8ce16ebc",
        "sha256:f7ac28352a42a715f8b50db8ed031486c50cabe37dc13f82348f74faaef8beb6",
        "sha256:af006fd7051cf71e3da75a2fd3f87c9b230ab17478f07ed5f535bde8a7349e5d",
    ]) {
        let mut changed = report.clone();
        changed.payload.controls = model::Controls::Resolved(controls);
        check(
            &report_bytes(changed).unwrap(),
            expectations,
            Err(AcceptanceDefect::SealedControls),
            identity,
        );
    }
}

#[test]
fn the_wire_ceiling_is_exclusive_and_checked_before_acceptance() {
    let (_, expectations) = &*GOLDEN;
    let mut stdout = vec![b'x'; usize::try_from(MACHINE_JSON_BYTES).unwrap()];
    assert_eq!(
        settle(&Supervised::Completed(exited(0)), &stdout, expectations),
        Err(Defect::Acceptance(AcceptanceDefect::Noncanonical)),
        "bytes exactly at the ceiling reach acceptance"
    );
    stdout.push(b'x');
    assert_eq!(
        settle(&Supervised::Completed(exited(0)), &stdout, expectations),
        Err(Defect::Oversize),
        "one byte past the ceiling never reaches acceptance"
    );
}
