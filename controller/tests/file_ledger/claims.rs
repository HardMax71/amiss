use std::fs;
use std::sync::Arc;

use amiss_controller::{
    ChangeId, ControllerClock, ControllerEvaluationId, DeliveryClaim, DeliveryLease,
    DeliveryLedger, FileLedger, FileLedgerError, LeaseCompletion, LeaseFence, LeaseRenewal,
    ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, Publication, StageOutcome,
    StagedPublication,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid};
use tempfile::TempDir;

use super::support::{
    MAX_RECORDS, TestClock, check_binding, config, delivery, executed, open, publication, staged,
};

#[test]
fn a_live_claim_resumes_for_its_owner_and_is_busy_for_another() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut first_owner = open(directory.path(), &clock);
    let first = executed(first_owner.claim(&delivery, &check_binding()).unwrap()).unwrap();

    assert_eq!(
        executed(first_owner.claim(&delivery, &check_binding()).unwrap()),
        Some(first.clone())
    );

    let mut second_owner = open(directory.path(), &clock);
    assert!(matches!(
        second_owner.claim(&delivery, &check_binding()).unwrap(),
        DeliveryClaim::Busy {
            evaluation_id,
            retry_at_unix_millis
        } if evaluation_id == first.evaluation_id
            && retry_at_unix_millis == first.expires_at_unix_millis
    ));
}

#[test]
fn the_epoch_instant_is_a_valid_clock_reading() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(0);
    let mut ledger = open(directory.path(), &clock);
    assert!(matches!(
        ledger.claim(&delivery("42"), &check_binding()).unwrap(),
        DeliveryClaim::Execute(_)
    ));
}

#[test]
fn a_fresh_root_adopts_nothing_it_did_not_write() {
    let stray_directory = TempDir::new().unwrap();
    fs::create_dir(stray_directory.path().join("junk")).unwrap();
    let stray_file = TempDir::new().unwrap();
    fs::write(stray_file.path().join(".atomicwrite-file"), b"partial").unwrap();

    for directory in [&stray_directory, &stray_file] {
        let clock_source: Arc<dyn ControllerClock> = TestClock::at(1_000);
        assert!(matches!(
            FileLedger::open_with_clock(directory.path(), config(MAX_RECORDS), clock_source),
            Err(FileLedgerError::Corrupt)
        ));
    }
    assert!(stray_directory.path().join("junk").is_dir());
    assert!(stray_file.path().join(".atomicwrite-file").is_file());
}

#[test]
fn the_record_root_must_already_be_a_directory() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let missing = directory.path().join("missing");
    let clock_source: Arc<dyn ControllerClock> = clock.clone();

    assert!(matches!(
        FileLedger::open_with_clock(&missing, config(MAX_RECORDS), clock_source),
        Err(FileLedgerError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound
    ));

    let file = directory.path().join("record-file");
    fs::write(&file, b"not a directory").unwrap();
    let clock_source: Arc<dyn ControllerClock> = clock;
    assert!(matches!(
        FileLedger::open_with_clock(file, config(MAX_RECORDS), clock_source),
        Err(FileLedgerError::Corrupt)
    ));
}

#[test]
fn expiry_reclaims_the_same_evaluation_with_a_higher_fence() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut first_owner = open(directory.path(), &clock);
    let mut second_owner = open(directory.path(), &clock);
    let first = executed(first_owner.claim(&delivery, &check_binding()).unwrap()).unwrap();

    clock.set(first.expires_at_unix_millis);
    let reclaimed = executed(second_owner.claim(&delivery, &check_binding()).unwrap()).unwrap();

    assert_eq!(reclaimed.evaluation_id, first.evaluation_id);
    assert_eq!(reclaimed.fence.get(), first.fence.get() + 1);
    assert_eq!(reclaimed.expires_at_unix_millis, 1_200);
    assert_eq!(
        first_owner.renew(&delivery, &first).unwrap(),
        LeaseRenewal::Lost
    );
    assert_eq!(
        first_owner
            .stage(&delivery, &first, &publication(&delivery, &first))
            .unwrap(),
        StageOutcome::Lost
    );
}

