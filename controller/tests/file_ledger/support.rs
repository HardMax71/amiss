use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use amiss_controller::{
    AcceptedDelivery, AuthenticatedDelivery, ChangeId, ChangeLocator, CheckConclusion,
    ControllerClock, DeliveryClaim, DeliveryHeader, DeliveryId, DeliveryIdentity, DeliveryLease,
    DeliveryRoute, FileLedger, FileLedgerConfig, GitLabWebhook, IngressLimits, IngressPolicy,
    IntegrationId, OidPair, OpaqueId, ProviderIdentity, ProviderInstance, ProviderNamespace,
    ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, Publication, ReplayWindow, RunIdentity,
    RunRefs, SignedTimePolicy, StageOutcome, StagedPublication, UntrustedDelivery, WebhookKey,
    WebhookKeyring,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use base64::Engine as _;
use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::Sha256;

pub(super) const LEASE: Duration = Duration::from_millis(100);
pub(super) const MAX_RECORDS: u64 = 64;
pub(super) const BOUNDED_ISSUED_AT: i64 = 1_744_578_123_000;
pub(super) const BOUNDED_KEEP_THROUGH: i64 = BOUNDED_ISSUED_AT + 70_000;
pub(super) const FIXTURE_KEY: &str =
    "0b320f59191352125bbed161c51c73615a815b31a16e07f1fd4e9276ed616369";

const WEBHOOK_SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";
const WEBHOOK_BODY: &[u8] = b"{\"object_kind\":\"pipeline\",\"status\":\"success\"}";

pub(super) struct TestClock(AtomicI64);

impl TestClock {
    pub(super) const fn new(now: i64) -> Self {
        Self(AtomicI64::new(now))
    }

    pub(super) fn set(&self, now: i64) {
        self.0.store(now, Ordering::SeqCst);
    }
}

impl ControllerClock for TestClock {
    fn now_unix_millis(&self) -> Option<i64> {
        Some(self.0.load(Ordering::SeqCst))
    }
}

pub(super) fn open(root: &Path, clock: &Arc<TestClock>) -> FileLedger {
    open_with_max(root, clock, MAX_RECORDS)
}

pub(super) fn open_with_max(root: &Path, clock: &Arc<TestClock>, max_records: u64) -> FileLedger {
    let clock: Arc<dyn ControllerClock> = clock.clone();
    FileLedger::open_with_clock(root, config(max_records), clock).unwrap()
}

pub(super) fn config(max_records: u64) -> FileLedgerConfig {
    FileLedgerConfig::new(LEASE, max_records, replay_window()).unwrap()
}

pub(super) fn replay_window() -> ReplayWindow {
    ReplayWindow::new(Duration::from_mins(1), Duration::from_secs(10)).unwrap()
}

fn provider() -> ProviderIdentity {
    provider_in("gitea")
}

fn gitlab_provider() -> ProviderIdentity {
    provider_in("gitlab")
}

fn provider_in(namespace: &str) -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new(namespace.to_owned()).unwrap(),
        instance: ProviderInstance::new("forge.example.test".to_owned()).unwrap(),
    }
}

pub(super) fn delivery(change_id: &str) -> AcceptedDelivery {
    delivery_with_id("delivery-9", change_id)
}

pub(super) fn delivery_with_id(delivery_id: &str, change_id: &str) -> AcceptedDelivery {
    let provider = provider();
    AcceptedDelivery::permanent(authenticated_delivery(provider, delivery_id, change_id))
}

