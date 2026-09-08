use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, ChangeSnapshot, CheckBinding, CheckConclusion,
    ControllerEvaluationId, DeliveryId, DeliveryIdentity, IntegrationId, ProviderError,
    ProviderIdentity, ProviderInstance, ProviderNamespace, Publication, ResolvedCommit,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};

use super::super::model::{
    BranchProtectionRecord, BranchRecord, CommitRecord, CommitStatusRecord, CreateCommitStatus,
    CreateReview, PayloadCommitRecord, PullRefRecord, PullRequestRecord, RefreshData,
    RepositoryRecord, ReviewRecord, UserRecord,
};
use super::super::rest::{GiteaRest, OperationDeadline};
use super::super::{Client, Config};
use crate::commit::{CommitBodyRecord, CommitMetaRecord};
use crate::{
    DedicatedReviewer, GiteaObjectRequest, GiteaObjectResolver, GiteaObjects, GiteaPullRequest,
};

pub(super) const GITEA_PROTECTION: &str = r#"{
  "rule_name":"main",
  "enable_push":false,
  "enable_push_whitelist":false,
  "push_whitelist_usernames":[],
  "push_whitelist_teams":[],
  "push_whitelist_deploy_keys":false,
  "protected_file_patterns":"",
  "unprotected_file_patterns":"",
  "enable_force_push":false,
  "enable_force_push_allowlist":false,
  "force_push_allowlist_usernames":[],
  "force_push_allowlist_teams":[],
  "force_push_allowlist_deploy_keys":false,
  "enable_bypass_allowlist":false,
  "bypass_allowlist_usernames":[],
  "bypass_allowlist_teams":[],
  "required_approvals":1,
  "enable_approvals_whitelist":true,
  "approvals_whitelist_username":["amiss-controller"],
  "approvals_whitelist_teams":[],
  "block_on_rejected_reviews":true,
  "block_on_outdated_branch":true,
  "dismiss_stale_approvals":true,
  "ignore_stale_approvals":false,
  "block_admin_merge_override":true
}"#;

pub(super) const FORGEJO_PROTECTION: &str = r#"{
  "rule_name":"main",
  "enable_push":false,
  "enable_push_whitelist":false,
  "push_whitelist_usernames":[],
  "push_whitelist_teams":[],
  "push_whitelist_deploy_keys":false,
  "protected_file_patterns":"",
  "unprotected_file_patterns":"",
  "required_approvals":1,
  "enable_approvals_whitelist":true,
  "approvals_whitelist_username":["amiss-controller"],
  "approvals_whitelist_teams":[],
  "block_on_rejected_reviews":true,
  "block_on_outdated_branch":true,
  "dismiss_stale_approvals":true,
  "ignore_stale_approvals":false,
  "apply_to_admins":true
}"#;

pub(super) static USER: LazyLock<UserRecord> = LazyLock::new(|| {
    amiss_wire::read_json(
        include_bytes!("../../../tests/fixtures/gitea-user.json"),
        u64::MAX,
    )
    .unwrap()
});

static COMMIT: LazyLock<CommitRecord> = LazyLock::new(|| {
    amiss_wire::read_json(
        include_bytes!("../../../tests/fixtures/gitea-commit-full.json"),
        u64::MAX,
    )
    .unwrap()
});

#[derive(Clone)]
pub(super) struct FakeRest {
    pub(super) state: Arc<Mutex<FakeState>>,
}

pub(super) struct FakeState {
    pub(super) data: RefreshData,
    pub(super) created: Vec<CreateReview>,
    pub(super) statuses: Vec<CommitStatusRecord>,
    pub(super) created_statuses: Vec<CreateCommitStatus>,
    pub(super) relation_requests: Vec<(RepositoryIdentity, BranchRef)>,
}

impl FakeRest {
    fn new(data: RefreshData) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState {
                data,
                created: Vec::new(),
                statuses: Vec::new(),
                created_statuses: Vec::new(),
                relation_requests: Vec::new(),
            })),
        }
    }
}

