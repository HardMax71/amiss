use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use amiss_controller::{
    AcceptedDelivery, AuthenticatedDelivery, ChangeId, ChangeLocator, CheckBinding,
    CheckConclusion, ControllerClock, DeliveryClaim, DeliveryHeader, DeliveryId, DeliveryIdentity,
    DeliveryLease, DeliveryRoute, FileLedger, FileLedgerConfig, GitLabWebhook, IngressLimits,
    IngressPolicy, IntegrationId, OidPair, OpaqueId, ProviderIdentity, ProviderInstance,
    ProviderNamespace, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, Publication,
    ReplayWindow, RunIdentity, RunRefs, SignedTimePolicy, StageOutcome, StagedPublication,
    UntrustedDelivery, WebhookKey, WebhookKeyring,
};
use amiss_wire::digest::hb;
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

pub(super) fn check_binding() -> CheckBinding {
    CheckBinding {
        plan_digest: hb("amiss/test-check-plan", b"plan"),
        required_status_name: "amiss/enforce".to_owned(),
        execution_constraint_digest: hb("amiss/test-execution-constraint", b"constraint"),
    }
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
    bounded_delivery_at(delivery_id, change_id, BOUNDED_ISSUED_AT)
}

pub(super) fn bounded_delivery_at(
    delivery_id: &str,
    change_id: &str,
    issued_at: i64,
) -> AcceptedDelivery {
    let provider = gitlab_provider();
    let trust_set = OpaqueId::new("webhooks-main".to_owned()).unwrap();
    let route = DeliveryRoute {
        provider: provider.clone(),
        trust_set: trust_set.clone(),
        signed_time: SignedTimePolicy::Required(Duration::from_mins(1)),
    };
    let timestamp = (issued_at / 1_000).to_string();
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
                received_at_unix_millis: issued_at,
                headers: &headers,
                body: WEBHOOK_BODY,
            },
            &TestClock::new(issued_at),
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
        (issued_at / 1_000)
            .checked_mul(1_000)
            .and_then(|issued_at| issued_at.checked_add(70_000))
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
    let run = run_identity(delivery);
    Publication {
        provider_run: delivery.provider_run.clone(),
        evaluation_id: lease.evaluation_id.clone(),
        check: lease.check.clone(),
        gate_commit: run.commits.candidate.clone(),
        run,
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

pub(super) fn downgrade_root_metadata(root: &Path) {
    const MAGIC: &[u8] = b"AMISS-DELIVERY-ROOT";
    const DOMAIN: &str = "amiss/controller-file-root-frame-v1";

    let path = root.join(".amiss-root.state");
    let bytes = fs::read(&path).unwrap();
    let header = MAGIC
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(8))
        .and_then(|length| length.checked_add(32))
        .unwrap();
    let payload = std::str::from_utf8(bytes.get(header..).unwrap()).unwrap();
    let legacy = payload.replace(
        "amiss/controller-file-root-v2",
        "amiss/controller-file-root-v1",
    );
    assert_ne!(legacy, payload);
    fs::write(path, test_frame(MAGIC, DOMAIN, legacy.as_bytes())).unwrap();
    fs::remove_file(root.join(".amiss-capacity.state")).unwrap();
}

pub(super) fn write_capacity(
    root: &Path,
    maximum: u64,
    records: u64,
    pending: Option<&str>,
    cleanup_pending: bool,
) {
    const MAGIC: &[u8] = b"AMISS-DELIVERY-CAPACITY";
    const DOMAIN: &str = "amiss/controller-file-capacity-frame-v1";

    let pending = serde_json::to_string(&pending).unwrap();
    let payload = format!(
        r#"{{"schema":"amiss/controller-file-capacity-v1","max_records":{maximum},"records":{records},"pending_key":{pending},"cleanup_pending":{cleanup_pending}}}"#
    );
    fs::write(
        root.join(".amiss-capacity.state"),
        test_frame(MAGIC, DOMAIN, payload.as_bytes()),
    )
    .unwrap();
}

pub(super) fn assert_frame_contract(
    path: &Path,
    magic: &[u8],
    domain: &str,
    maximum: u64,
    schema: &str,
) {
    let bytes = fs::read(path).unwrap();
    assert!(u64::try_from(bytes.len()).unwrap() <= maximum);
    let header_length = magic
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(8))
        .and_then(|length| length.checked_add(32))
        .unwrap();
    let payload = bytes.get(header_length..).unwrap();
    assert_eq!(bytes, test_frame(magic, domain, payload));
    let value: serde_json::Value = serde_json::from_slice(payload).unwrap();
    assert_eq!(
        value.get("schema").and_then(serde_json::Value::as_str),
        Some(schema)
    );
    assert!(payload.starts_with(format!(r#"{{"schema":"{schema}""#).as_bytes()));
}

fn test_frame(magic: &[u8], domain: &str, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(magic);
    frame.push(1);
    frame.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
    frame.extend_from_slice(hb(domain, payload).as_bytes());
    frame.extend_from_slice(payload);
    frame
}
