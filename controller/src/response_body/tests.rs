#![cfg(test)]

use std::io::{Cursor, Error, ErrorKind, Read};

use super::read_response_body;
use crate::ProviderError;

#[test]
fn declared_and_actual_body_lengths_share_one_ceiling() {
    let bytes = b"response body";
    assert_eq!(
        read_response_body(Cursor::new(bytes), Some(13), 13),
        Ok(bytes.to_vec())
    );
    assert_eq!(
        read_response_body(Cursor::new(bytes), Some(14), 13),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        read_response_body(Cursor::new(bytes), None, 12),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn admission_reads_only_the_single_byte_needed_to_prove_overflow() {
    let mut reader = Cursor::new(b"untrusted body continues beyond the limit");
    assert_eq!(
        read_response_body(&mut reader, None, 4),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(reader.position(), 5);
    reader.set_position(0);
    assert_eq!(
        read_response_body(&mut reader, Some(5), 4),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(reader.position(), 0);
}

#[test]
fn body_io_failures_remain_unavailable() {
    assert_eq!(
        read_response_body(FailedReader, None, 32),
        Err(ProviderError::Unavailable)
    );
    assert_eq!(
        read_response_body(Cursor::new(b""), Some(0), 0),
        Ok(Vec::new())
    );
}

struct FailedReader;

impl Read for FailedReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(Error::new(ErrorKind::ConnectionReset, "fixture failure"))
    }
}