impl GiteaRest for FakeRest {
    fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        OperationDeadline::after(Duration::from_secs(5))
    }

    fn current_user(&self, _deadline: OperationDeadline) -> Result<UserRecord, ProviderError> {
        Ok(self.state.lock().unwrap().data.reviewer.clone())
    }

    fn relation_head(
        &self,
        repository: &RepositoryIdentity,
        target: &BranchRef,
        _deadline: OperationDeadline,
    ) -> Result<(RepositoryRecord, CommitRecord), ProviderError> {
        let mut state = self.state.lock().unwrap();
        state
            .relation_requests
            .push((repository.clone(), target.clone()));
        Ok((
            state.data.repository.clone(),
            state.data.current_head.clone(),
        ))
    }

    fn refresh_data(
        &self,
        _config: &Config,
        _pull_request: GiteaPullRequest<'_>,
        _deadline: OperationDeadline,
    ) -> Result<RefreshData, ProviderError> {
        Ok(self.state.lock().unwrap().data.clone())
    }

    fn create_review(
        &self,
        _pull_request: GiteaPullRequest<'_>,
        review: &CreateReview,
        _deadline: OperationDeadline,
    ) -> Result<ReviewRecord, ProviderError> {
        let mut state = self.state.lock().unwrap();
        state.created.push(review.clone());
        let created = ReviewRecord {
            id: u64::try_from(state.data.reviews.len())
                .unwrap()
                .saturating_add(100),
            user: Some(state.data.reviewer.clone()),
            state: review.event.clone(),
            body: review.body.clone(),
            commit_id: review.commit_id.clone(),
            stale: false,
            dismissed: false,
        };
        state.data.reviews.push(created.clone());
        Ok(created)
    }

    fn commit_statuses(
        &self,
        _repository: &RepositoryIdentity,
        _commit: &Oid,
        _deadline: OperationDeadline,
    ) -> Result<Vec<CommitStatusRecord>, ProviderError> {
        Ok(self.state.lock().unwrap().statuses.clone())
    }

    fn create_commit_status(
        &self,
        _repository: &RepositoryIdentity,
        _commit: &Oid,
        status: &CreateCommitStatus,
        _deadline: OperationDeadline,
    ) -> Result<CommitStatusRecord, ProviderError> {
        let mut state = self.state.lock().unwrap();
        state.created_statuses.push(status.clone());
        let created = CommitStatusRecord {
            id: u64::try_from(state.statuses.len())
                .unwrap()
                .saturating_add(200),
            creator: Some(state.data.reviewer.clone()),
            status: status.state.clone(),
            target_url: status.target_url.clone(),
            description: status.description.clone(),
            context: status.context.clone(),
        };
        state.statuses.insert(0, created.clone());
        Ok(created)
    }
}

pub(super) struct Fixture {
    pub(super) change: ChangeLocator,
    delivery: AuthenticatedDelivery,
    pub(super) rest: FakeRest,
    pub(super) client: Client<FakeRest>,
}

impl Fixture {
    pub(super) fn new(namespace: &str) -> Self {
        Self::mutated(namespace, |_| {})
    }

    pub(super) fn mutated(namespace: &str, mutate: impl FnOnce(&mut RefreshData)) -> Self {
        Self::built(namespace, mutate, |_| {})
    }

    pub(super) fn resolving(namespace: &str, mutate: impl FnOnce(&mut GiteaObjects)) -> Self {
        Self::built(namespace, |_| {}, mutate)
    }

    fn built(
        namespace: &str,
        mutate: impl FnOnce(&mut RefreshData),
        mutate_objects: impl FnOnce(&mut GiteaObjects),
    ) -> Self {
        let provider = provider(namespace);
        let repository = RepositoryIdentity::new(
            "forge.example".to_owned(),
            "acme".to_owned(),
            "widget".to_owned(),
        )
        .unwrap();
        let change = ChangeLocator {
            provider: provider.clone(),
            repository,
            change: ChangeId::new("repository/101/pull/4201/number/42".to_owned()).unwrap(),
        };
        let integration = IntegrationId::new("77".to_owned()).unwrap();
        let provider_run = crate::identity::provider_run(
            &integration,
            &change,
            &oid('b'),
            &branch("topic"),
            &branch("main"),
        )
        .unwrap();
        let delivery = AuthenticatedDelivery {
            identity: DeliveryIdentity {
                provider: provider.clone(),
                integration,
                delivery: DeliveryId::new("signed-body".to_owned()).unwrap(),
            },
            change: change.clone(),
            provider_run,
        };
        let mut data = refresh_data(protection_for(namespace), repository_for(namespace));
        mutate(&mut data);
        let mut objects = resolved_objects();
        mutate_objects(&mut objects);
        let rest = FakeRest::new(data);
        let client = Client {
            config: Config {
                provider,
                reviewer: reviewer(),
                review_name: "amiss".to_owned(),
            },
            rest: rest.clone(),
            objects: Arc::new(FakeObjects {
                objects,
                requests: Mutex::new(Vec::new()),
            }),
        };
        Self {
            change,
            delivery,
            rest,
            client,
        }
    }

    pub(super) fn pull_request(&self) -> GiteaPullRequest<'_> {
        GiteaPullRequest {
            change: &self.change,
            reviewer_id: 77,
            repository_id: 101,
            repository_owner: "acme",
            repository_name: "widget",
            pull_request_id: 4201,
            number: 42,
            candidate_commit: &self.delivery.provider_run.candidate_commit,
        }
    }

    pub(super) fn publication(
        &self,
        snapshot: ChangeSnapshot,
        evaluation: &str,
        conclusion: CheckConclusion,
    ) -> Publication {
        let digest = hb("amiss/controller-gitea-live-test", b"fixture");
        Publication {
            provider_run: self.delivery.provider_run.clone(),
            evaluation_id: ControllerEvaluationId::new(evaluation.to_owned()).unwrap(),
            check: CheckBinding {
                plan_digest: digest,
                required_status_name: "amiss".to_owned(),
                execution_constraint_digest: digest,
            },
            run: snapshot.run,
            gate_commit: snapshot.gate_commit,
            conclusion,
            report: Some(
                amiss_fixtures::captured_report(amiss_fixtures::SCANNER_REPORT.to_vec()).unwrap(),
            ),
            artifact: None,
        }
    }
}