pub(super) fn bounded_delivery(delivery_id: &str, change_id: &str) -> AcceptedDelivery {
    let provider = gitlab_provider();
    let trust_set = OpaqueId::new("webhooks-main".to_owned()).unwrap();
    let route = DeliveryRoute {
        provider: provider.clone(),
        trust_set: trust_set.clone(),
        signed_time: SignedTimePolicy::Required(Duration::from_mins(1)),
    };
    let timestamp = (BOUNDED_ISSUED_AT / 1_000).to_string();
    let signature = standard_signature(delivery_id.as_bytes(), timestamp.as_bytes());
    let headers = [
        DeliveryHeader {
            name: "webhook-id",
            value: delivery_id.as_bytes(),
        },
        DeliveryHeader {
            name: "webhook-timestamp",
            value: timestamp.as_bytes(),
        },
        DeliveryHeader {
            name: "webhook-signature",
            value: signature.as_bytes(),
        },
    ];
    let policy = IngressPolicy::new(
        IngressLimits::new(1_024, 16, 2_048).unwrap(),
        replay_window(),
        Duration::ZERO,
    )
    .unwrap();
    let check = policy
        .pre_auth(
            UntrustedDelivery {
                route: &route,
                received_at_unix_millis: BOUNDED_ISSUED_AT,
                headers: &headers,
                body: WEBHOOK_BODY,
            },
            &TestClock::new(BOUNDED_ISSUED_AT),
        )
        .unwrap();
    let key = WebhookKey::new(
        OpaqueId::new("gitlab-current".to_owned()).unwrap(),
        WEBHOOK_SECRET.to_vec(),
        0,
        None,
    )
    .unwrap();
    let proof = GitLabWebhook::new(WebhookKeyring::new(trust_set, vec![key]).unwrap())
        .verify(check)
        .unwrap();
    let verified = proof.bind(authenticated_delivery(
        provider,
        "untrusted-placeholder",
        change_id,
    ));
    let accepted = policy.post_auth(check, verified).unwrap();
    assert_eq!(
        accepted.replay_keep_through_unix_millis(),
        Some(BOUNDED_KEEP_THROUGH)
    );
    accepted
}

fn authenticated_delivery(
    provider: ProviderIdentity,
    delivery_id: &str,
    change_id: &str,
) -> AuthenticatedDelivery {
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: provider.clone(),
            integration: IntegrationId::new("installation-7".to_owned()).unwrap(),
            delivery: DeliveryId::new(delivery_id.to_owned()).unwrap(),
        },
        change: change(provider, change_id),
        provider_run: provider_run(),
    }
}

fn standard_signature(delivery_id: &[u8], timestamp: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(WEBHOOK_SECRET).unwrap();
    for part in [delivery_id, b".", timestamp, b".", WEBHOOK_BODY] {
        mac.update(part);
    }
    format!(
        "v1,{}",
        base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    )
}

fn change(provider: ProviderIdentity, change_id: &str) -> ChangeLocator {
    ChangeLocator {
        provider,
        repository: RepositoryIdentity::new(
            "forge.example.test".to_owned(),
            "owner".to_owned(),
            "amiss".to_owned(),
        )
        .unwrap(),
        change: ChangeId::new(change_id.to_owned()).unwrap(),
    }
}

fn provider_run() -> ProviderRunIdentity {
    ProviderRunIdentity::new(
        ProviderRunId::new("provider-run-11".to_owned()).unwrap(),
        ProviderRunAttempt::new(1).unwrap(),
        ObjectFormat::Sha1,
        oid('b'),
    )
    .unwrap()
}

fn oid(byte: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, byte.to_string().repeat(40)).unwrap()
}

pub(super) fn publication(delivery: &AcceptedDelivery, lease: &DeliveryLease) -> Publication {
    let delivery = delivery.delivery();
    Publication {
        provider_run: delivery.provider_run.clone(),
        evaluation_id: lease.evaluation_id.clone(),
        run: run_identity(delivery),
        conclusion: CheckConclusion::Pass,
        report: Some(vec![0, 1, 2, 0xfe, 0xff]),
    }
}

fn run_identity(delivery: &AuthenticatedDelivery) -> RunIdentity {
    RunIdentity::new(
        delivery.change.clone(),
        RunRefs {
            forge: ForgeDialect::Gitea,
            candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
            target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        },
        ObjectFormat::Sha1,
        OidPair {
            base: oid('a'),
            candidate: delivery.provider_run.candidate_commit.clone(),
        },
        OidPair {
            base: oid('c'),
            candidate: oid('d'),
        },
    )
    .unwrap()
}

pub(super) fn executed(claim: DeliveryClaim) -> Option<DeliveryLease> {
    if let DeliveryClaim::Execute(lease) = claim {
        Some(lease)
    } else {
        None
    }
}

pub(super) fn staged(outcome: StageOutcome) -> Option<StagedPublication> {
    if let StageOutcome::Staged(publication) = outcome {
        Some(publication)
    } else {
        None
    }
}

pub(super) fn ledger_file(root: &Path, marker: &str) -> Option<PathBuf> {
    fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| is_delivery_file(name, marker))
        })
}

pub(super) fn is_delivery_file(name: &str, suffix: &str) -> bool {
    name.strip_suffix(suffix).is_some_and(|key| {
        key.len() == 64
            && key
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    })
}