#[test]
fn renewal_advances_the_deadline_and_rejects_stale_or_rebound_claims() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let rebound = delivery("43");
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let first = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();

    clock.set(1_050);
    let renewed = match ledger.renew(&delivery, &first).unwrap() {
        LeaseRenewal::Renewed(lease) => Some(lease),
        LeaseRenewal::Lost => None,
    }
    .unwrap();

    assert_eq!(renewed.evaluation_id, first.evaluation_id);
    assert_eq!(renewed.fence, first.fence);
    assert_eq!(renewed.expires_at_unix_millis, 1_150);
    assert_eq!(ledger.renew(&delivery, &first).unwrap(), LeaseRenewal::Lost);
    assert_eq!(
        ledger.claim(&rebound, &check_binding()).unwrap(),
        DeliveryClaim::BindingConflict
    );
}

#[test]
fn clock_rollback_does_not_shorten_a_persisted_lease() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut owner = open(directory.path(), &clock);
    let first = executed(owner.claim(&delivery, &check_binding()).unwrap()).unwrap();

    clock.set(1_050);
    let renewed = match owner.renew(&delivery, &first).unwrap() {
        LeaseRenewal::Renewed(lease) => Some(lease),
        LeaseRenewal::Lost => None,
    }
    .unwrap();
    assert_eq!(renewed.expires_at_unix_millis, 1_150);

    clock.set(900);
    assert_eq!(
        owner.renew(&delivery, &renewed).unwrap(),
        LeaseRenewal::Renewed(renewed.clone())
    );
    let mut other_owner = open(directory.path(), &clock);
    assert!(matches!(
        other_owner.claim(&delivery, &check_binding()).unwrap(),
        DeliveryClaim::Busy {
            evaluation_id,
            retry_at_unix_millis: 1_150
        } if evaluation_id == renewed.evaluation_id
    ));
}

#[test]
fn the_check_binding_is_frozen_for_every_delivery_transition() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let check = check_binding();
    let mut changed = check.clone();
    changed.plan_digest = hb("amiss/test-check-plan", b"changed");
    let mut ledger = open(directory.path(), &clock);
    let lease = executed(ledger.claim(&delivery, &check).unwrap()).unwrap();

    assert_eq!(
        ledger.claim(&delivery, &changed).unwrap(),
        DeliveryClaim::BindingConflict
    );

    let mut changed_lease = lease.clone();
    changed_lease.check = changed.clone();
    assert_eq!(
        ledger.renew(&delivery, &changed_lease).unwrap(),
        LeaseRenewal::Lost
    );

    let mut changed_publication = publication(&delivery, &lease);
    changed_publication.check = changed.clone();
    assert_eq!(
        ledger
            .stage(&delivery, &lease, &changed_publication)
            .unwrap(),
        StageOutcome::Lost
    );

    let publication = publication(&delivery, &lease);
    let frozen = staged(ledger.stage(&delivery, &lease, &publication).unwrap()).unwrap();
    let mut changed_staged = frozen.clone();
    changed_staged.publication.check = changed;
    assert_eq!(
        ledger.complete(&delivery, &changed_staged).unwrap(),
        LeaseCompletion::Lost
    );
    assert_eq!(
        ledger.complete(&delivery, &frozen).unwrap(),
        LeaseCompletion::Completed
    );
}

type Deviation = fn(&mut DeliveryLease);
type Deviate = fn(&mut Publication);
type Restage = fn(&mut StagedPublication);

