#![cfg(test)]

use amiss_wire::controls::{
    BlobLineSelection, Profile, ProjectionAssertion, ProjectionKind, ProjectionSource,
};
use amiss_wire::model::{RepoPath, RepoPathText};
use amiss_wire::report::AnalysisErrorCode;

use crate::evaluate::run::{GovernedInputs, evaluate_with_site};
use crate::projection::{DriftReason, Outcome, Verdict};

#[test]
fn a_source_serialization_failure_is_an_analysis_error() {
    let document = RepoPathText::new("README.md".to_owned()).unwrap();
    let source = |last_line| {
        ProjectionSource::BlobLines(BlobLineSelection {
            path: RepoPathText::new("src/lib.rs".to_owned()).unwrap(),
            first_line: 1,
            last_line,
        })
    };
    let mut outcome = Outcome {
        assertion: ProjectionAssertion {
            document: document.clone(),
            name: "example".to_owned(),
            projection: ProjectionKind::CodeTextV1,
            source: source(2),
        },
        carrier_digests: Vec::new(),
        answered_spans: Vec::new(),
        representative_span: None,
        representative_display: None,
        verdict: Verdict::Drift {
            reason: DriftReason::SourceAbsent,
            expected_digest: None,
            observed_digest: None,
            expected_bytes: None,
            observed_bytes: None,
            difference: None,
        },
    };
    for (last_line, finding_count, error_count) in [(2, 1, 0), (u64::MAX, 0, 1)] {
        outcome.assertion.source = source(last_line);
        let (findings, errors) = evaluate_with_site(
            &[],
            &[],
            Profile::Observe,
            &crate::policy::Effects::default(),
            GovernedInputs {
                site: &crate::semantic::SiteEvaluation::default(),
                governed: &[],
                claims: &[],
                projections: std::slice::from_ref(&outcome),
            },
        );
        assert_eq!(findings.len(), finding_count);
        assert_eq!(errors.len(), error_count);
        if let Some(error) = errors.first() {
            assert_eq!(error.code, AnalysisErrorCode::ReportConstructionFailed);
            assert_eq!(error.path, Some(RepoPath::from(&document)));
        }
    }
}
