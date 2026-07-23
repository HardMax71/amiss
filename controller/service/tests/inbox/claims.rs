use amiss_controller_service::{
    ClaimOutcome, CompleteOutcome, EnqueueOutcome, InboxState, RenewOutcome, RetryOutcome,
};
use tempfile::TempDir;

use super::support::{claimed, incoming, open};

#[test]
fn expired_claim_is_recovered_after_restart() {
    let directory = TempDir::new().unwrap();
    let mut inbox = open(directory.path());
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    let first = claimed(inbox.claim(100).unwrap());
    assert_eq!(first.lease.attempt, 1);
    assert_eq!(first.lease.expires_at_unix_millis, 200);
    drop(inbox);

    let mut reopened = open(directory.path());
    assert!(matches!(
        reopened.claim(199).unwrap(),
        ClaimOutcome::Waiting {
            ready_at_unix_millis: 200
        }
    ));
    let recovered = claimed(reopened.claim(200).unwrap());
    assert_eq!(recovered.lease.attempt, 2);
    assert_eq!(recovered.delivery.body, b"body");
}

#[test]
fn retry_waits_until_the_requested_time() {
    let directory = TempDir::new().unwrap();
    let mut inbox = open(directory.path());
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    let first = claimed(inbox.claim(10).unwrap());
    assert_eq!(
        inbox.retry(&first.lease, 20, 500).unwrap(),
        RetryOutcome::Scheduled
    );
    assert_eq!(
        inbox.entries().unwrap()[0].state,
        InboxState::Pending {
            attempts: 1,
            available_at_unix_millis: 500,
        }
    );
    assert!(matches!(
        inbox.claim(499).unwrap(),
        ClaimOutcome::Waiting {
            ready_at_unix_millis: 500
        }
    ));
    assert_eq!(claimed(inbox.claim(500).unwrap()).lease.attempt, 2);
}

#[test]
fn renewal_extends_the_live_claim_and_expired_tokens_are_lost() {
    let directory = TempDir::new().unwrap();
    let mut inbox = open(directory.path());
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    let first = claimed(inbox.claim(100).unwrap());
    let renewed = match inbox.renew(&first.lease, 150).unwrap() {
        RenewOutcome::Renewed(lease) => lease,
        RenewOutcome::Lost => panic!("live lease was lost"),
    };
    assert_eq!(renewed.expires_at_unix_millis, 250);
    assert!(matches!(
        inbox.renew(&renewed, 250).unwrap(),
        RenewOutcome::Lost
    ));
    assert_eq!(inbox.retry(&renewed, 250, 300).unwrap(), RetryOutcome::Lost);
    assert_eq!(
        inbox.complete(&renewed, 250).unwrap(),
        CompleteOutcome::Lost
    );
}

#[test]
fn completion_removes_raw_bytes_and_replay_returns_to_the_delivery_ledger() {
    let directory = TempDir::new().unwrap();
    let mut inbox = open(directory.path());
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    let claimed = claimed(inbox.claim(100).unwrap());
    assert_eq!(
        inbox.complete(&claimed.lease, 150).unwrap(),
        CompleteOutcome::Completed
    );
    assert!(inbox.entries().unwrap().is_empty());
    assert!(matches!(inbox.claim(150).unwrap(), ClaimOutcome::Empty));
    assert_eq!(
        inbox.enqueue(incoming("delivery-1", b"body")).unwrap(),
        EnqueueOutcome::Stored
    );
}
