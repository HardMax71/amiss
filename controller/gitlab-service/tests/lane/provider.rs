use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use amiss_controller::{
    OpaqueId, ProviderError, ProviderIdentity, ProviderInstance, ProviderNamespace,
};
use amiss_controller_gitlab::{
    GitLabAccess, GitLabApi, GitLabBranch, GitLabCommit, GitLabJob, GitLabMergeChecks,
    GitLabMergeRequest, GitLabOidc, GitLabPipeline, GitLabProject, GitLabProtection, GitLabRefresh,
    GitLabRefreshQuery, GitLabTrainCar, GitLabTrainSettings, OidcPublicKey, PolicyBinding,
    RunnerTrust,
};
use amiss_wire::model::{ObjectFormat, Oid};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};

use super::repositories::Repositories;

pub(super) const HOST: &str = "gitlab.example";
pub(super) const PROJECT_ID: u64 = 101;
const PROJECT_PATH: &str = "acme/widget";
const AUDIENCE: &str = "amiss-controller";
const KID: &str = "current";

#[derive(Clone)]
pub(super) struct FakeGitLab {
    shared: Arc<State>,
}

struct State {
    refreshes: Mutex<VecDeque<Result<GitLabRefresh, ProviderError>>>,
    calls: AtomicUsize,
}

impl FakeGitLab {
    pub(super) fn new(
        refreshes: impl IntoIterator<Item = Result<GitLabRefresh, ProviderError>>,
    ) -> Self {
        Self {
            shared: Arc::new(State {
                refreshes: Mutex::new(refreshes.into_iter().collect()),
                calls: AtomicUsize::new(0),
            }),
        }
    }

    pub(super) fn calls(&self) -> usize {
        self.shared.calls.load(Ordering::SeqCst)
    }
}

impl GitLabApi for FakeGitLab {
    fn refresh(&self, _query: &GitLabRefreshQuery) -> Result<GitLabRefresh, ProviderError> {
        self.shared.calls.fetch_add(1, Ordering::SeqCst);
        self.shared
            .refreshes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(ProviderError::Unavailable))
    }
}

pub(super) fn source() -> Arc<GitLabOidc> {
    let key = OidcPublicKey::from_rsa_pem(
        KID.to_owned(),
        OpaqueId::new("gitlab-key/current".to_owned()).unwrap(),
        include_bytes!("../../../gitlab/tests/fixtures/public.pem"),
    )
    .unwrap();
    Arc::new(
        GitLabOidc::new(
            provider(),
            OpaqueId::new("gitlab-oidc".to_owned()).unwrap(),
            format!("https://{HOST}"),
            AUDIENCE.to_owned(),
            policy(),
            vec![key],
            2,
        )
        .unwrap(),
    )
}

pub(super) fn policy() -> PolicyBinding {
    PolicyBinding {
        integration: OpaqueId::new("pipeline-execution-policy/1".to_owned()).unwrap(),
        project_id: PROJECT_ID,
        project_path: PROJECT_PATH.to_owned(),
        target_branch: "main".to_owned(),
        job_name: "amiss:policy".to_owned(),
        config_url: format!("https://{HOST}/security/policy.yml"),
        config_commit: oid('f'),
        runners: RunnerTrust {
            gitlab_hosted: true,
            self_hosted_ids: BTreeSet::from([77]),
        },
    }
}

pub(super) fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::new(HOST.to_owned()).unwrap(),
    }
}

pub(super) fn claims(gate: &Oid) -> Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    json!({
        "iss": format!("https://{HOST}"),
        "sub": "project_path:acme/widget:ref_type:branch:ref:topic",
        "aud": AUDIENCE,
        "exp": now + 300,
        "nbf": now - 1,
        "iat": now,
        "jti": "gitlab-service-lane-jti",
        "job_project_id": PROJECT_ID.to_string(),
        "job_project_path": PROJECT_PATH,
        "pipeline_id": "202",
        "pipeline_source": "merge_request_event",
        "job_id": "303",
        "runner_id": "77",
        "runner_environment": "gitlab-hosted",
        "sha": gate.as_str(),
        "job_source": "pipeline_execution_policy",
        "job_config": {
            "url": format!("https://{HOST}/security/policy.yml"),
            "sha": oid('f').as_str()
        }
    })
}

