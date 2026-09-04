use std::fs;

use amiss_fixtures::git;
use amiss_git::Repository;
use amiss_scan::pipeline::{SetupShell, commit_pair, staged_index};
use amiss_scan::report::RequestDigests;
use amiss_wire::controls::Profile;
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::model::DocumentClassification;
use amiss_wire::report::{EngineProvenance, validate_envelope};
use tempfile::TempDir;

#[test]
fn unparsed_documents_survive_the_report_contract() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]).unwrap();
    fs::create_dir(root.join("vendor")).unwrap();
    fs::write(root.join("vendor/skipped.org"), "excluded\n").unwrap();
    for path in ["tour.ipynb", "plan.org"] {
        fs::write(root.join(path), "[not parsed](missing.md)\n").unwrap();
    }
    git(root, &["add", "."]).unwrap();
    git(root, &["commit", "-qm", "unparsed documents"]).unwrap();
    let commit = Oid::new(
        ObjectFormat::Sha1,
        git(root, &["rev-parse", "HEAD"]).unwrap().trim().to_owned(),
    )
    .unwrap();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let engine = EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine", b"document contract"),
    };
    let setup = SetupShell {
        engine: engine.clone(),
        profile: Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        semantic: amiss_scan::semantic::Input::None,
        requests: RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    for report in [
        commit_pair(&repo, &engine, None, &setup, &commit, &commit),
        staged_index(&repo, &engine, None, &setup, &commit),
    ] {
        assert_eq!(report.exit_code, 0);
        let (payload, digest, _) = validate_envelope(&report.envelope).unwrap();
        assert_eq!(digest, report.payload_digest);
        assert_eq!(payload.documents.len(), 3);
        assert_eq!(payload.summary.documents.unsupported, 2);
        assert_eq!(payload.summary.documents.excluded_builtin, 1);
        assert_eq!(payload.summary.documents.scanned, 0);
        assert!(payload.observations.is_empty());
        for document in &payload.documents {
            assert_eq!(
                document.classification,
                DocumentClassification::UnparsedMarkup
            );
            for side in [document.base.as_ref(), document.candidate.as_ref()] {
                let side = side.unwrap();
                assert_eq!(side.adapter_id, None);
                assert_eq!(side.extracted_references, 0);
            }
        }
        let wire: serde_json::Value = serde_json::from_slice(&report.wire()).unwrap();
        super::support::assert_report(&wire, "unparsed documents");
    }
}
