use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use amiss_controller::{
    OpaqueId, ProviderError, ProviderIdentity, ProviderInstance, ProviderNamespace, ResolvedCommit,
};
use amiss_controller_fixtures::{RsaKeys, rsa_keys};
use amiss_controller_gitlab::claims::Claims;
use amiss_controller_gitlab::{
    GitLabAccess, GitLabApi, GitLabBranch, GitLabJob, GitLabMergeChecks, GitLabMergeRequest,
    GitLabOidc, GitLabPipeline, GitLabProject, GitLabProtection, GitLabRefresh, GitLabRefreshQuery,
    GitLabTrainCar, GitLabTrainSettings, OidcPublicKey, PolicyBinding, RunnerTrust,
};
use amiss_wire::model::{ObjectFormat, Oid};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

use amiss_controller_fixtures::lane::Repositories;

pub(super) const HOST: &str = "gitlab.example";
pub(super) const PROJECT_ID: u64 = 101;
const PROJECT_PATH: &str = "acme/widget";
const AUDIENCE: &str = "amiss-controller";
const KID: &str = "current";

#[expect(
    clippy::expect_used,
    reason = "the generated RSA fixture must remain valid"
)]
static RSA_KEYS: LazyLock<RsaKeys> =
    LazyLock::new(|| rsa_keys().expect("the RSA fixture is valid"));

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
        OpaqueId::try_from("gitlab-key/current".to_owned()).unwrap(),
        &RSA_KEYS.public_pem,
    )
    .unwrap();
    Arc::new(
        GitLabOidc::new(
            provider(),
            OpaqueId::try_from("gitlab-oidc".to_owned()).unwrap(),
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
        integration: OpaqueId::try_from("pipeline-execution-policy/1".to_owned()).unwrap(),
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
        namespace: ProviderNamespace::try_from("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::try_from(HOST.to_owned()).unwrap(),
    }
}

pub(super) fn claims(gate: &Oid, now_millis: i64) -> Claims {
    let now = u64::try_from(now_millis.div_euclid(1_000)).unwrap();
    let mut claims: Claims = serde_json::from_slice(amiss_fixtures::GITLAB_POLICY_CLAIMS).unwrap();
    claims.iat = now;
    claims.nbf = now.checked_sub(1).unwrap();
    claims.exp = now.checked_add(300).unwrap();
    "gitlab-service-lane-jti".clone_into(&mut claims.jti);
    gate.as_str().clone_into(&mut claims.sha);
    claims
}

pub(super) fn sign(claims: &Claims) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KID.to_owned());
    encode(
        &header,
        claims,
        &EncodingKey::from_rsa_pem(&RSA_KEYS.private_pem).unwrap(),
    )
    .unwrap()
}

pub(super) fn refresh(repositories: &Repositories) -> GitLabRefresh {
    let commits = &repositories.commits;
    let trees = &repositories.trees;
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
            source: Some("pipeline_execution_policy".to_owned()),
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
        gate: ResolvedCommit {
            id: commits.candidate.clone(),
            tree: trees.candidate.clone(),
            parents: vec![commits.base.clone(), oid('c')],
        },
        base: ResolvedCommit {
            id: commits.base.clone(),
            tree: trees.base.clone(),
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
