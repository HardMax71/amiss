#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum CheckStatus {
    Queued,
    InProgress,
    Requested,
    Waiting,
    Pending,
    Completed,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum CheckConclusion {
    Success,
    Failure,
    Cancelled,
    Neutral,
    Skipped,
    TimedOut,
    ActionRequired,
    Stale,
    StartupFailure,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

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
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum WebhookAction {
    Opened,
    Reopened,
    Synchronize,
    Edited,
    Completed,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}
