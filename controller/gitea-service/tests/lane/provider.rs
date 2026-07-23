use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, ChangeState, ControllerClock, DeliveryHeader,
    DeliveryRoute, IngressPolicy, OidPair, ProviderError, ProviderIdentity, ProviderInstance,
    ProviderNamespace, Publication, RunIdentity, RunRefs, SystemClock, UntrustedDelivery,
};
use amiss_controller_gitea::{
    DedicatedReviewer, GiteaApi, GiteaPullRequest, GiteaPullRequestSource,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid};
use hmac::{Hmac, KeyInit as _, Mac as _};
use serde_json::json;
use sha2::Sha256;

pub(super) const REVIEWER_ID: u64 = 77;
pub(super) const REPOSITORY_ID: u64 = 101;
const PULL_REQUEST_ID: u64 = 4_201;
const PULL_REQUEST_NUMBER: u64 = 42;

pub(super) struct SignedEvent {
    pub body: Vec<u8>,
    pub signature: Vec<u8>,
    pub received_at_unix_millis: i64,
}

impl SignedEvent {
    pub(super) fn new(candidate: &Oid, secret: &[u8]) -> Self {
        Self::for_target(candidate, "main", secret)
    }

    pub(super) fn for_target(candidate: &Oid, target: &str, secret: &[u8]) -> Self {
        let body = serde_json::to_vec(&json!({
            "action": "synchronized",
            "repository": {
                "id": REPOSITORY_ID,
                "name": "widget",
                "full_name": "acme/widget",
                "owner": { "id": 12, "login": "acme" }
            },
            "number": PULL_REQUEST_NUMBER,
            "pull_request": {
                "id": PULL_REQUEST_ID,
                "number": PULL_REQUEST_NUMBER,
                "head": {
                    "sha": candidate.as_str(),
                    "ref": "topic",
                    "repo_id": 202,
                    "repo": {
                        "id": 202,
                        "name": "widget",
                        "full_name": "contributor/widget",
                        "owner": { "id": 13, "login": "contributor" }
                    }
                },
                "base": {
                    "sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "ref": target,
                    "repo_id": REPOSITORY_ID,
                    "repo": {
                        "id": REPOSITORY_ID,
                        "name": "widget",
                        "full_name": "acme/widget",
                        "owner": { "id": 12, "login": "acme" }
                    }
                }
            }
        }))
        .unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(&body);
        let signature = hex::encode(mac.finalize().into_bytes()).into_bytes();
        Self {
            body,
            signature,
            received_at_unix_millis: SystemClock.now_unix_millis().unwrap(),
        }
    }

    pub(super) fn delivery(
        &self,
        route: &DeliveryRoute,
        ingress: IngressPolicy,
        source: &GiteaPullRequestSource,
        signature_header: &str,
    ) -> AuthenticatedDelivery {
        let header = DeliveryHeader {
            name: signature_header,
            value: &self.signature,
        };
        let headers = [header];
        let checked = ingress
            .pre_auth(
                UntrustedDelivery {
                    route,
                    received_at_unix_millis: self.received_at_unix_millis,
                    headers: &headers,
                    body: &self.body,
                },
                &SystemClock,
            )
            .unwrap();
        let verified = source.authenticate(checked).unwrap();
        ingress
            .post_auth(checked, verified)
            .unwrap()
            .delivery()
            .clone()
    }
}

#[derive(Clone)]
pub(super) struct FakeGitea {
    state: Arc<State>,
}

struct State {
    expected_reviewer_id: u64,
    refreshes: Mutex<VecDeque<Result<ChangeSnapshot, ProviderError>>>,
    publish_failures: Mutex<usize>,
    publications: Mutex<Vec<Publication>>,
}

impl FakeGitea {
    pub(super) fn new(
        expected_reviewer_id: u64,
        refreshes: impl IntoIterator<Item = Result<ChangeSnapshot, ProviderError>>,
        publish_failures: usize,
    ) -> Self {
        Self {
            state: Arc::new(State {
                expected_reviewer_id,
                refreshes: Mutex::new(refreshes.into_iter().collect()),
                publish_failures: Mutex::new(publish_failures),
                publications: Mutex::new(Vec::new()),
            }),
        }
    }
}

impl GiteaApi for FakeGitea {
    fn refresh(&self, pull_request: GiteaPullRequest<'_>) -> Result<ChangeSnapshot, ProviderError> {
        if pull_request.reviewer_id != self.state.expected_reviewer_id {
            return Err(ProviderError::AuthorizationRevoked);
        }
        self.state
            .refreshes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(ProviderError::Unavailable))
    }

    fn publish(
        &self,
        pull_request: GiteaPullRequest<'_>,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        if pull_request.reviewer_id != self.state.expected_reviewer_id {
            return Err(ProviderError::AuthorizationRevoked);
        }
        let mut failures = self.state.publish_failures.lock().unwrap();
        if *failures > 0 {
            *failures = failures.saturating_sub(1);
            return Err(ProviderError::Unavailable);
        }
        drop(failures);
        self.state
            .publications
            .lock()
            .unwrap()
            .push(publication.clone());
        Ok(())
    }
}

pub(super) fn publication_count(api: &FakeGitea) -> usize {
    api.state.publications.lock().unwrap().len()
}

pub(super) fn flow_trace(api: &FakeGitea) -> String {
    let conclusions: Vec<_> = api
        .state
        .publications
        .lock()
        .unwrap()
        .iter()
        .map(|publication| publication.conclusion)
        .collect();
    let refreshes_left = api.state.refreshes.lock().unwrap().len();
    format!("published {conclusions:?}, scripted refreshes left {refreshes_left}")
}

pub(super) fn last_conclusion(api: &FakeGitea) -> Option<amiss_controller::CheckConclusion> {
    api.state
        .publications
        .lock()
        .unwrap()
        .last()
        .map(|publication| publication.conclusion)
}

pub(super) fn snapshot(
    delivery: &AuthenticatedDelivery,
    base: OidPair,
    trees: OidPair,
) -> ChangeSnapshot {
    ChangeSnapshot {
        state: ChangeState::Active,
        run: RunIdentity::new(
            delivery.change.clone(),
            RunRefs {
                forge: ForgeDialect::Gitea,
                candidate: branch("topic"),
                target: branch("main"),
                default_branch: branch("main"),
            },
            ObjectFormat::Sha1,
            OidPair {
                base: base.base,
                candidate: delivery.provider_run.candidate_commit.clone(),
            },
            trees,
        )
        .unwrap(),
        gate_commit: delivery.provider_run.candidate_commit.clone(),
    }
}

pub(super) fn provider(namespace: &str) -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new(namespace.to_owned()).unwrap(),
        instance: ProviderInstance::new("forge.example".to_owned()).unwrap(),
    }
}

pub(super) fn reviewer() -> DedicatedReviewer {
    DedicatedReviewer::new(REVIEWER_ID, "amiss-controller".to_owned()).unwrap()
}

fn branch(name: &str) -> BranchRef {
    BranchRef::new(format!("refs/heads/{name}")).unwrap()
}
