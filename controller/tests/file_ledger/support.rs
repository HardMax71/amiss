use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, CheckConclusion, ControllerClock,
    DeliveryClaim, DeliveryId, DeliveryIdentity, DeliveryLease, FileLedger, IntegrationId, OidPair,
    ProviderIdentity, ProviderInstance, ProviderNamespace, ProviderRunAttempt, ProviderRunId,
    ProviderRunIdentity, Publication, RunIdentity, RunRefs, StageOutcome, StagedPublication,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

pub(super) const LEASE: Duration = Duration::from_millis(100);
pub(super) const FIXTURE_KEY: &str =
    "0b320f59191352125bbed161c51c73615a815b31a16e07f1fd4e9276ed616369";
pub(super) const FIXTURE_EVALUATION: &str =
    "eval:0b320f59191352125bbed161c51c73615a815b31a16e07f1fd4e9276ed616369";

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
    let clock: Arc<dyn ControllerClock> = clock.clone();
    FileLedger::open_with_clock(root, LEASE, clock).unwrap()
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("gitea".to_owned()).unwrap(),
        instance: ProviderInstance::new("forge.example.test".to_owned()).unwrap(),
    }
}

pub(super) fn delivery(change_id: &str) -> AuthenticatedDelivery {
    let provider = provider();
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: provider.clone(),
            integration: IntegrationId::new("installation-7".to_owned()).unwrap(),
            delivery: DeliveryId::new("delivery-9".to_owned()).unwrap(),
        },
        change: change(provider, change_id),
        provider_run: provider_run(),
    }
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

pub(super) fn publication(delivery: &AuthenticatedDelivery, lease: &DeliveryLease) -> Publication {
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
                .is_some_and(|name| name.contains(marker))
        })
}
