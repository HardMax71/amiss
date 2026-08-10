#![cfg(test)]

use super::StoredReplayKeep;

#[test]
fn a_negative_keep_through_is_corrupt() {
    assert!(
        StoredReplayKeep::KeepThrough { unix_millis: 0 }
            .validate()
            .is_ok()
    );
    assert!(
        StoredReplayKeep::KeepThrough { unix_millis: -1 }
            .validate()
            .is_err()
    );
}
