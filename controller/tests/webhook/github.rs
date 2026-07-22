use amiss_controller::{GitHubWebhook, IngressError, ReplayIdentity, WebhookError};

use super::support::{NOW, anchor, header, key, replay_check, ring, trust_set};

const SECRET: &[u8] = b"It's a Secret to Everybody";
const BODY: &[u8] = b"Hello, World!";
const SIGNATURE: &[u8] = b"sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";

#[test]
fn accepts_githubs_documented_vector() -> Result<(), IngressError> {
    let verifier = GitHubWebhook::new(ring("github-current", SECRET));
    let headers = [
        header("X-Hub-Signature-256", SIGNATURE),
        header("X-GitHub-Delivery", b"unsigned-delivery-id"),
    ];
    let proof = verifier.verify(replay_check(&headers, BODY, NOW)?).unwrap();

    assert_eq!(proof.anchor(), &anchor("github-current"));
    assert_eq!(proof.replay(), &ReplayIdentity::ExactBody);
    assert_eq!(proof.issued_at_unix_millis(), None);
    Ok(())
}

#[test]
fn rejects_tampering_and_the_wrong_key() -> Result<(), IngressError> {
    let verifier = GitHubWebhook::new(ring("github-current", SECRET));
    let headers = [header("x-hub-signature-256", SIGNATURE)];
    assert_eq!(
        verifier.verify(replay_check(&headers, b"Hello, World?", NOW)?),
        Err(WebhookError::Authentication)
    );

    let wrong = GitHubWebhook::new(ring("github-wrong", b"replacement-webhook-secret-2026"));
    assert_eq!(
        wrong.verify(replay_check(&headers, BODY, NOW)?),
        Err(WebhookError::Authentication)
    );
    Ok(())
}

#[test]
fn requires_one_strict_signature_header() -> Result<(), IngressError> {
    let verifier = GitHubWebhook::new(ring("github-current", SECRET));
    assert_eq!(
        verifier.verify(replay_check(&[], BODY, NOW)?),
        Err(WebhookError::Headers)
    );
    let duplicate = [
        header("x-hub-signature-256", SIGNATURE),
        header("X-HUB-SIGNATURE-256", SIGNATURE),
    ];
    assert_eq!(
        verifier.verify(replay_check(&duplicate, BODY, NOW)?),
        Err(WebhookError::Headers)
    );

    let uppercase = b"sha256=757107EA0EB2509FC211221CCE984B8A37570B6D7586C22C46F4379C8B043E17";
    let headers = [header("x-hub-signature-256", uppercase)];
    assert_eq!(
        verifier.verify(replay_check(&headers, BODY, NOW)?),
        Err(WebhookError::Headers)
    );
    Ok(())
}

#[test]
fn key_window_is_selected_from_controller_receipt_time() -> Result<(), IngressError> {
    let keys = amiss_controller::WebhookKeyring::new(
        trust_set(),
        vec![
            key("retiring", SECRET, 0, Some(NOW)),
            key("current", b"replacement-webhook-secret-2026", NOW, None),
        ],
    )
    .unwrap();
    let verifier = GitHubWebhook::new(keys);

    let retiring_headers = [header("x-hub-signature-256", SIGNATURE)];
    assert_eq!(
        verifier.verify(replay_check(&retiring_headers, BODY, NOW)?),
        Err(WebhookError::Authentication)
    );
    let current_headers = [header(
        "x-hub-signature-256",
        b"sha256=1777da5ac74bba4f1f1a51aa3ce95a30b3a860cc42da67acde4710302ca37e8b",
    )];
    assert_eq!(
        verifier
            .verify(replay_check(&current_headers, BODY, NOW)?)
            .unwrap()
            .anchor(),
        &anchor("current")
    );
    Ok(())
}
