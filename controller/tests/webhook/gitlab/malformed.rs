use amiss_controller::{GitLabWebhook, WebhookError, WebhookKey, WebhookKeyringError};
use secrecy::SecretString;

use super::{BODY, ID, SECRET, SIGNATURE, TIMESTAMP, headers};
use crate::support::{NOW, anchor, header, ring, signed_check};

#[test]
fn rejects_malformed_standard_webhooks_tokens() {
    for token in [
        "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
        "whsec_MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY",
        "whsec_************************************",
        "whsec_c2hvcnQ=",
    ] {
        assert_eq!(
            WebhookKey::from_standard_token(
                anchor("gitlab-current"),
                SecretString::from(token),
                0,
                None,
            )
            .unwrap_err(),
            WebhookKeyringError::Secret
        );
    }
}

#[test]
fn legacy_plaintext_token_is_not_an_authentication_path() {
    let verifier = GitLabWebhook::new(ring("gitlab-current", SECRET));
    let headers = [header("x-gitlab-token", SECRET)];
    assert_eq!(
        verifier.verify(signed_check(&headers, BODY, NOW)),
        Err(WebhookError::Headers)
    );
}

#[test]
fn requires_exactly_one_of_each_signed_header() {
    let verifier = GitLabWebhook::new(ring("gitlab-current", SECRET));
    let duplicate = [
        header("webhook-id", ID),
        header("Webhook-Id", ID),
        header("webhook-timestamp", TIMESTAMP),
        header("webhook-signature", SIGNATURE),
    ];
    assert_eq!(
        verifier.verify(signed_check(&duplicate, BODY, NOW)),
        Err(WebhookError::Headers)
    );

    let missing = [
        header("webhook-id", ID),
        header("webhook-timestamp", TIMESTAMP),
    ];
    assert_eq!(
        verifier.verify(signed_check(&missing, BODY, NOW)),
        Err(WebhookError::Headers)
    );
}

#[test]
fn rejects_ambiguous_ids_and_noncanonical_timestamps() {
    let verifier = GitLabWebhook::new(ring("gitlab-current", SECRET));
    let dotted = [
        header("webhook-id", b"left.right"),
        header("webhook-timestamp", TIMESTAMP),
        header("webhook-signature", SIGNATURE),
    ];
    let leading_zero = [
        header("webhook-id", ID),
        header("webhook-timestamp", b"01744578123"),
        header("webhook-signature", SIGNATURE),
    ];
    let overflow = [
        header("webhook-id", ID),
        header("webhook-timestamp", b"9223372036854775807"),
        header("webhook-signature", SIGNATURE),
    ];

    assert_eq!(
        verifier.verify(signed_check(&dotted, BODY, NOW)),
        Err(WebhookError::Headers)
    );
    assert_eq!(
        verifier.verify(signed_check(&leading_zero, BODY, NOW)),
        Err(WebhookError::Headers)
    );
    assert_eq!(
        verifier.verify(signed_check(&overflow, BODY, NOW)),
        Err(WebhookError::Headers)
    );
}

#[test]
fn bounds_and_strictly_decodes_signature_entries() {
    let verifier = GitLabWebhook::new(ring("gitlab-current", SECRET));
    for malformed in [
        b"v2,eoSaLtOFqb9PT8wdg5hLQ8m9BxoPEp7HLufb1Anqlzg=".as_slice(),
        b"v1,!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!".as_slice(),
        b"v1,eoSaLtOFqb9PT8wdg5hLQ8m9BxoPEp7HLufb1Anqlzg".as_slice(),
        b" v1,eoSaLtOFqb9PT8wdg5hLQ8m9BxoPEp7HLufb1Anqlzg=".as_slice(),
    ] {
        assert_eq!(
            {
                let headers = headers(malformed);
                verifier.verify(signed_check(&headers, BODY, NOW))
            },
            Err(WebhookError::Headers)
        );
    }

    let duplicate = format!(
        "{} {}",
        std::str::from_utf8(SIGNATURE).unwrap(),
        std::str::from_utf8(SIGNATURE).unwrap()
    );
    let duplicate_headers = headers(duplicate.as_bytes());
    assert_eq!(
        verifier.verify(signed_check(&duplicate_headers, BODY, NOW)),
        Err(WebhookError::Headers)
    );

    let too_many = std::iter::repeat_n(std::str::from_utf8(SIGNATURE).unwrap(), 9)
        .collect::<Vec<_>>()
        .join(" ");
    let too_many_headers = headers(too_many.as_bytes());
    assert_eq!(
        verifier.verify(signed_check(&too_many_headers, BODY, NOW)),
        Err(WebhookError::Headers)
    );
}
