use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    ArtifactBundle, ArtifactComponent, ArtifactError, ArtifactStoreConfig, ControllerClock,
    ControllerEvaluationId, FileArtifactStore,
};
use amiss_controller_fixtures::clock::TestClock;
use amiss_controller_fixtures::semantic::semantic_input_artifact;
use amiss_wire::digest::{hb, sha256};

mod bounded_input;
mod external_evidence;
mod failure_tags;
mod mdbook;
mod mdbook_config;
mod relation_binding;
mod semantic_artifact;
mod semantic_binding;
mod semantic_retention;

#[test]
fn generated_semantic_artifacts_keep_their_bytes_and_replay_after_retention() {
    let fixture = semantic_input_artifact().unwrap();
    let encoded = serde_json::to_vec(fixture.artifact.artifact.as_ref().unwrap()).unwrap();
    let original = amiss_fixtures::captured_report(fixture.report).unwrap();
    let captured =
        amiss_fixtures::captured_report(serde_json::to_vec_pretty(&original.envelope).unwrap())
            .unwrap();
    assert_ne!(original.bytes, captured.bytes);
    assert_eq!(
        sha256(&encoded).to_string(),
        "sha256:6e84784cf279b723c750b0151d38bfa74caf9166995267f037dad6303aa9d595"
    );
    let root = tempfile::tempdir().unwrap();
    let clock: Arc<dyn ControllerClock> = TestClock::at(1_000);
    let config = ArtifactStoreConfig {
        base_url: "https://amiss.example/artifacts".to_owned(),
        retention: Duration::from_secs(1),
        max_records: 4,
        max_bytes: 1_048_576,
        max_record_bytes: 524_288,
    };
    let store = FileArtifactStore::open_with_clock(root.path(), config.clone(), Arc::clone(&clock))
        .unwrap();
    let evaluation = ControllerEvaluationId::new("evaluation/semantic".to_owned()).unwrap();
    let bundle = ArtifactBundle {
        report: &captured,
        semantic: Some(&fixture.artifact),
        plan: None,
        evidence: None,
        assessment: None,
        external_tally: None,
        external_incomplete: false,
    };
    let reference = store.retain(&evaluation, bundle).unwrap();
    assert_eq!(reference.semantic_digest, Some(sha256(&encoded)));
    assert_eq!(reference.report_digest, sha256(&captured.bytes));
    assert_eq!(
        store
            .read(&reference.id, ArtifactComponent::Semantic)
            .unwrap(),
        encoded
    );
    drop(store);
    let reopened = FileArtifactStore::open_with_clock(root.path(), config, clock).unwrap();
    reopened.verify(&reference).unwrap();
    assert_eq!(
        reopened
            .read(&reference.id, ArtifactComponent::Report)
            .unwrap(),
        captured.bytes
    );
    assert_eq!(reopened.find(&evaluation).unwrap(), Some(reference));
    let rejections = [
        ("missing", vec![]),
        ("mismatch", vec![hb("test", b"other")]),
    ]
    .into_iter()
    .map(|(name, digests)| {
        (
            name,
            amiss_fixtures::captured_report(amiss_fixtures::semantic_report(&digests).unwrap())
                .unwrap(),
            fixture.artifact.as_ref().clone(),
        )
    })
    .chain(
        semantic_retention::defects(&fixture.artifact)
            .unwrap()
            .into_iter()
            .map(|(name, bound)| (name, captured.clone(), bound)),
    );
    for (name, report, bound) in rejections {
        let rejected = ControllerEvaluationId::new(format!("evaluation/{name}")).unwrap();
        assert!(
            matches!(
                reopened.retain(
                    &rejected,
                    ArtifactBundle {
                        report: &report,
                        semantic: Some(&bound),
                        ..bundle
                    },
                ),
                Err(ArtifactError::Corrupt)
            ),
            "{name}"
        );
        assert_eq!(reopened.find(&rejected).unwrap(), None);
    }
}
