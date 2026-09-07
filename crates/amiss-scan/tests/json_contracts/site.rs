use amiss_fixtures::{SiteObservation, site_observation};
use amiss_git::Repository;
use amiss_scan::{SetupShell, pipeline::commit_pair, report::RequestDigests};
use amiss_wire::{
    controls::Profile,
    digest::hb,
    model::{ObjectFormat, Oid},
    report::{EngineProvenance, FindingKind, model::FindingKeyScope},
    semantic::{
        SemanticEvidenceTemplate, SemanticProducer, SemanticProducerKind, TemplateSchema,
        observation::SITE_BUILD_VERSION,
    },
};

#[test]
fn site_defect_identities_bind_the_exact_kind_and_route() {
    let fixture = amiss_fixtures::commit_pair(
        &[("README.md", "# Readme\n")],
        &[
            ("README.md", "# Readme\n\nChanged.\n"),
            ("other.md", "# Other\n"),
        ],
    )
    .unwrap();
    let repo = Repository::open(std::path::Path::new(&fixture.repo), ObjectFormat::Sha1).unwrap();
    let base = Oid::new(ObjectFormat::Sha1, fixture.base.clone()).unwrap();
    let candidate = Oid::new(ObjectFormat::Sha1, fixture.candidate.clone()).unwrap();
    let engine = EngineProvenance {
        version: "test".to_owned(),
        digest: hb("test", b"engine"),
    };
    let observations = vec![
        site_observation("/collision/", SiteObservation::Page("README.md", &[])).unwrap(),
        site_observation("/collision/", SiteObservation::Page("other.md", &[])).unwrap(),
        site_observation(
            "/broken/",
            SiteObservation::Redirect("README.md", "/absent/"),
        )
        .unwrap(),
    ];
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
        semantic: amiss_scan::semantic::Input::Template(SemanticEvidenceTemplate {
            schema: TemplateSchema::Current,
            producer: SemanticProducer {
                kind: SemanticProducerKind::SiteBuild,
                identity: "fixture".parse().unwrap(),
                version: SITE_BUILD_VERSION.to_owned(),
                context_digest: hb("test", b"context"),
                input_digest: hb("test", b"input"),
            },
            complete: true,
            observations: observations
                .into_iter()
                .map(std::borrow::Cow::Owned)
                .collect(),
        }),
        requests: RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    let built = commit_pair(&repo, &engine, None, &setup, &base, &candidate).unwrap();
    let bytes = amiss_scan::report::wire(&built).unwrap();
    let (payload, _, _) = amiss_wire::report::validate_envelope(&bytes).unwrap();
    assert!(payload.errors.is_empty(), "{:?}", payload.errors);
    let mut ids = payload
        .findings
        .iter()
        .filter(|finding| finding.kind == FindingKind::SiteBuildDefect)
        .map(|finding| {
            let FindingKeyScope::Observation { observation_id, .. } = finding.key_input.scope
            else {
                panic!("site defects must have observation scope");
            };
            observation_id
        })
        .collect::<Vec<_>>();
    ids.sort();
    let mut expected = [
        hb(
            "amiss/scanner-site-defect",
            br#"{"kind":"broken-redirect","route":"/broken/"}"#,
        ),
        hb(
            "amiss/scanner-site-defect",
            br#"{"kind":"duplicate-route","route":"/collision/"}"#,
        ),
    ];
    expected.sort();
    assert_eq!(ids, expected);
}
