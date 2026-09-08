use amiss_controller::{ForgeEvidence, ForgeProducer, ProviderError, forge_evidence};
use amiss_wire::{
    digest::hb,
    external::{self, ExternalEvidenceRow, ForgeRepository},
    model::ForgeDialect,
};

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
