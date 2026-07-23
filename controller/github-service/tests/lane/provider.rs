use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, ChangeState, ControllerClock, DeliveryHeader,
    DeliveryRoute, IngressPolicy, OidPair, ProviderError, Publication, RunIdentity, RunRefs,
    SystemClock, UntrustedDelivery,
};
use amiss_controller_github::{GitHubApi, GitHubPullRequest, GitHubPullRequestSource};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid};
use hmac::{Hmac, KeyInit as _, Mac as _};
use serde_json::json;
use sha2::Sha256;

const INSTALLATION_ID: u64 = 7;
const REPOSITORY_ID: u64 = 101;
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
            "action": "synchronize",
            "installation": { "id": INSTALLATION_ID },
            "repository": {
                "id": REPOSITORY_ID,
                "name": "widget",
                "full_name": "acme/widget",
                "owner": { "login": "acme" }
            },
            "number": PULL_REQUEST_NUMBER,
            "pull_request": {
                "id": PULL_REQUEST_ID,
                "number": PULL_REQUEST_NUMBER,
                "head": {
                    "sha": candidate.as_str(),
                    "ref": "topic"
                },
                "base": {
                    "ref": target,
                    "repo": {
                        "id": REPOSITORY_ID,
                        "name": "widget",
                        "full_name": "acme/widget",
                        "owner": { "login": "acme" }
                    }
                }
            }
        }))
        .unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(&body);
        let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes())).into_bytes();
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
        source: &GitHubPullRequestSource,
    ) -> AuthenticatedDelivery {
        let header = DeliveryHeader {
            name: "x-hub-signature-256",
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
pub(super) struct FakeGitHub {
    state: Arc<State>,
}

struct State {
    refreshes: Mutex<VecDeque<Result<ChangeSnapshot, ProviderError>>>,
    publications: Mutex<Vec<Publication>>,
}

impl FakeGitHub {
    pub(super) fn new(
        refreshes: impl IntoIterator<Item = Result<ChangeSnapshot, ProviderError>>,
    ) -> Self {
        Self {
            state: Arc::new(State {
                refreshes: Mutex::new(refreshes.into_iter().collect()),
                publications: Mutex::new(Vec::new()),
            }),
        }
    }

    pub(super) fn publications(&self) -> Vec<Publication> {
        self.state.publications.lock().unwrap().clone()
    }

    pub(super) fn flow_trace(&self) -> String {
        let conclusions: Vec<_> = self
            .state
            .publications
            .lock()
            .unwrap()
            .iter()
            .map(|publication| publication.conclusion)
            .collect();
        let refreshes_left = self.state.refreshes.lock().unwrap().len();
        format!("published {conclusions:?}, scripted refreshes left {refreshes_left}")
    }
}

impl GitHubApi for FakeGitHub {
    fn refresh(
        &self,
        _pull_request: GitHubPullRequest<'_>,
    ) -> Result<ChangeSnapshot, ProviderError> {
        self.state
            .refreshes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(ProviderError::Unavailable))
    }

    fn publish(
        &self,
        _pull_request: GitHubPullRequest<'_>,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        self.state
            .publications
            .lock()
            .unwrap()
            .push(publication.clone());
        Ok(())
    }
}

pub(super) fn snapshot(
    delivery: &AuthenticatedDelivery,
    state: ChangeState,
    base: OidPair,
    trees: OidPair,
) -> ChangeSnapshot {
    ChangeSnapshot {
        state,
        run: RunIdentity::new(
            delivery.change.clone(),
            RunRefs {
                forge: ForgeDialect::Github,
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
        gate_commit: oid('e'),
    }
}

fn branch(name: &str) -> BranchRef {
    BranchRef::new(format!("refs/heads/{name}")).unwrap()
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}
