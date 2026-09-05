#![cfg(test)]

use amiss_wire::model::ForgeDialect;

use super::{ForgeEvidence, ForgeProducer, forge_evidence};
use crate::ProviderError;

#[test]
fn an_invalid_plan_spends_no_provider_budget() {
    let mut prepared = false;
    let result = forge_evidence(
        b"null",
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
        |_state, _target| Ok(ForgeEvidence::Denied),
    );

    assert_eq!(result, Err(ProviderError::InvalidResponse));
    assert!(!prepared);
}