/// A renewal is refused unless the lease matches the row in every field, and
/// a staged publication is refused unless it names the run the row holds.
#[test]
fn a_lease_and_a_publication_are_matched_field_by_field() {
    let leases: [(&str, Deviation); 2] = [
        ("another fence", |lease| {
            lease.fence = LeaseFence::new(lease.fence.get().saturating_add(1)).unwrap();
        }),
        ("another deadline", |lease| {
            lease.expires_at_unix_millis = lease.expires_at_unix_millis.saturating_add(1);
        }),
    ];
    for (reason, deviate) in leases {
        let directory = TempDir::new().unwrap();
        let clock = TestClock::at(1_000);
        let delivery = delivery("42");
        let mut ledger = open(directory.path(), &clock);
        let mut lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
        deviate(&mut lease);
        assert_eq!(
            ledger.renew(&delivery, &lease).unwrap(),
            LeaseRenewal::Lost,
            "{reason}"
        );
    }

    let publications: [(&str, Deviate); 4] = [
        ("another evaluation", |publication| {
            publication.evaluation_id =
                ControllerEvaluationId::new("other-evaluation".to_owned()).unwrap();
        }),
        ("another provider run", |publication| {
            publication.provider_run = ProviderRunIdentity::new(
                ProviderRunId::new("other-run".to_owned()).unwrap(),
                ProviderRunAttempt::new(1).unwrap(),
                ObjectFormat::Sha1,
                publication.provider_run.candidate_commit.clone(),
            )
            .unwrap();
        }),
        ("another change", |publication| {
            publication.run.change.change = ChangeId::new("99".to_owned()).unwrap();
        }),
        ("another candidate commit", |publication| {
            publication.run.commits.candidate =
                Oid::new(ObjectFormat::Sha1, "c".repeat(40)).unwrap();
        }),
    ];
    for (reason, deviate) in publications {
        let directory = TempDir::new().unwrap();
        let clock = TestClock::at(1_000);
        let delivery = delivery("42");
        let mut ledger = open(directory.path(), &clock);
        let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
        let mut publication = publication(&delivery, &lease);
        deviate(&mut publication);
        assert_eq!(
            ledger.stage(&delivery, &lease, &publication).unwrap(),
            StageOutcome::Lost,
            "{reason}"
        );
    }
}

/// Another owner holding the same evaluation, fence, and deadline still does
/// not own the row.
#[test]
fn a_lease_belongs_to_the_owner_that_took_it() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut owner = open(directory.path(), &clock);
    let lease = executed(owner.claim(&delivery, &check_binding()).unwrap()).unwrap();
    drop(owner);

    let mut stranger = open(directory.path(), &clock);
    assert_eq!(
        stranger.renew(&delivery, &lease).unwrap(),
        LeaseRenewal::Lost,
        "the lease names another owner"
    );
}

/// Completion is refused unless the staged publication is the one the row
/// froze, in its evaluation, its fence, and its bytes.
#[test]
fn completion_answers_for_the_staged_publication_alone() {
    let rows: [(&str, bool, Restage); 5] = [
        ("another evaluation", false, |staged| {
            staged.evaluation_id =
                ControllerEvaluationId::new("other-evaluation".to_owned()).unwrap();
        }),
        ("another fence", false, |staged| {
            staged.fence = LeaseFence::new(staged.fence.get().saturating_add(1)).unwrap();
        }),
        ("another report", false, |staged| {
            staged.publication.report = Some(vec![9, 9, 9, 9, 9]);
        }),
        ("another report after completion", true, |staged| {
            staged.publication.report = Some(vec![9, 9, 9, 9, 9]);
        }),
        ("another evaluation after completion", true, |staged| {
            staged.evaluation_id =
                ControllerEvaluationId::new("other-evaluation".to_owned()).unwrap();
        }),
    ];
    for (reason, complete_first, deviate) in rows {
        let directory = TempDir::new().unwrap();
        let clock = TestClock::at(1_000);
        let delivery = delivery("42");
        let mut ledger = open(directory.path(), &clock);
        let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
        let publication = publication(&delivery, &lease);
        let mut frozen = staged(ledger.stage(&delivery, &lease, &publication).unwrap()).unwrap();
        if complete_first {
            ledger.complete(&delivery, &frozen).unwrap();
        }
        deviate(&mut frozen);
        assert_eq!(
            ledger.complete(&delivery, &frozen).unwrap(),
            LeaseCompletion::Lost,
            "{reason}"
        );
    }
}
