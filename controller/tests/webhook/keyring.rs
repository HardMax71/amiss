use amiss_controller::{
    GitHubWebhook, WebhookError, WebhookKey, WebhookKeyring, WebhookKeyringError,
};

use super::support::{NOW, anchor, header, key, replay_check, ring, trust_set};

const BODY: &[u8] = b"Hello, World!";
const OLD_SECRET: &[u8] = b"It's a Secret to Everybody";
const OLD_SIGNATURE: &[u8] =
    b"sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";

#[test]
fn validates_keyring_shape_and_windows() {
    assert_eq!(
        WebhookKeyring::new(trust_set(), Vec::new()).unwrap_err(),
        WebhookKeyringError::Empty
    );
    assert_eq!(
        WebhookKey::new(anchor("weak"), b"too-short".to_vec(), 0, None).unwrap_err(),
        WebhookKeyringError::Secret
    );
    assert_eq!(
        WebhookKey::new(anchor("backwards"), OLD_SECRET.to_vec(), NOW, Some(NOW),).unwrap_err(),
        WebhookKeyringError::Window
    );
    assert_eq!(
        WebhookKey::new(anchor("negative"), OLD_SECRET.to_vec(), -1, None).unwrap_err(),
        WebhookKeyringError::Window
    );

    let too_many = (0..9)
        .map(|position| {
            key(
                &format!("anchor-{position}"),
                format!("unique-secret-material-{position:02}").as_bytes(),
                0,
                None,
            )
        })
        .collect();
    assert_eq!(
        WebhookKeyring::new(trust_set(), too_many).unwrap_err(),
        WebhookKeyringError::TooMany
    );
}

#[test]
fn rejects_repeated_anchor_ids_and_secret_material() {
    assert_eq!(
        WebhookKeyring::new(
            trust_set(),
            vec![
                key("same", OLD_SECRET, 0, None),
                key("same", b"replacement-webhook-secret-2026", 0, None),
            ]
        )
        .unwrap_err(),
        WebhookKeyringError::DuplicateAnchor
    );
    assert_eq!(
        WebhookKeyring::new(
            trust_set(),
            vec![
                key("first", OLD_SECRET, 0, None),
                key("second", OLD_SECRET, NOW, None),
            ]
        )
        .unwrap_err(),
        WebhookKeyringError::DuplicateSecret
    );
}

#[test]
fn debug_output_redacts_secret_material() {
    let key = key("redacted", OLD_SECRET, 0, None);
    let rendered = format!("{key:?}");
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("It's a Secret"));
}

#[test]
fn replacing_the_keyring_revokes_an_omitted_anchor() {
    let old = GitHubWebhook::new(ring("old", OLD_SECRET));
    let headers = [header("x-hub-signature-256", OLD_SIGNATURE)];
    assert!(old.verify(replay_check(&headers, BODY, NOW)).is_ok());

    let replacement = GitHubWebhook::new(ring("replacement", b"replacement-webhook-secret-2026"));
    assert_eq!(
        replacement.verify(replay_check(&headers, BODY, NOW)),
        Err(WebhookError::Authentication)
    );
}

#[test]
fn rejects_an_inactive_set() {
    let future =
        WebhookKeyring::new(trust_set(), vec![key("future", OLD_SECRET, NOW + 1, None)]).unwrap();
    let headers = [header("x-hub-signature-256", OLD_SIGNATURE)];
    assert_eq!(
        GitHubWebhook::new(future).verify(replay_check(&headers, BODY, NOW)),
        Err(WebhookError::NoActiveAnchor)
    );
}

#[test]
fn bounds_and_validates_the_complete_header_block() {
    let verifier = GitHubWebhook::new(ring("active", OLD_SECRET));
    let too_many = vec![header("x-noop", b"value"); 129];
    assert_eq!(
        verifier.verify(replay_check(&too_many, BODY, NOW)),
        Err(WebhookError::Headers)
    );

    let oversized = vec![b'x'; 32 * 1_024];
    let oversized_headers = [
        header("x-noop", &oversized),
        header("x-hub-signature-256", OLD_SIGNATURE),
    ];
    assert_eq!(
        verifier.verify(replay_check(&oversized_headers, BODY, NOW)),
        Err(WebhookError::Headers)
    );
    let invalid_name = [
        header("bad header", b"value"),
        header("x-hub-signature-256", OLD_SIGNATURE),
    ];
    assert_eq!(
        verifier.verify(replay_check(&invalid_name, BODY, NOW)),
        Err(WebhookError::Headers)
    );
    let invalid_value = [
        header("x-noop", b"line\r\nbreak"),
        header("x-hub-signature-256", OLD_SIGNATURE),
    ];
    assert_eq!(
        verifier.verify(replay_check(&invalid_value, BODY, NOW)),
        Err(WebhookError::Headers)
    );
}
