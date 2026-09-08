use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryPermissions {
    pub admin: bool,
    pub push: bool,
    pub pull: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InternalTracker {
    pub enable_time_tracker: bool,
    pub allow_only_contributors_to_track_time: bool,
    pub enable_issue_dependencies: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalTracker {
    pub external_tracker_url: String,
    pub external_tracker_format: String,
    pub external_tracker_style: TrackerStyle,
    pub external_tracker_regexp_pattern: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalWiki {
    pub external_wiki_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackerStyle {
    Numeric,
    Alphanumeric,
    Regexp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectsMode {
    Repo,
    Owner,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeStyle {
    Merge,
    Rebase,
    RebaseMerge,
    Squash,
    FastForwardOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateStyle {
    Merge,
    Rebase,
}
