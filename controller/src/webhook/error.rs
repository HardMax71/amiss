use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebhookKeyringError {
    Empty,
    TooMany,
    Secret,
    Window,
    DuplicateAnchor,
    DuplicateSecret,
}

impl fmt::Display for WebhookKeyringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(error_message(ErrorMessage::Keyring(*self)))
    }
}

impl std::error::Error for WebhookKeyringError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebhookError {
    Headers,
    ReceiptTime,
    NoActiveAnchor,
    Authentication,
}

impl fmt::Display for WebhookError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(error_message(ErrorMessage::Verification(*self)))
    }
}

impl std::error::Error for WebhookError {}

#[derive(Clone, Copy)]
enum ErrorMessage {
    Keyring(WebhookKeyringError),
    Verification(WebhookError),
}

const fn error_message(error: ErrorMessage) -> &'static str {
    match error {
        ErrorMessage::Keyring(WebhookKeyringError::Empty) => "webhook keyring is empty",
        ErrorMessage::Keyring(WebhookKeyringError::TooMany) => "webhook keyring has too many keys",
        ErrorMessage::Keyring(WebhookKeyringError::Secret) => "webhook secret is invalid",
        ErrorMessage::Keyring(WebhookKeyringError::Window) => "webhook key window is invalid",
        ErrorMessage::Keyring(WebhookKeyringError::DuplicateAnchor) => {
            "webhook anchor ID is repeated"
        }
        ErrorMessage::Keyring(WebhookKeyringError::DuplicateSecret) => "webhook secret is repeated",
        ErrorMessage::Verification(WebhookError::Headers) => {
            "webhook authentication headers are invalid"
        }
        ErrorMessage::Verification(WebhookError::ReceiptTime) => "webhook receipt time is invalid",
        ErrorMessage::Verification(WebhookError::NoActiveAnchor) => {
            "no webhook anchor is active for the receipt time"
        }
        ErrorMessage::Verification(WebhookError::Authentication) => {
            "webhook signature verification failed"
        }
    }
}
