use amiss_controller::OpaqueId;
use amiss_controller::{ProviderIdentity, ProviderNamespace, ProviderRunAttempt};

#[test]
fn provider_namespace_is_open_but_canonical() {
    assert!(ProviderNamespace::try_from("github".to_owned()).is_ok());
    assert!(ProviderNamespace::try_from("forgejo-v2".to_owned()).is_ok());
    assert!(ProviderNamespace::try_from("GitHub".to_owned()).is_err());
    assert!(ProviderNamespace::try_from("gitea/family".to_owned()).is_err());
}

#[test]
fn provider_identity_validates_both_parts() {
    assert!(ProviderIdentity::new("gitlab".to_owned(), "gitlab.example".to_owned()).is_some());
    assert!(ProviderIdentity::new("GitLab".to_owned(), "gitlab.example".to_owned()).is_none());
    assert!(ProviderIdentity::new("gitlab".to_owned(), "bad host".to_owned()).is_none());
}

#[test]
fn opaque_delivery_ids_reject_ambiguous_bytes() {
    assert!(OpaqueId::try_from("0123-abcd:1".to_owned()).is_ok());
    assert!(OpaqueId::try_from(" delivery".to_owned()).is_err());
    assert!(OpaqueId::try_from("line\nbreak".to_owned()).is_err());
    assert!(OpaqueId::try_from("a".repeat(256)).is_ok());
    assert!(OpaqueId::try_from("a".repeat(257)).is_err());
}

#[test]
fn provider_attempt_is_positive() {
    assert!(ProviderRunAttempt::try_from(0).is_err());
    assert_eq!(
        ProviderRunAttempt::try_from(2).map(ProviderRunAttempt::get),
        Ok(2)
    );
    assert!(ProviderRunAttempt::try_from(9_007_199_254_740_991).is_ok());
    assert!(ProviderRunAttempt::try_from(9_007_199_254_740_992).is_err());
}
