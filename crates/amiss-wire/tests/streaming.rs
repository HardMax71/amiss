#![expect(
    clippy::unwrap_used,
    reason = "tests require deterministic writer outcomes"
)]

use std::io::{self, Write};

use amiss_wire::json::{self, Value};
use amiss_wire::report::{FATAL_SCRATCH_BYTES, FatalSerializer};

struct ShortWrites(Vec<u8>);

impl Write for ShortWrites {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let prefix = bytes.get(..bytes.len().min(7)).unwrap();
        self.0.extend_from_slice(prefix);
        Ok(prefix.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("writer sentinel")]
struct Marker;

struct Refusal;

impl Write for Refusal {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, Marker))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn canonical_and_fatal_streams_complete_short_writes_without_changing_bytes() {
    let mut reserve = FatalSerializer::new();
    for size in [
        0,
        1,
        FATAL_SCRATCH_BYTES - 1,
        FATAL_SCRATCH_BYTES,
        FATAL_SCRATCH_BYTES + 1,
    ] {
        let value = serde_json::json!({
            "\u{e000}": [true, null, 42, "escaped \" slash \\ line\n"],
            "\u{10000}": {"payload": "x".repeat(size)},
        });
        let expected = json::canonical(&value);
        let mut direct = ShortWrites(Vec::new());
        json::write_to(&value, &mut direct).unwrap();
        assert_eq!(direct.0, expected);
        assert_eq!(
            serde_json::to_vec(&json::canonical_view(&value)).unwrap(),
            expected
        );

        let mut expected_line = expected;
        expected_line.push(b'\n');
        let mut staged = ShortWrites(Vec::new());
        let count = reserve.emit(&value, &mut staged).unwrap();
        assert_eq!(staged.0, expected_line);
        assert_eq!(count, u64::try_from(expected_line.len()).unwrap());
    }
}

#[test]
fn writer_errors_preserve_the_source_and_leave_the_fatal_reserve_reusable() {
    let envelope = serde_json::json!({"payload": "x".repeat(FATAL_SCRATCH_BYTES * 2)});
    let failure = json::write_to(&envelope, &mut Refusal).unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::BrokenPipe);
    assert!(failure.get_ref().unwrap().is::<Marker>());

    let mut reserve = FatalSerializer::new();
    let failure = reserve.emit(&envelope, &mut Refusal).unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::BrokenPipe);
    assert!(failure.get_ref().unwrap().is::<Marker>());
    let mut recovered = Vec::new();
    assert_eq!(reserve.emit(&Value::Null, &mut recovered).unwrap(), 5);
    assert_eq!(recovered, b"null\n");

    let mut empty = [];
    let mut full = io::Cursor::new(empty.as_mut_slice());
    assert_eq!(
        reserve.emit(&envelope, &mut full).unwrap_err().kind(),
        io::ErrorKind::WriteZero
    );
}
