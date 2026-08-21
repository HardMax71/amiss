#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WebhookKeyringError {
    #[error("webhook keyring is empty")]
    Empty,
    #[error("webhook keyring has too many keys")]
    TooMany,
    #[error("webhook secret is invalid")]
    Secret,
    #[error("webhook key window is invalid")]
    Window,
    #[error("webhook anchor ID is repeated")]
    DuplicateAnchor,
    #[error("webhook secret is repeated")]
    DuplicateSecret,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WebhookError {
    #[error("webhook authentication headers are invalid")]
    Headers,
    #[error("webhook receipt time is invalid")]
    ReceiptTime,
    #[error("no webhook anchor is active for the receipt time")]
    NoActiveAnchor,
    #[error("webhook signature verification failed")]
    Authentication,
}
