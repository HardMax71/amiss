use std::fs;

use amiss_wire::external::{
    ExternalEvidence, ExternalEvidenceProducer, ExternalEvidenceRow, ExternalEvidenceSchema,
    ExternalPlanEnvelope, ExternalReason, ExternalVerdict, ExternalVerdictRow, ProbeMethod,
    parse_assessment, parse_plan,
};

use crate::support;

#[test]
fn external_assessment_uses_shared_artifact_refusals_and_bounded_file_reads() {
    let directory = tempfile::tempdir().unwrap();
    let plan = directory.path().join("plan with spaces.json");
    let evidence = directory.path().join("evidence with spaces.json");
    let inputs = [
        (
            &plan,
            include_bytes!("../../../../spec/examples/scanner-external-plan.json").as_slice(),
        ),
        (
            &evidence,
            include_bytes!("../../../../spec/examples/scanner-external-evidence.json").as_slice(),
        ),
    ];
    for (path, valid) in inputs {
        fs::write(path, valid).unwrap();
    }
    for (path, valid) in inputs {
        let extended = std::str::from_utf8(valid)
            .unwrap()
            .replacen('{', "{\"future\":true,", 1);
        for malformed in [
            extended.as_bytes(),
            b"not json".as_slice(),
            br#"{"nested":{"duplicate":1,"duplicate":2}}"#,
            br#"{"nested":{"duplicate":1,"\u0064uplicate":2}}"#,
            b"{} trailing content",
            b"9007199254740992",
            b"\xff",
        ] {
            fs::write(path, malformed).unwrap();
            for format in ["human", "json"] {
                let (code, stdout, stderr) = support::amiss(&[
                    "external-assess",
                    "--plan",
                    plan.to_str().unwrap(),
                    "--evidence",
                    evidence.to_str().unwrap(),
                    "--format",
                    format,
                ]);
                assert_eq!(code, 2);
                assert!(stdout.is_empty());
                let artifact = if path == &plan { "plan" } else { "evidence" };
                assert!(
                    stderr.starts_with(&format!(
                        "amiss external-assess: external {artifact} is invalid: "
                    )),
                    "{malformed:?} with {format} output: {stderr}"
                );
            }
        }
        for (size, reason) in [
            (None, "is unreadable"),
            (
                Some(amiss_wire::external::EXTERNAL_DOCUMENT_BYTES + 1),
                "is larger than a scanner report can be",
            ),
        ] {
            fs::remove_file(path).unwrap();
            if let Some(size) = size {
                fs::File::create(path).unwrap().set_len(size).unwrap();
            }
            let (code, stdout, stderr) = support::amiss(&[
                "external-assess",
                "--plan",
                plan.to_str().unwrap(),
                "--evidence",
                evidence.to_str().unwrap(),
                "--format",
                "json",
            ]);
            assert_eq!(code, 2);
            assert!(stdout.is_empty());
            assert_eq!(
                stderr,
                format!("amiss external-assess: {} {reason}\n", path.display())
            );
            fs::write(path, valid).unwrap();
        }
    }
    let (code, stdout, stderr) = support::amiss(&[
        "external-assess",
        "--plan",
        plan.to_str().unwrap(),
        "--evidence",
        evidence.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert!(parse_assessment(&stdout).is_ok());
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn planned_pair() -> (amiss_fixtures::CommitPair, String, ExternalPlanEnvelope) {
    let pair = amiss_fixtures::commit_pair(
        &[("docs/a.md", "[kept](https://kept.example/k)\n")],
        &[(
            "docs/a.md",
            "[kept](https://kept.example/k) [new](https://new.example/n)\n",
        )],
    )
    .unwrap();
    let (code, stdout, stderr) = support::amiss(&[
        "check",
        "--repo",
        &pair.repo,
        "--object-format",
        "sha1",
        "--base",
        &pair.base,
        "--candidate",
        &pair.candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let report_path = format!("{}/report.json", pair.repo);
    fs::write(&report_path, &stdout).unwrap();
    let (code, plan_bytes, stderr) = support::amiss(&[
        "external-plan",
        "--report",
        &report_path,
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let plan = parse_plan(&plan_bytes).unwrap();
    let plan_path = format!("{}/plan.json", pair.repo);
    fs::write(&plan_path, &plan_bytes).unwrap();
    (pair, plan_path, plan)
}

fn evidence(plan: &ExternalPlanEnvelope, status: u16, method: ProbeMethod) -> ExternalEvidence {
    ExternalEvidence {
        schema: ExternalEvidenceSchema::Current,
        plan_payload_digest: plan.payload_digest,
        producer: ExternalEvidenceProducer {
            name: "curl-recipe".to_owned(),
            version: "0".to_owned(),
        },
        rows: vec![ExternalEvidenceRow::HttpProbe {
            destination: "https://new.example/n".to_owned(),
            method,
            status: Some(status),
            failure: None,
            final_destination: None,
            redirect_chain_permanent: None,
            checked_at: "2026-08-14T00:00:00Z".to_owned(),
        }],
    }
}

#[test]
fn the_chain_judges_an_introduced_destination() {
    let (pair, plan_path, plan) = planned_pair();
    let evidence_path = format!("{}/evidence.json", pair.repo);
    fs::write(
        &evidence_path,
        serde_json::to_vec(&evidence(&plan, 410, ProbeMethod::Get)).unwrap(),
    )
    .unwrap();

    let (code, stdout, stderr) = support::amiss(&[
        "external-assess",
        "--plan",
        &plan_path,
        "--evidence",
        &evidence_path,
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let assessment = parse_assessment(&stdout).unwrap();
    assert_eq!(
        assessment.payload.verdicts,
        vec![ExternalVerdictRow {
            destination: "https://new.example/n".to_owned(),
            documents: vec!["docs/a.md".to_owned()],
            reason: Some(ExternalReason::Gone),
            verdict: ExternalVerdict::Refuted,
            retarget: None,
        }],
    );
    assert_eq!(
        assessment.payload.subject.plan_payload_digest,
        plan.payload_digest,
    );
    assert_eq!(
        assessment.payload.subject.report_payload_digest,
        plan.payload.report.payload_digest,
    );
}

#[test]
fn evidence_for_a_foreign_plan_is_refused() {
    let (pair, plan_path, plan) = planned_pair();
    let mut foreign = evidence(&plan, 200, ProbeMethod::Head);
    foreign.plan_payload_digest = amiss_wire::digest::hb("foreign", b"plan");
    assert_ne!(foreign.plan_payload_digest, plan.payload_digest);
    let evidence_path = format!("{}/evidence.json", pair.repo);
    fs::write(&evidence_path, serde_json::to_vec(&foreign).unwrap()).unwrap();

    let (code, stdout, stderr) = support::amiss(&[
        "external-assess",
        "--plan",
        &plan_path,
        "--evidence",
        &evidence_path,
    ]);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("binds another plan"), "{stderr}");
}

#[test]
fn the_human_projection_windows_the_refuted() {
    let (pair, plan_path, plan) = planned_pair();
    let evidence_path = format!("{}/evidence.json", pair.repo);
    fs::write(
        &evidence_path,
        serde_json::to_vec(&evidence(&plan, 404, ProbeMethod::Get)).unwrap(),
    )
    .unwrap();

    let (code, stdout, stderr) = support::amiss(&[
        "external-assess",
        "--plan",
        &plan_path,
        "--evidence",
        &evidence_path,
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        "amiss external-assess: refuted 1 unproven 0 reachable 0\n\
         refuted \"https://new.example/n\" (gone)\n"
    );
}

#[test]
fn the_human_projection_suggests_only_a_proved_permanent_retarget() {
    let (pair, plan_path, plan) = planned_pair();
    let mut evidence = evidence(&plan, 200, ProbeMethod::Head);
    let [
        ExternalEvidenceRow::HttpProbe {
            final_destination,
            redirect_chain_permanent,
            ..
        },
    ] = evidence.rows.as_mut_slice()
    else {
        panic!("the fixture contains one HTTP probe");
    };
    *final_destination = Some("https://current.example/n".to_owned());
    *redirect_chain_permanent = Some(true);
    let evidence_path = format!("{}/evidence.json", pair.repo);
    fs::write(&evidence_path, serde_json::to_vec(&evidence).unwrap()).unwrap();

    let (code, stdout, stderr) = support::amiss(&[
        "external-assess",
        "--plan",
        &plan_path,
        "--evidence",
        &evidence_path,
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        "amiss external-assess: refuted 0 unproven 0 reachable 1\n\
         retarget suggestion \"https://new.example/n\" -> \"https://current.example/n\"\n"
    );
}

#[test]
fn the_grammar_closes_the_assessment_form() {
    for argv in [
        ["external-assess"].as_slice(),
        &["external-assess", "--plan", "p.json"],
        &["external-assess", "--evidence", "e.json"],
        &[
            "external-assess",
            "--plan",
            "p.json",
            "--evidence",
            "e.json",
            "--report",
            "r.json",
        ],
        &["external-assess", "--plan", "", "--evidence", "e.json"],
        &["check", "--plan", "p.json"],
        &[
            "external-plan",
            "--report",
            "r.json",
            "--evidence",
            "e.json",
        ],
    ] {
        let (code, _stdout, stderr) = support::amiss(argv);
        assert_eq!(code, 2, "{argv:?} must be refused");
        assert!(
            stderr.contains("INVALID_INVOCATION"),
            "{argv:?} names the code: {stderr}"
        );
    }
}
