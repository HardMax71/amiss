#![cfg(test)]

use std::io::{Cursor, Error, ErrorKind, Read};

use serde::Deserialize;

use super::decode_bounded_json;
use crate::ProviderError;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Body {
    answer: u8,
}

#[test]
fn the_declared_and_actual_lengths_share_one_ceiling() {
    let bytes = br#"{"answer":42}"#;
    assert_eq!(
        decode_bounded_json::<Body>(Cursor::new(bytes), Some(13), 13),
        Ok((Body { answer: 42 }, 13))
    );
    assert_eq!(
        decode_bounded_json::<Body>(Cursor::new(bytes), Some(14), 13),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        decode_bounded_json::<Body>(Cursor::new(bytes), None, 12),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn malformed_json_and_failed_reads_keep_distinct_error_classes() {
    assert_eq!(
        decode_bounded_json::<Body>(Cursor::new(b"not-json"), None, 32),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        decode_bounded_json::<Body>(FailedReader, None, 32),
        Err(ProviderError::Unavailable)
    );
}

struct FailedReader;

impl Read for FailedReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(Error::new(ErrorKind::ConnectionReset, "fixture failure"))
    }
}
