#![cfg(test)]

use amiss_git::Repository;
use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_wire::controls::{Profile, canonical_debt_snapshot, parse_debt_snapshot};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::EngineProvenance;
use amiss_wire::report::model::{EmptyRepositoryPath, FindingKeyScope, RepositoryIntentPath};

use super::{Adoption, Built, items};

fn candidate() -> (Built, Adoption) {
    let chain = amiss_fixtures::commit_chain(&[
        ("base", &[("README.md", "[gone](missing.md)\n")]),
        ("candidate", &[("README.md", "[gone](missing.md)\n")]),
    ])
    .unwrap();
    let repository = Repository::open(chain.root(), ObjectFormat::Sha1).unwrap();
    let shell = SetupShell {
        engine: EngineProvenance {
            version: "0.0.0-test".to_owned(),
            digest: hb("amiss/scanner-engine", b"adoption test"),
        },
        profile: Profile::Enforce,
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
        requests: amiss_scan::report::RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    let built = commit_pair(
        &repository,
        &shell.engine,
        None,
        &shell,
        &Oid::new(ObjectFormat::Sha1, chain.commits[0].id.clone()).unwrap(),
        &Oid::new(ObjectFormat::Sha1, chain.commits[1].id.clone()).unwrap(),
    )
    .unwrap();
    assert_eq!(built.envelope.payload.findings.len(), 1);
    let adoption = Adoption {
        floor_digest: hb("amiss/organization-floor", b"test").to_string(),
        owner: "team:docs-platform".to_owned(),
        reason: "legacy reference".to_owned(),
        created_at: "2026-07-01T00:00:00Z".to_owned(),
        expires_at: "2026-08-01T00:00:00Z".to_owned(),
        output: chain.root().join("unused.json"),
    };
    (built, adoption)
}

#[test]
fn adoption_retains_source_identities_for_the_final_writer_to_verify() {
    let (built, adoption) = candidate();
    let mut snapshot = parse_debt_snapshot(include_bytes!(
        "../../../../spec/examples/debt-snapshot.json"
    ))
    .unwrap();
    let (accepted, ineligible, factless) = items(&built, &adoption).unwrap();
    assert_eq!((accepted.len(), ineligible, factless), (1, 0, 0));
    snapshot.items = accepted;
    canonical_debt_snapshot(&snapshot).unwrap();

    let wrong = hb("amiss/test", b"stale identity");
    let mut stale = built.clone();
    stale.envelope.payload.findings[0].candidate_fact_digest = Some(wrong);
    snapshot.items = items(&stale, &adoption).unwrap().0;
    assert_eq!(snapshot.items[0].accepted_fact_digest, wrong);
    let error = canonical_debt_snapshot(&snapshot).unwrap_err();
    assert_eq!(error.kind, ErrorKind::DigestMismatch);
    assert_eq!(error.path, "$.items[0].accepted_fact_digest");

    let mut stale = built;
    stale.envelope.payload.findings[0].finding_key = wrong;
    snapshot.items = items(&stale, &adoption).unwrap().0;
    assert_eq!(snapshot.items[0].finding_key, wrong);
    let error = canonical_debt_snapshot(&snapshot).unwrap_err();
    assert_eq!(error.kind, ErrorKind::DigestMismatch);
    assert_eq!(error.path, "$.items[0].finding_key");
}

#[test]
fn adoption_refuses_nonreference_scopes_and_nontext_key_paths() {
    let (built, adoption) = candidate();
    let scope = &built.envelope.payload.findings[0]
        .candidate_fact
        .as_ref()
        .unwrap()
        .key_input
        .scope;
    let FindingKeyScope::Reference {
        document,
        normalized_target_intent,
        occurrence,
        source_construct,
    } = scope
    else {
        panic!("the scanner produced a reference finding");
    };
    let raw = RepoPath::from_bytes(b"raw-\xff.md".to_vec()).unwrap();
    let scopes = std::iter::once(FindingKeyScope::Document {
        document: document.clone(),
    })
    .chain(
        [
            (raw.clone(), normalized_target_intent.path.clone()),
            (document.clone(), RepositoryIntentPath::Path(raw)),
            (
                document.clone(),
                RepositoryIntentPath::Empty(EmptyRepositoryPath::Empty),
            ),
        ]
        .map(|(document, path)| FindingKeyScope::Reference {
            document,
            normalized_target_intent: amiss_wire::report::model::RepositoryTargetIntent {
                path,
                ..normalized_target_intent.clone()
            },
            occurrence: occurrence.clone(),
            source_construct: *source_construct,
        }),
    );
    for scope in scopes {
        let mut invalid = built.clone();
        invalid.envelope.payload.findings[0]
            .candidate_fact
            .as_mut()
            .unwrap()
            .key_input
            .scope = scope;
        assert!(items(&invalid, &adoption).is_err());
    }
}
