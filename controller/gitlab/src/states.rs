#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PipelineStatus {
    Running,
    Canceled,
    Created,
    Failed,
    Manual,
    Pending,
    Preparing,
    Scheduled,
    Skipped,
    Success,
    WaitingForCallback,
    WaitingForResource,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MergeRequestState {
    Opened,
    Closed,
    Locked,
    Merged,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TrainStatus {
    Idle,
    Fresh,
    Stale,
    Merging,
    Merged,
    SkipMerged,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PipelineSource {
    MergeRequestEvent,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum JobSource {
    PipelineExecutionPolicy,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TrainEnforcement {
    EnforceForAllUsers,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MergeMethod {
    Merge,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SquashOption {
    Never,
    #[serde(untagged)]
    #[strum(default)]
    Unknown(String),
}
