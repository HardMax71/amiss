#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PullRequestState {
    Open,
    Closed,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewState {
    Approved,
    Pending,
    Comment,
    RequestChanges,
    RequestReview,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum CommitStatus {
    Pending,
    Success,
    Error,
    Failure,
    Warning,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum WebhookAction {
    Opened,
    Reopened,
    Synchronized,
    Edited,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}
