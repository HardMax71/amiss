use amiss_controller_service::{
    ClaimOutcome, CompleteOutcome, EnqueueOutcome, InboxError, InboxState, RenewOutcome,
    RetryOutcome,
};
use tempfile::TempDir;

use super::support::{claimed, incoming, open, owner_of};

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
fn renewal_fences_the_replaced_token() {
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
        inbox.renew(&first.lease, 160).unwrap(),
        RenewOutcome::Lost
    ));
    assert_eq!(
        inbox.retry(&first.lease, 160, 300).unwrap(),
        RetryOutcome::Lost
    );
    assert_eq!(
        inbox.complete(&first.lease, 160).unwrap(),
        CompleteOutcome::Lost
    );
    assert_eq!(
        inbox.complete(&renewed, 160).unwrap(),
        CompleteOutcome::Completed
    );
}

#[test]
fn renewal_never_shortens_on_clock_rollback_and_expired_tokens_are_lost() {
    let directory = TempDir::new().unwrap();
    let mut inbox = open(directory.path());
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    let first = claimed(inbox.claim(100).unwrap());
    let renewed = match inbox.renew(&first.lease, 50).unwrap() {
        RenewOutcome::Renewed(lease) => lease,
        RenewOutcome::Lost => panic!("live lease was lost"),
    };
    assert_eq!(renewed.expires_at_unix_millis, 200);
    assert!(matches!(
        inbox.renew(&renewed, 200).unwrap(),
        RenewOutcome::Lost
    ));
    assert_eq!(inbox.retry(&renewed, 200, 300).unwrap(), RetryOutcome::Lost);
    assert_eq!(
        inbox.complete(&renewed, 200).unwrap(),
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

/// The requested time may be now, and may not be behind it.
#[test]
fn a_retry_may_be_scheduled_for_this_instant_and_not_before_it() {
    let directory = TempDir::new().unwrap();
    let mut inbox = open(directory.path());
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    let lease = claimed(inbox.claim(10).unwrap()).lease;
    assert_eq!(
        inbox.retry(&lease, 50, 50).unwrap(),
        RetryOutcome::Scheduled,
        "at once is a lawful next attempt"
    );

    let lease = claimed(inbox.claim(50).unwrap()).lease;
    assert!(
        matches!(inbox.retry(&lease, 60, 59), Err(InboxError::Clock)),
        "a next attempt in the past is a clock the run cannot trust"
    );
}

/// Time before the epoch is not a time this inbox reads.
#[test]
fn a_negative_clock_is_refused_before_any_row_is_read() {
    let directory = TempDir::new().unwrap();
    let mut inbox = open(directory.path());
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    assert!(matches!(inbox.claim(-1), Err(InboxError::Clock)));
    let lease = claimed(inbox.claim(10).unwrap()).lease;
    assert!(matches!(
        inbox.retry(&lease, -1, 500),
        Err(InboxError::Clock)
    ));
    assert!(matches!(inbox.complete(&lease, -1), Err(InboxError::Clock)));
}

/// A claim owner is the whole of the randomness it was built from: two
/// halves, both varying, or two processes could hold one delivery.
#[test]
fn a_claim_owner_carries_both_halves_of_its_randomness() {
    let mut leading = std::collections::BTreeSet::new();
    for _process in 0..4 {
        let directory = TempDir::new().unwrap();
        let mut inbox = open(directory.path());
        inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
        claimed(inbox.claim(10).unwrap());
        let owner = owner_of(directory.path());
        assert_eq!(owner.len(), 32, "{owner}");
        assert!(
            owner.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "{owner}"
        );
        leading.insert(owner.get(..16).unwrap_or_default().to_owned());
    }
    assert!(
        leading.len() > 1,
        "the high half is randomness too: {leading:?}"
    );
}
