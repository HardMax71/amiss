use amiss_controller::{GiteaWebhook, IngressError, ReplayIdentity, WebhookError};

use super::support::{NOW, anchor, header, replay_check, ring};

const SECRET: &[u8] = b"gitea-webhook-secret-2026";
const BODY: &[u8] = b"{\"ref\":\"refs/heads/main\"}";
const SIGNATURE: &[u8] = b"3e1e755f6f700e2aab9589e55d1c1e2be6d0bd4ef1f859040e718adef9655975";

#[test]
fn accepts_constructed_gitea_vector_without_trusting_delivery_header() -> Result<(), IngressError> {
    let verifier = GiteaWebhook::new(ring("gitea-current", SECRET));
    let headers = [
        header("x-gitea-signature", SIGNATURE),
        header("x-gitea-delivery", b"unsigned-delivery-id"),
    ];
    let proof = verifier.verify(replay_check(&headers, BODY, NOW)?).unwrap();

    assert_eq!(proof.anchor(), &anchor("gitea-current"));
    assert_eq!(proof.replay(), &ReplayIdentity::ExactBody);
    assert_eq!(proof.issued_at_unix_millis(), None);
    Ok(())
}

#[test]
fn accepts_forgejo_header_without_changing_configured_identity() -> Result<(), IngressError> {
    let verifier = GiteaWebhook::new(ring("gitea-current", SECRET));
    let headers = [header("x-forgejo-signature", SIGNATURE)];
    let proof = verifier.verify(replay_check(&headers, BODY, NOW)?).unwrap();

    assert_eq!(proof.anchor(), &anchor("gitea-current"));
    assert_eq!(proof.replay(), &ReplayIdentity::ExactBody);
    Ok(())
}

#[test]
fn rejects_duplicate_or_ambiguous_family_signatures() -> Result<(), IngressError> {
    let verifier = GiteaWebhook::new(ring("gitea-current", SECRET));
    let ambiguous = [
        header("x-gitea-signature", SIGNATURE),
        header("x-forgejo-signature", SIGNATURE),
    ];
    assert_eq!(
        verifier.verify(replay_check(&ambiguous, BODY, NOW)?),
        Err(WebhookError::Headers)
    );
    let duplicate = [
        header("x-forgejo-signature", SIGNATURE),
        header("X-Forgejo-Signature", SIGNATURE),
    ];
    assert_eq!(
        verifier.verify(replay_check(&duplicate, BODY, NOW)?),
        Err(WebhookError::Headers)
    );
    Ok(())
}

#[test]
fn rejects_body_changes_and_noncanonical_signatures() -> Result<(), IngressError> {
    let verifier = GiteaWebhook::new(ring("gitea-current", SECRET));
    let headers = [header("x-gitea-signature", SIGNATURE)];
    assert_eq!(
        verifier.verify(replay_check(
            &headers,
            b"{\"ref\":\"refs/heads/other\"}",
            NOW,
        )?),
        Err(WebhookError::Authentication)
    );
    let uppercase = [header(
        "x-gitea-signature",
        b"3E1E755F6F700E2AAB9589E55D1C1E2BE6D0BD4EF1F859040E718ADEF9655975",
    )];
    assert_eq!(
        verifier.verify(replay_check(&uppercase, BODY, NOW)?),
        Err(WebhookError::Headers)
    );
    let prefixed = [header(
        "x-gitea-signature",
        b"sha256=3e1e755f6f700e2aab9589e55d1c1e2be6d0bd4ef1f859040e718adef9655975",
    )];
    assert_eq!(
        verifier.verify(replay_check(&prefixed, BODY, NOW)?),
        Err(WebhookError::Headers)
    );
    Ok(())
}
