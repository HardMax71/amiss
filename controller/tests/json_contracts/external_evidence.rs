use amiss_controller::{ForgeEvidence, ForgeProducer, ProviderError, forge_evidence};
use amiss_wire::{
    digest::hb,
    external::{self, ExternalEvidenceRow, ForgeRepository},
    model::ForgeDialect,
};

#[test]
fn external_report_fixture_retains_complete_wire_and_plan_identities() {
    for (destinations, envelope_digest, payload_digest, plan_digest) in [
        (
            [].as_slice(),
            "sha256:ba4a2908c1a21ac3a52be00e52e296a19aff074ead2df2d545d13f7f332eef4b",
            "sha256:988f08098ae896ef5cffba8648db755c44bc12a4e2aac575d1d13931824de719",
            "sha256:b3576b5cd3c3cf29053d3d6ce73c16e589dbd1d9bd077c95c52a36257368beee",
        ),
        (
            &["https://example.com/manual?x=\"quoted\"&path=é"],
            "sha256:ae630a27600babad77cfaadc130fc51a12e65f8c658df755ada67e38ff686e73",
            "sha256:d6081ca0c8cd98a0792c9c6b1d044e4b201ab18e688a83b97317b3a889fbacc3",
            "sha256:3853230872e56d64dccbf57e43697b114640d296b3b975b3a183e2f8229c50f4",
        ),
        (
            &[
                "https://github.com/acme/widgets/blob/main/a.md",
                "http://plain.example/insecure",
            ],
            "sha256:32e10b887442926005140d232877ce88e02b16e80f1c34e73c7747ecea24e027",
            "sha256:86b08294dd09d8363fcacfac82143b96d245776c268aeb991367b7169a4a345a",
            "sha256:5f81af172ffd9c43955174bc19a67f6b35e1f14715c681c8893ff98ea3f94ab4",
        ),
    ] {
        let report = amiss_fixtures::external_report(destinations).unwrap();
        let bytes = serde_json_canonicalizer::to_vec(&report).unwrap();
        let plan = amiss_fixtures::external_plan(destinations).unwrap();
        assert_eq!(
            hb(amiss_wire::report::ENVELOPE_SCHEMA, &bytes).to_string(),
            envelope_digest
        );
        assert_eq!(report.payload_digest.to_string(), payload_digest);
        assert_eq!(plan.payload_digest.to_string(), plan_digest);
        assert_eq!(plan.payload.report.payload_digest, report.payload_digest);
        assert_eq!(
            amiss_wire::report::validate_envelope(&bytes).unwrap().0,
            report
        );
        assert_eq!(amiss_fixtures::canonical_json(&bytes).unwrap(), bytes);
    }
}

#[test]
fn typed_provider_evidence_preserves_partial_facts_and_derived_laws() {
    let first = "https://github.com/acme/one/blob/main/a.md";
    let plan =
        amiss_fixtures::external_plan(&[first, "https://github.com/acme/two/blob/main/b.md"])
            .unwrap();
    for (name, version, checked_at, accepted) in [
        ("probe \"quoted\" \\ 😀", "0.0.0", "t0", true),
        ("", "0.0.0", "t0", false),
        ("probe", "", "t0", false),
        ("probe", "0.0.0", "", false),
    ] {
        let mut inspected = 0;
        let result = forge_evidence(
            &plan,
            ForgeProducer {
                dialect: ForgeDialect::Github,
                host: "github.com",
                name,
                version,
                checked_at,
            },
            || Ok(()),
            |_state, _repository| {
                inspected += 1;
                Ok(ForgeEvidence::ReadableThenUnavailable)
            },
        );
        assert_eq!(inspected, 1);
        if !accepted {
            assert_eq!(result, Err(ProviderError::InvalidResponse));
            continue;
        }
        let evidence = result.unwrap();
        assert_eq!(evidence.plan_payload_digest, plan.payload_digest);
        assert_eq!(
            (&*evidence.producer.name, &*evidence.producer.version),
            (name, version)
        );
        assert_eq!(
            evidence.rows,
            vec![ExternalEvidenceRow::ForgeApi {
                destination: first.to_owned(),
                repository: ForgeRepository::Readable,
                tail: None,
                checked_at: checked_at.to_owned(),
            }]
        );
        let bytes = external::evidence(&evidence).unwrap();
        let digest = hb(external::EVIDENCE_SCHEMA, &bytes);
        assert_eq!(
            external::parse_evidence(&bytes).unwrap(),
            (evidence.clone(), digest)
        );
        let assessment =
            external::assess(&plan, &evidence, "0.0.0", hb("test", b"engine")).unwrap();
        assert_eq!(assessment.payload.subject.evidence_digest, digest);
        assert_eq!(assessment.payload.verdicts.len(), 2);
    }
}
