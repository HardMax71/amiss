#![cfg(test)]

use std::io::{Cursor, Error, ErrorKind, Read};

use serde::{Deserialize, Serialize};

use super::decode_bounded_json;
use crate::ProviderError;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Body {
    answer: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Nested(Vec<Nested>);

#[test]
fn the_declared_and_actual_lengths_share_one_ceiling() {
    let bytes = br#"{"answer":42}"#;
    assert_eq!(
        decode_bounded_json::<Body, _>(Cursor::new(bytes), Some(13), 13, |bytes| {
            serde_json::from_slice(bytes)
        }),
        Ok((Body { answer: 42 }, 13))
    );
    assert_eq!(
        decode_bounded_json::<Body, _>(Cursor::new(bytes), Some(14), 13, |bytes| {
            serde_json::from_slice(bytes)
        }),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        decode_bounded_json::<Body, _>(Cursor::new(bytes), None, 12, |bytes| {
            serde_json::from_slice(bytes)
        }),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn malformed_json_and_failed_reads_keep_distinct_error_classes() {
    assert_eq!(
        decode_bounded_json::<Body, _>(Cursor::new(b"not-json"), None, 32, |bytes| {
            serde_json::from_slice(bytes)
        }),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        decode_bounded_json::<Body, _>(FailedReader, None, 32, |bytes| serde_json::from_slice(
            bytes
        )),
        Err(ProviderError::Unavailable)
    );
    let mut nested = Nested(Vec::new());
    for _ in 0..126 {
        nested = Nested(vec![nested]);
    }
    let bytes = serde_json::to_vec(&nested).unwrap();
    assert_eq!(
        decode_bounded_json::<Nested, _>(Cursor::new(&bytes), None, bytes.len(), |bytes| {
            serde_json::from_slice(bytes)
        }),
        Ok((nested.clone(), bytes.len()))
    );
    let bytes = serde_json::to_vec(&Nested(vec![nested])).unwrap();
    assert_eq!(
        decode_bounded_json::<Nested, _>(Cursor::new(&bytes), None, bytes.len(), |bytes| {
            serde_json::from_slice(bytes)
        }),
        Err(ProviderError::InvalidResponse)
    );
}

struct FailedReader;

impl Read for FailedReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(Error::new(ErrorKind::ConnectionReset, "fixture failure"))
    }
}
