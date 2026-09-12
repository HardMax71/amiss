use js_int::Int;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

#[serde_with::apply(
    Option<_> => #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the protection API exposes independent control flags"
)]
pub struct BranchProtectionRecord {
    pub rule_name: String,
    pub priority: Option<Int>,
    pub enable_push: bool,
    pub enable_push_whitelist: bool,
    pub push_whitelist_usernames: Vec<String>,
    pub push_whitelist_teams: Vec<String>,
    pub push_whitelist_deploy_keys: bool,
    pub enable_force_push: Option<bool>,
    pub enable_force_push_allowlist: Option<bool>,
    pub force_push_allowlist_usernames: Option<Vec<String>>,
    pub force_push_allowlist_teams: Option<Vec<String>>,
    pub force_push_allowlist_deploy_keys: Option<bool>,
    pub enable_bypass_allowlist: Option<bool>,
    pub bypass_allowlist_usernames: Option<Vec<String>>,
    pub bypass_allowlist_teams: Option<Vec<String>>,
    #[serde(with = "As::<TryFromInto<Int>>")]
    pub required_approvals: i64,
    pub enable_approvals_whitelist: bool,
    #[serde(rename = "approvals_whitelist_username")]
    pub approvals_whitelist_usernames: Vec<String>,
    pub approvals_whitelist_teams: Vec<String>,
    pub block_on_rejected_reviews: bool,
    pub block_on_codeowner_reviews: Option<bool>,
    pub block_on_outdated_branch: bool,
    pub dismiss_stale_approvals: bool,
    pub ignore_stale_approvals: bool,
    pub unprotected_file_patterns: String,
    pub block_admin_merge_override: Option<bool>,
    pub apply_to_admins: Option<bool>,
}
