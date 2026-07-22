use amiss_controller::{GitLabWebhook, ReplayIdentity, WebhookError, WebhookKeyring};
use secrecy::SecretString;

use super::{BODY, ID, NEW_SECRET, NEW_SIGNATURE, SECRET, SIGNATURE, TIMESTAMP, headers};
use crate::support::{NOW, anchor, header, key, ring, signed_check, trust_set};

#[test]
fn accepts_a_standard_webhooks_vector() {
    let key = amiss_controller::WebhookKey::from_standard_token(
        anchor("gitlab-current"),
        SecretString::from("whsec_MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY="),
        0,
        None,
    )
    .unwrap();
    let verifier = GitLabWebhook::new(WebhookKeyring::new(trust_set(), vec![key]).unwrap());
    let headers = headers(SIGNATURE);
    let proof = verifier.verify(signed_check(&headers, BODY, NOW)).unwrap();

    assert_eq!(proof.trust_set(), &trust_set());
    assert_eq!(proof.anchor(), &anchor("gitlab-current"));
    assert_eq!(
        proof.replay(),
        &ReplayIdentity::Authenticated(
            amiss_controller::DeliveryId::new("f5e5f430-f57b-4e6e-9fac-d9128cd7232f".to_owned(),)
                .unwrap(),
        )
    );
    assert_eq!(proof.issued_at_unix_millis(), Some(NOW));
}

#[test]
fn accepts_multiple_rotation_signatures_and_reports_the_newest_key() {
    let keys = WebhookKeyring::new(
        trust_set(),
        vec![
            key("retiring", SECRET, 0, None),
            key("current", NEW_SECRET, NOW - 1, None),
        ],
    )
    .unwrap();
    let verifier = GitLabWebhook::new(keys);
    let signatures = format!(
        "{} {}",
        std::str::from_utf8(SIGNATURE).unwrap(),
        std::str::from_utf8(NEW_SIGNATURE).unwrap()
    );

    let headers = headers(signatures.as_bytes());
    assert_eq!(
        verifier
            .verify(signed_check(&headers, BODY, NOW))
            .unwrap()
            .anchor(),
        &anchor("current")
    );
}

#[test]
fn authenticates_raw_bytes_without_decoding_an_event() {
    let verifier = GitLabWebhook::new(ring("gitlab-current", SECRET));
    let signature = b"v1,YvjfiRRw3NtVOi9qkGYp0ufalxqO3NH1BJKWe/sz5Y4=";
    let headers = headers(signature);
    let proof = verifier
        .verify(signed_check(&headers, b"not json", NOW))
        .unwrap();
    assert_eq!(proof.anchor(), &anchor("gitlab-current"));
}

#[test]
fn every_signed_component_is_bound() {
    let verifier = GitLabWebhook::new(ring("gitlab-current", SECRET));
    let changed_id = [
        header("webhook-id", b"f5e5f430-f57b-4e6e-9fac-d9128cd7232e"),
        header("webhook-timestamp", TIMESTAMP),
        header("webhook-signature", SIGNATURE),
    ];
    let changed_time = [
        header("webhook-id", ID),
        header("webhook-timestamp", b"1744578124"),
        header("webhook-signature", SIGNATURE),
    ];

    assert_eq!(
        verifier.verify(signed_check(&changed_id, BODY, NOW)),
        Err(WebhookError::Authentication)
    );
    assert_eq!(
        verifier.verify(signed_check(&changed_time, BODY, NOW)),
        Err(WebhookError::Authentication)
    );
    assert_eq!(
        {
            let headers = headers(SIGNATURE);
            verifier.verify(signed_check(&headers, b"{}", NOW))
        },
        Err(WebhookError::Authentication)
    );
}

#[test]
fn rejects_a_signature_from_an_unconfigured_secret() {
    let verifier = GitLabWebhook::new(ring("gitlab-current", SECRET));
    let headers = headers(NEW_SIGNATURE);
    assert_eq!(
        verifier.verify(signed_check(&headers, BODY, NOW)),
        Err(WebhookError::Authentication)
    );
}