pub(super) fn reviewer() -> DedicatedReviewer {
    DedicatedReviewer::new(77, "amiss-controller".to_owned()).unwrap()
}

pub(super) fn provider(namespace: &str) -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new(namespace.to_owned()).unwrap(),
        instance: ProviderInstance::new("forge.example".to_owned()).unwrap(),
    }
}

pub(super) fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}

pub(super) fn commit(commit: char, tree: char, parents: &[char]) -> CommitRecord {
    CommitRecord {
        sha: oid(commit),
        commit: CommitBodyRecord {
            tree: CommitMetaRecord {
                sha: oid(tree),
                ..COMMIT.commit.tree.clone()
            },
            ..COMMIT.commit.clone()
        },
        parents: parents
            .iter()
            .map(|parent| CommitMetaRecord {
                sha: oid(*parent),
                url: format!(
                    "https://forge.example/api/v1/repos/acme/widget/git/commits/{}",
                    oid(*parent)
                ),
                created: "0001-01-01T00:00:00Z".to_owned(),
            })
            .collect(),
        ..COMMIT.clone()
    }
}

pub(super) fn resolved(commit: char, tree: char, parents: &[char]) -> ResolvedCommit {
    ResolvedCommit {
        id: oid(commit),
        tree: oid(tree),
        parents: parents.iter().map(|parent| oid(*parent)).collect(),
    }
}

fn resolved_objects() -> GiteaObjects {
    GiteaObjects {
        candidate: resolved('b', 'd', &['a']),
        base: Some(resolved('a', 'c', &[])),
    }
}

pub(super) struct FakeObjects {
    pub(super) objects: GiteaObjects,
    pub(super) requests: Mutex<Vec<GiteaObjectRequest>>,
}

pub(super) fn objects() -> Arc<dyn GiteaObjectResolver> {
    Arc::new(FakeObjects {
        objects: resolved_objects(),
        requests: Mutex::new(Vec::new()),
    })
}

impl GiteaObjectResolver for FakeObjects {
    fn resolve(&self, request: &GiteaObjectRequest) -> Result<GiteaObjects, ProviderError> {
        self.requests.lock().unwrap().push(request.clone());
        Ok(GiteaObjects {
            candidate: self.objects.candidate.clone(),
            base: request.base_commit.as_ref().and(self.objects.base.clone()),
        })
    }
}

fn protection_for(namespace: &str) -> BranchProtectionRecord {
    let raw = if namespace == "gitea" {
        GITEA_PROTECTION
    } else {
        FORGEJO_PROTECTION
    };
    serde_json::from_str(raw).unwrap()
}

fn repository_for(namespace: &str) -> RepositoryRecord {
    let input = if namespace == "gitea" {
        include_bytes!("../../../tests/fixtures/gitea-repository.json").as_slice()
    } else {
        include_bytes!("../../../tests/fixtures/forgejo-repository.json").as_slice()
    };
    amiss_wire::read_json(input, u64::MAX).unwrap()
}

fn refresh_data(protection: BranchProtectionRecord, repository: RepositoryRecord) -> RefreshData {
    let target_repository = repository.clone();
    let head_repository = RepositoryRecord {
        id: 202,
        full_name: "contributor/widget".to_owned(),
        owner: UserRecord {
            id: 203,
            login: "contributor".to_owned(),
            username: "contributor".to_owned(),
            ..USER.clone()
        },
        ..repository.clone()
    };
    RefreshData {
        reviewer: USER.clone(),
        repository,
        pull_request: PullRequestRecord {
            id: 4201,
            number: 42,
            state: "open".to_owned(),
            mergeable: true,
            merged: false,
            merge_base: oid('a').as_str().to_owned(),
            head: PullRefRecord {
                sha: oid('b').as_str().to_owned(),
                branch: "topic".to_owned(),
                repo_id: 202,
                repo: Some(head_repository),
            },
            base: PullRefRecord {
                sha: oid('a').as_str().to_owned(),
                branch: "main".to_owned(),
                repo_id: 101,
                repo: Some(target_repository),
            },
        },
        target_branch: BranchRecord {
            name: "main".to_owned(),
            commit: Some(PayloadCommitRecord {
                id: oid('a').as_str().to_owned(),
            }),
            protected: true,
            required_approvals: 1,
            effective_branch_protection_name: "main".to_owned(),
        },
        protection,
        target: commit('a', 'c', &[]),
        candidate: commit('b', 'd', &['a']),
        current_head: commit('b', 'd', &['a']),
        reviews: vec![ReviewRecord {
            id: 1,
            user: Some(UserRecord {
                id: 88,
                login: "human".to_owned(),
                username: "human".to_owned(),
                ..USER.clone()
            }),
            state: "COMMENT".to_owned(),
            body: "looks interesting".to_owned(),
            commit_id: oid('b').as_str().to_owned(),
            stale: false,
            dismissed: false,
        }],
    }
}

fn branch(name: &str) -> BranchRef {
    BranchRef::new(format!("refs/heads/{name}")).unwrap()
}
