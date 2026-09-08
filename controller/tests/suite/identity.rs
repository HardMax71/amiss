use amiss_controller::{
    DeliveryId, ProviderIdentity, ProviderNamespace, ProviderRunAttempt, ProviderRunId,
    ProviderRunIdentity,
};
use amiss_wire::model::{ObjectFormat, Oid};

#[test]
fn provider_run_preserves_typed_ids_and_checks_the_declared_format()
-> Result<(), Box<dyn std::error::Error>> {
    for (format, length) in [(ObjectFormat::Sha1, 40), (ObjectFormat::Sha256, 64)] {
        let candidate: Oid = serde_json::from_str(&format!("\"{}\"", "a".repeat(length)))?;
        for declared in [ObjectFormat::Sha1, ObjectFormat::Sha256] {
            let run = ProviderRunIdentity::new(
                ProviderRunId::new("run/1".to_owned()).ok_or("run id")?,
                ProviderRunAttempt::new(1).ok_or("attempt")?,
                declared,
                candidate.clone(),
            );
            assert_eq!(run.is_some(), declared == format);
            if let Some(run) = run {
                assert_eq!(run.candidate_commit, candidate);
                assert_eq!(run.object_format, format);
            }
        }
    }
    Ok(())
}

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
