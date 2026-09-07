#![cfg(test)]

use amiss_wire::model::ForgeDialect;

use super::{ForgeEvidence, ForgeProducer, forge_evidence};
use crate::ProviderError;

#[test]
fn an_invalid_plan_spends_no_provider_budget() {
    let plan = amiss_fixtures::external_plan(&["https://github.com/acme/widgets"])
        .expect("a complete plan");
    let mut wrong_digest = plan.clone();
    wrong_digest.payload_digest = amiss_wire::digest::hb("wrong", b"plan");
    let mut changed = plan.clone();
    changed.payload.engine.engine_version.clear();
    let mut invalid_contract = changed.clone();
    invalid_contract.payload_digest = amiss_wire::digest::hb(
        amiss_wire::external::PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&invalid_contract.payload).unwrap(),
    );
    let mut invalid_count = plan.clone();
    invalid_count.payload.retained_count = u64::MAX;

    for (plan, accepted) in [
        (plan, true),
        (wrong_digest, false),
        (changed, false),
        (invalid_contract, false),
        (invalid_count, false),
    ] {
        let mut prepared = false;
        let mut inspected = false;
        let result = forge_evidence(
            &plan,
            ForgeProducer {
                dialect: ForgeDialect::Github,
                host: "github.com",
                name: "producer",
                version: "0.0.0",
                checked_at: "t0",
            },
            || {
                prepared = true;
                Ok(())
            },
            |_state, _target| {
                inspected = true;
                Ok(ForgeEvidence::Denied)
            },
        );

        assert_eq!(prepared, accepted);
        assert_eq!(inspected, accepted);
        if accepted {
            let (evidence, _) = amiss_wire::external::parse_evidence(&result.unwrap()).unwrap();
            assert_eq!(evidence.plan_payload_digest, plan.payload_digest);
            assert_eq!(evidence.rows.len(), 1);
        } else {
            assert_eq!(result, Err(ProviderError::InvalidResponse));
        }
    }
}
