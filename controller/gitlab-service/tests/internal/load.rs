#![cfg(test)]

use super::{POLICY_JOB_HEADER_BYTES, PROVIDER_RESPONSE_BYTES};

#[test]
fn the_ceilings_are_the_documented_values() {
    assert_eq!(PROVIDER_RESPONSE_BYTES, 4_194_304);
    assert_eq!(POLICY_JOB_HEADER_BYTES, 32_768);
}
