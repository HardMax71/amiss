#![expect(
    clippy::unwrap_used,
    reason = "fixed controller fixtures and protocol identities must fail loudly"
)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use amiss_controller::{
    AdapterRegistry, Controller, DeliveryHeader, DeliveryRoute, Evaluation, FileLedger,
    FileLedgerConfig, HandleOutcome, HeartbeatOutcome, OpaqueId, PlanRegistry, ProviderError,
    ReplayWindow, RunHeartbeat, RunIdentity, RunRequest, Runner, RunnerOutcome, SignedTimePolicy,
    UntrustedDelivery, register_plan,
};
use amiss_controller_gitlab::{
    GitLabApi, GitLabMergeTrainAdapter, GitLabRefresh, GitLabRefreshQuery,
};

use support::identity::{TestClock, now_seconds};
use support::oidc::{accept, claims, ingress, oidc, sign};
use support::plan::{plan, scope};
use support::refresh::valid_refresh;

const BODY: &[u8] = br#"{"merge_request_iid":42}"#;

#[derive(Clone)]
struct StaticApi(GitLabRefresh);

impl GitLabApi for StaticApi {
    fn refresh(&self, _query: &GitLabRefreshQuery) -> Result<GitLabRefresh, ProviderError> {
        Ok(self.0.clone())
    }
}

struct TestRunner {
    transform: fn(RunIdentity) -> RunIdentity,
}

impl Runner for TestRunner {
    fn run(&mut self, request: &RunRequest, heartbeat: &mut dyn RunHeartbeat) -> RunnerOutcome {
        assert!(matches!(
            heartbeat.renew(),
            HeartbeatOutcome::Renewed { .. }
        ));
        RunnerOutcome::Complete {
            identity: Box::new((self.transform)(request.run.clone())),
            evaluation: Evaluation::Pass,
            report: br#"{"schema":"test-report"}"#.to_vec(),
        }
    }
}

#[test]
fn signed_policy_request_runs_once_and_replay_cannot_pass_again() {
    let now = now_seconds();
    let mut lane = lane(now, |identity| identity);
    assert_eq!(
        handle(&mut lane.controller, &lane.source, now),
        HandleOutcome::Published(amiss_controller::CheckConclusion::Pass)
    );
    assert!(matches!(
        handle(&mut lane.controller, &lane.source, now),
        HandleOutcome::Duplicate { .. }
    ));
}

#[test]
fn controller_classifies_wrong_identity_and_wrong_tree_without_passing() {
    let now = now_seconds();
    let wrong_identity = run_once(now, |mut identity| {
        identity.change.change = OpaqueId::new("project/101/merge-request/99".to_owned()).unwrap();
        identity
    });
    assert_eq!(
        wrong_identity,
        HandleOutcome::Published(amiss_controller::CheckConclusion::Unavailable(
            amiss_controller::RunFailure::WrongIdentity,
        ))
    );

    let wrong_tree = run_once(now, |mut identity| {
        identity.trees.candidate = support::identity::oid('d');
        identity
    });
    assert_eq!(
        wrong_tree,
        HandleOutcome::Published(amiss_controller::CheckConclusion::Unavailable(
            amiss_controller::RunFailure::WrongTree,
        ))
    );
}

struct Lane {
    controller: Controller<FileLedger, TestRunner>,
    source: Arc<amiss_controller_gitlab::GitLabOidc>,
    _root: TestRoot,
}

fn lane(now: u64, transform: fn(RunIdentity) -> RunIdentity) -> Lane {
    let source = oidc();
    let accepted = accept(&source, &claims(now), BODY, now).unwrap();
    let delivery = accepted.delivery().clone();
    let adapter = Arc::new(GitLabMergeTrainAdapter::new(
        Arc::clone(&source),
        StaticApi(valid_refresh(&delivery)),
    ));
    let mut registry = AdapterRegistry::new();
    registry.register(adapter).unwrap();
    let mut plans = PlanRegistry::new();
    register_plan(&mut plans, scope(&delivery), plan()).unwrap();
    let replay = ReplayWindow::new(Duration::from_mins(10), Duration::from_mins(1)).unwrap();
    let root = TestRoot::new();
    let clock: Arc<dyn amiss_controller::ControllerClock> =
        Arc::new(TestClock(i64::try_from(now).unwrap() * 1_000));
    let ledger = FileLedger::open_with_clock(
        &root.0,
        FileLedgerConfig::new(Duration::from_secs(30), 100, replay).unwrap(),
        Arc::clone(&clock),
    )
    .unwrap();
    let controller = Controller::new_with_clock(
        registry,
        plans,
        ledger,
        TestRunner { transform },
        ingress(),
        clock,
    );
    Lane {
        controller,
        source,
        _root: root,
    }
}

struct TestRoot(std::path::PathBuf);

impl TestRoot {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);

        let path = std::env::temp_dir().join(format!(
            "amiss-gitlab-controller-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn run_once(now: u64, transform: fn(RunIdentity) -> RunIdentity) -> HandleOutcome {
    let mut lane = lane(now, transform);
    handle(&mut lane.controller, &lane.source, now)
}

fn handle(
    controller: &mut Controller<FileLedger, TestRunner>,
    source: &amiss_controller_gitlab::GitLabOidc,
    now: u64,
) -> HandleOutcome {
    let token = sign(&claims(now));
    let authorization = format!("Bearer {token}");
    let route = DeliveryRoute {
        provider: source.provider.clone(),
        trust_set: source.trust_set.clone(),
        signed_time: SignedTimePolicy::Required(Duration::from_mins(5)),
    };
    let headers = [DeliveryHeader {
        name: "authorization",
        value: authorization.as_bytes(),
    }];
    controller
        .handle(UntrustedDelivery {
            route: &route,
            received_at_unix_millis: i64::try_from(now).unwrap() * 1_000,
            headers: &headers,
            body: BODY,
        })
        .unwrap()
}
