#![forbid(unsafe_code)]

pub use amiss_controller::AcquiredCommit;

pub mod states;

mod adapter;
mod fetch_plan;
mod identity;
mod live;
mod model;
mod oidc;
mod snapshot;

pub use adapter::{GitLabApi, GitLabMergeTrainAdapter, policy_job_accepted};
pub use fetch_plan::{GitLabFetchPlan, GitLabPlanError, gitlab_fetch_plan};
pub use live::{GitLabClient, GitLabClientError, GitLabObjectResolver, GitLabTimeouts};
pub use model::{
    GitLabAccess, GitLabBranch, GitLabJob, GitLabMergeChecks, GitLabMergeRequest,
    GitLabObjectRequest, GitLabObjects, GitLabPipeline, GitLabProject, GitLabProtection,
    GitLabRefresh, GitLabRefreshQuery, GitLabTrainCar, GitLabTrainSettings,
};
pub use oidc::{
    GitLabConfigError, GitLabOidc, MAX_KEYS, OidcPublicKey, PolicyBinding, RunnerTrust,
    public_keys_from_jwks,
};