pub(super) fn sign(claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KID.to_owned());
    encode(
        &header,
        claims,
        &EncodingKey::from_rsa_pem(include_bytes!("../../../gitlab/tests/fixtures/private.pem"))
            .unwrap(),
    )
    .unwrap()
}

pub(super) fn refresh(repositories: &Repositories) -> GitLabRefresh {
    let commits = repositories.commits();
    let trees = repositories.trees();
    let gate = commits.candidate.as_str().to_owned();
    let base = commits.base.as_str().to_owned();
    let source = oid('c').as_str().to_owned();
    GitLabRefresh {
        project: GitLabProject {
            id: PROJECT_ID,
            path_with_namespace: PROJECT_PATH.to_owned(),
            default_branch: "main".to_owned(),
            http_url_to_repo: format!("https://{HOST}/{PROJECT_PATH}.git"),
            repository_object_format: "sha1".to_owned(),
            checks: GitLabMergeChecks {
                pipeline_must_succeed: true,
                skipped_pipeline_allowed: false,
                merged_results_enabled: true,
            },
            train: GitLabTrainSettings {
                enabled: true,
                skip_allowed: false,
                enforcement: "enforce_for_all_users".to_owned(),
            },
            merge_method: "merge".to_owned(),
            squash_option: "never".to_owned(),
        },
        job: GitLabJob {
            id: 303,
            name: "amiss:policy".to_owned(),
            status: "running".to_owned(),
            source: "pipeline_execution_policy".to_owned(),
            pipeline_id: 202,
            commit: gate.clone(),
            runner_id: 77,
        },
        pipeline: GitLabPipeline {
            id: 202,
            project_id: PROJECT_ID,
            sha: gate.clone(),
            reference: "refs/merge-requests/42/train".to_owned(),
            source: "merge_request_event".to_owned(),
            status: "running".to_owned(),
        },
        train: Some(GitLabTrainCar {
            id: 404,
            status: "fresh".to_owned(),
            target_branch: "main".to_owned(),
            merge_request_iid: 42,
            merge_request_project_id: PROJECT_ID,
            merge_request_state: "opened".to_owned(),
            pipeline_id: 202,
            pipeline_project_id: PROJECT_ID,
            pipeline_sha: gate.clone(),
            pipeline_ref: "refs/merge-requests/42/train".to_owned(),
            pipeline_source: "merge_request_event".to_owned(),
            pipeline_status: "running".to_owned(),
        }),
        merge_request: GitLabMergeRequest {
            iid: 42,
            project_id: PROJECT_ID,
            state: "opened".to_owned(),
            draft: false,
            source_project_id: PROJECT_ID,
            target_project_id: PROJECT_ID,
            source_branch: "topic".to_owned(),
            target_branch: "main".to_owned(),
            sha: source.clone(),
            detailed_merge_status: "ci_still_running".to_owned(),
            squash_on_merge: false,
        },
        target: GitLabBranch {
            name: "main".to_owned(),
            commit: base.clone(),
        },
        gate: GitLabCommit {
            id: gate,
            tree: trees.candidate.as_str().to_owned(),
            parents: vec![base.clone(), source],
        },
        base: GitLabCommit {
            id: base,
            tree: trees.base.as_str().to_owned(),
            parents: Vec::new(),
        },
        protections: vec![GitLabProtection {
            name: "main".to_owned(),
            allow_force_push: false,
            push_access_levels: vec![GitLabAccess {
                access_level: 0,
                user_id: None,
                group_id: None,
                deploy_key_id: None,
                member_role_id: None,
            }],
        }],
    }
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}
