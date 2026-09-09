use std::collections::BTreeSet;

use amiss_scan::{
    evaluate::{GovernedInputs, evaluate},
    projection::{Outcome, Verdict},
    report::construct,
    semantic::{SiteDefect, SiteEvaluation},
};
use amiss_wire::{
    controls::{
        ProjectionAssertion, ProjectionKind, ProjectionSink, ProjectionSource, TreePathSelection,
    },
    digest::hb,
    report::{
        FindingKind,
        model::{BrokenRedirectReason, FindingFactEvidence, ProjectionObserved},
    },
};

fn evaluated_inputs() -> Result<(SiteEvaluation, Outcome), Box<dyn std::error::Error>> {
    let source = amiss_wire::model::RepoPath::new("README.md".to_owned())
        .ok_or("invalid fixture source path")?;
    let site = SiteEvaluation {
        navigation: None,
        defects: vec![SiteDefect {
            id: hb("test", b"site defect"),
            evidence: FindingFactEvidence::BrokenRedirect {
                claim_digest: hb("test", b"redirect claim"),
                destination: "/absent/".to_owned(),
                reason: BrokenRedirectReason::MissingRoute,
                route: "/old/".to_owned(),
                source: source.clone(),
            },
            source: Some(source),
            member_count: 1,
        }]
        .into(),
    };
    let projection = Outcome {
        assertion: ProjectionAssertion {
            document: "README.md".parse()?,
            name: "api".to_owned(),
            projection: ProjectionKind::SortedRowsV1,
            sink: ProjectionSink::PreviousCode,
            source: ProjectionSource::TreePaths(TreePathSelection {
                root: "src".parse()?,
                suffix: None,
                maximum_depth: 1,
            }),
        },
        carrier_digests: Vec::new(),
        answered_spans: Vec::new(),
        representative_span: None,
        representative_display: None,
        verdict: Verdict::Drift {
            reason: ProjectionObserved::SinkAbsent,
            expected_digest: None,
            observed_digest: None,
            expected_bytes: None,
            observed_bytes: None,
            difference: None,
        },
    };
    Ok((site, projection))
}

#[test]
fn report_construction_uses_every_explicit_site_and_projection_input() {
    let setup = super::bare_setup(64);
    let discovery = super::excluded_discovery(&[]);
    let (site, projection) = evaluated_inputs().unwrap();
    let no_site = SiteEvaluation::default();
    let projections = std::slice::from_ref(&projection);
    for (site, projections, expected) in [
        (&no_site, &[][..], BTreeSet::new()),
        (
            &site,
            &[][..],
            BTreeSet::from([FindingKind::SiteBuildDefect]),
        ),
        (
            &no_site,
            projections,
            BTreeSet::from([FindingKind::ProjectionDrift]),
        ),
        (
            &site,
            projections,
            BTreeSet::from([FindingKind::SiteBuildDefect, FindingKind::ProjectionDrift]),
        ),
    ] {
        let (findings, errors) = evaluate(
            &[],
            &[],
            setup.profile,
            &setup.policy,
            GovernedInputs {
                site,
                governed: &[],
                claims: &[],
                projections,
            },
        )
        .unwrap();
        assert!(errors.is_empty());
        assert_eq!(
            findings
                .iter()
                .map(|finding| finding.key_input.finding_kind)
                .collect::<BTreeSet<_>>(),
            expected
        );
        let built = construct(
            &setup,
            &discovery,
            &discovery,
            Vec::new(),
            site,
            &[],
            projections,
        )
        .unwrap();
        assert!(built.envelope.payload.result.complete);
        assert_eq!(
            built
                .envelope
                .payload
                .findings
                .iter()
                .map(|row| row.kind)
                .collect::<BTreeSet<_>>(),
            expected
        );
        let bytes = amiss_scan::report::wire(&built).unwrap();
        let (read, _) = amiss_wire::report::validate_envelope(&bytes).unwrap();
        assert_eq!(read.payload_digest, built.payload_digest);
        assert_eq!(
            read.payload
                .findings
                .iter()
                .map(|row| row.kind)
                .collect::<BTreeSet<_>>(),
            expected
        );
    }
}

#[test]
fn report_inputs_keep_limits_and_evaluation_failure_reports() {
    let mut setup = super::bare_setup(64);
    setup.policy.complete_findings = 1;
    let discovery = super::excluded_discovery(&[]);
    let (site, projection) = evaluated_inputs().unwrap();
    let limited = construct(
        &setup,
        &discovery,
        &discovery,
        Vec::new(),
        &site,
        &[],
        std::slice::from_ref(&projection),
    )
    .unwrap();
    assert!(!limited.envelope.payload.result.complete);
    assert_eq!(limited.envelope.payload.errors.len(), 1);
    let error = &limited.envelope.payload.errors[0];
    assert_eq!(
        error.code,
        amiss_wire::report::AnalysisErrorCode::ResourceLimitExceeded
    );

    let invalid = Outcome {
        verdict: Verdict::Drift {
            reason: ProjectionObserved::ContentDiffers,
            expected_digest: None,
            observed_digest: None,
            expected_bytes: Some(u64::MAX),
            observed_bytes: None,
            difference: None,
        },
        ..projection
    };
    assert!(matches!(
        evaluate(
            &[],
            &[],
            setup.profile,
            &setup.policy,
            GovernedInputs {
                site: &SiteEvaluation::default(),
                governed: &[],
                claims: &[],
                projections: std::slice::from_ref(&invalid),
            },
        ),
        Err(amiss_scan::Error::Internal)
    ));
    let incomplete = construct(
        &setup,
        &discovery,
        &discovery,
        Vec::new(),
        &SiteEvaluation::default(),
        &[],
        std::slice::from_ref(&invalid),
    )
    .unwrap();
    assert!(!incomplete.envelope.payload.result.complete);
    assert_eq!(incomplete.envelope.payload.errors.len(), 1);
    assert_eq!(
        incomplete.envelope.payload.errors[0].code,
        amiss_wire::report::AnalysisErrorCode::InternalError
    );
}
