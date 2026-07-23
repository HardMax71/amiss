use amiss_controller::{DeliveryId, ProviderIdentity, ProviderNamespace, ProviderRunAttempt};

#[test]
fn provider_namespace_is_open_but_canonical() {
    assert!(ProviderNamespace::new("github".to_owned()).is_some());
    assert!(ProviderNamespace::new("forgejo-v2".to_owned()).is_some());
    assert!(ProviderNamespace::new("GitHub".to_owned()).is_none());
    assert!(ProviderNamespace::new("gitea/family".to_owned()).is_none());
}

#[test]
fn provider_identity_validates_both_parts() {
    assert!(ProviderIdentity::new("gitlab".to_owned(), "gitlab.example".to_owned()).is_some());
    assert!(ProviderIdentity::new("GitLab".to_owned(), "gitlab.example".to_owned()).is_none());
    assert!(ProviderIdentity::new("gitlab".to_owned(), "bad host".to_owned()).is_none());
}

#[test]
fn opaque_delivery_ids_reject_ambiguous_bytes() {
    assert!(DeliveryId::new("0123-abcd:1".to_owned()).is_some());
    assert!(DeliveryId::new(" delivery".to_owned()).is_none());
    assert!(DeliveryId::new("line\nbreak".to_owned()).is_none());
    assert!(DeliveryId::new("a".repeat(256)).is_some());
    assert!(DeliveryId::new("a".repeat(257)).is_none());
}

#[test]
fn provider_attempt_is_positive() {
    assert!(ProviderRunAttempt::new(0).is_none());
    assert_eq!(
        ProviderRunAttempt::new(2).map(ProviderRunAttempt::get),
        Some(2)
    );
    assert!(ProviderRunAttempt::new(9_007_199_254_740_991).is_some());
    assert!(ProviderRunAttempt::new(9_007_199_254_740_992).is_none());
}
