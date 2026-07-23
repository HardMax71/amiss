use std::time::Duration;

use super::super::model::BranchRecord;
use super::super::rest::protection_rule_path;
use super::super::{GiteaClient, GiteaClientError, GiteaTimeouts};
use super::support::{provider, reviewer};

#[test]
fn live_client_rejects_unsafe_transport_and_identity_configuration() {
    let timeouts = GiteaTimeouts::new(Duration::from_secs(1), Duration::from_secs(3)).unwrap();
    let create = |provider, api: &str| {
        GiteaClient::new(
            provider,
            reviewer(),
            "a-secure-dedicated-token".to_owned(),
            api,
            "amiss".to_owned(),
            timeouts,
        )
    };
    assert!(create(provider("compatible-fork"), "https://forge.example/api/v1").is_ok());
    assert_eq!(
        create(provider("gitea"), "http://forge.example/api/v1").err(),
        Some(GiteaClientError::Configuration)
    );
    assert_eq!(
        create(provider("gitea"), "https://elsewhere.example/api/v1").err(),
        Some(GiteaClientError::Configuration)
    );
    assert_eq!(
        create(provider("forgejo"), "https://forge.example:443/api/v1").err(),
        Some(GiteaClientError::Configuration)
    );
    assert!(create(provider("forgejo"), "https://forge.example/api/v1").is_ok());
}

#[test]
fn effective_rule_name_selects_the_protection_resource() {
    let branch = |name: &str| BranchRecord {
        name: "main".to_owned(),
        commit: None,
        protected: true,
        required_approvals: 1,
        effective_branch_protection_name: name.to_owned(),
    };
    assert_eq!(
        protection_rule_path(&branch("release/*")),
        Ok("release%2F%2A".to_owned())
    );
    assert_eq!(
        protection_rule_path(&branch("")),
        Err(amiss_controller::ProviderError::InvalidResponse)
    );
}
