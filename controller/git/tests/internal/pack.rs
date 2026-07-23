#![cfg(test)]

use std::io::{Cursor, Write as _};
use std::sync::Barrier;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use flate2::Compression;
use flate2::write::ZlibEncoder;

use super::{PackLimits, index_options, validate_and_spool, with_index_interrupt};

const SECOND: Duration = Duration::from_secs(1);

#[test]
fn fixed_limits_and_indexing_are_advertised() {
    let limits = PackLimits::DEFAULT;
    assert_eq!(limits.pack_bytes, 2_147_483_648);
    assert_eq!(limits.objects, 2_000_000);
    assert_eq!(limits.object_bytes, 134_217_728);
    assert_eq!(limits.inflated_bytes, 4_294_967_296);
    assert_eq!(limits.resolved_bytes, 4_294_967_296);
    assert_eq!(limits.delta_depth, 128);

    let options = index_options();
    assert_eq!(options.thread_limit, Some(1));
    assert_eq!(options.iteration_mode, gix_pack::data::input::Mode::Verify);
    assert_eq!(options.object_hash, gix::hash::Kind::Sha1);
}

#[test]
fn accepts_one_bounded_ordinary_object() {
    let bytes = pack([ordinary(3, b"hello")]);
    assert!(validate(&bytes, limits()).is_ok());
}

#[test]
fn indexes_the_same_validated_pack_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = pack([ordinary(3, b"hello")]);
    let root = tempfile::tempdir()?;
    let pack_directory = root.path().join("pack");
    std::fs::create_dir(&pack_directory)?;
    let mut input = Cursor::new(bytes);
    let mut progress = gix::progress::Discard;
    let cancelled = AtomicBool::new(false);
    let _installed = super::validate_and_install(
        &mut input,
        &pack_directory,
        &mut progress,
        &cancelled,
        Instant::now(),
        SECOND,
    )?;
    let extensions = std::fs::read_dir(pack_directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .path()
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    assert!(extensions.iter().any(|extension| extension == "pack"));
    assert!(extensions.iter().any(|extension| extension == "idx"));
    Ok(())
}

#[test]
fn rejects_declared_object_count_before_allocating_entries() {
    let bytes = pack_header(2);
    let limits = PackLimits {
        objects: 1,
        ..limits()
    };
    assert!(validate(&bytes, limits).is_err());
}

#[test]
fn rejects_pack_stream_over_byte_limit() {
    let bytes = pack([ordinary(3, b"hello")]);
    let limits = PackLimits {
        pack_bytes: u64::try_from(bytes.len())
            .unwrap_or(u64::MAX)
            .saturating_sub(1),
        ..limits()
    };
    assert!(validate(&bytes, limits).is_err());
}

#[test]
fn rejects_inflate_larger_than_declared_entry() {
    let bytes = pack([entry(3, 1, b"two", &[])]);
    assert!(validate(&bytes, limits()).is_err());
}

#[test]
fn rejects_aggregate_inflation() {
    let bytes = pack([ordinary(3, b"abc"), ordinary(3, b"def")]);
    let limits = PackLimits {
        inflated_bytes: 5,
        ..limits()
    };
    assert!(validate(&bytes, limits).is_err());
}

#[test]
fn rejects_reference_delta_without_reading_an_external_base() {
    let bytes = pack([entry(7, 0, b"", &[0_u8; 20])]);
    assert!(validate(&bytes, limits()).is_err());
}

#[test]
fn rejects_offset_delta_without_an_exact_prior_entry() {
    let bytes = pack([entry(6, 2, &[0, 0], &[1])]);
    assert!(validate(&bytes, limits()).is_err());
}

#[test]
fn rejects_delta_result_larger_than_object_limit() {
    let base = ordinary(3, b"a");
    let delta_offset = 12_u64.saturating_add(u64::try_from(base.len()).unwrap_or(u64::MAX));
    let distance = delta_offset.saturating_sub(12);
    let delta = entry(6, 2, &[1, 6], &offset(distance));
    let bytes = pack([base, delta]);
    let limits = PackLimits {
        object_bytes: 5,
        ..limits()
    };
    assert!(validate(&bytes, limits).is_err());
}

#[test]
fn rejects_delta_chain_past_depth_limit() {
    let base = ordinary(3, b"a");
    let first_offset = 12_u64.saturating_add(u64::try_from(base.len()).unwrap_or(u64::MAX));
    let first = entry(6, 2, &[1, 1], &offset(first_offset.saturating_sub(12)));
    let second_offset = first_offset.saturating_add(u64::try_from(first.len()).unwrap_or(u64::MAX));
    let second = entry(
        6,
        2,
        &[1, 1],
        &offset(second_offset.saturating_sub(first_offset)),
    );
    let bytes = pack([base, first, second]);
    let limits = PackLimits {
        delta_depth: 1,
        ..limits()
    };
    assert!(validate(&bytes, limits).is_err());
}

#[test]
fn rejects_missing_and_trailing_checksum_bytes() {
    let mut missing = pack([ordinary(3, b"a")]);
    missing.pop();
    assert!(validate(&missing, limits()).is_err());

    let mut trailing = pack([ordinary(3, b"a")]);
    trailing.push(0);
    assert!(validate(&trailing, limits()).is_err());
}

#[test]
fn cancellation_and_deadline_stop_receipt() {
    let bytes = pack([ordinary(3, b"a")]);
    let cancelled = AtomicBool::new(true);
    assert!(validate_at(&bytes, limits(), &cancelled, Instant::now(), SECOND).is_err());

    let active = AtomicBool::new(false);
    let expired = Instant::now()
        .checked_sub(SECOND)
        .unwrap_or_else(Instant::now);
    assert!(validate_at(&bytes, limits(), &active, expired, SECOND).is_err());
}

#[test]
fn expired_deadline_prevents_indexing_from_starting() {
    let cancelled = AtomicBool::new(false);
    let called = AtomicBool::new(false);
    let expired = Instant::now()
        .checked_sub(SECOND)
        .unwrap_or_else(Instant::now);
    let result = with_index_interrupt(&cancelled, expired, SECOND, |_interrupted| {
        called.store(true, Ordering::Release);
        Ok(())
    });
    assert!(result.is_err());
    assert!(!called.load(Ordering::Acquire));
}

#[test]
fn deadline_and_lease_interrupt_active_indexing() {
    let cancelled = AtomicBool::new(false);
    let observed = AtomicBool::new(false);
    let started = Instant::now();
    let result = with_index_interrupt(
        &cancelled,
        started,
        Duration::from_millis(25),
        |interrupted| observe_interrupt(interrupted, &observed),
    );
    assert!(result.is_err());
    assert!(observed.load(Ordering::Acquire));
    assert!(started.elapsed() < SECOND);

    let cancelled = AtomicBool::new(false);
    let observed = AtomicBool::new(false);
    let entered = Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            entered.wait();
            cancelled.store(true, Ordering::Release);
        });
        let result = with_index_interrupt(&cancelled, Instant::now(), SECOND, |interrupted| {
            entered.wait();
            observe_interrupt(interrupted, &observed)
        });
        assert!(result.is_err());
    });
    assert!(observed.load(Ordering::Acquire));
    assert!(cancelled.load(Ordering::Acquire));
}

fn observe_interrupt(
    interrupted: &AtomicBool,
    observed: &AtomicBool,
) -> Result<(), super::PackError> {
    let waiting = Instant::now();
    while !interrupted.load(Ordering::Acquire) {
        if waiting.elapsed() >= SECOND {
            return Err(super::PackError("the test interrupt was not delivered"));
        }
        std::thread::yield_now();
    }
    observed.store(true, Ordering::Release);
    Ok(())
}

fn validate(bytes: &[u8], limits: PackLimits) -> Result<(), super::PackError> {
    validate_at(
        bytes,
        limits,
        &AtomicBool::new(false),
        Instant::now(),
        SECOND,
    )
}

fn validate_at(
    bytes: &[u8],
    limits: PackLimits,
    cancelled: &AtomicBool,
    started: Instant,
    timeout: Duration,
) -> Result<(), super::PackError> {
    let mut input = Cursor::new(bytes);
    let mut spool = tempfile::tempfile().map_err(super::io_error)?;
    validate_and_spool(&mut input, &mut spool, limits, cancelled, started, timeout)
}

fn limits() -> PackLimits {
    PackLimits {
        pack_bytes: 1_048_576,
        objects: 16,
        object_bytes: 1_024,
        inflated_bytes: 4_096,
        resolved_bytes: 4_096,
        delta_depth: 8,
    }
}

fn pack<const N: usize>(entries: [Vec<u8>; N]) -> Vec<u8> {
    let mut bytes = pack_header(u32::try_from(N).unwrap_or(u32::MAX));
    for entry in entries {
        bytes.extend_from_slice(&entry);
    }
    let mut digest = gix::hash::hasher(gix::hash::Kind::Sha1);
    digest.update(&bytes);
    match digest.try_finalize() {
        Ok(digest) => bytes.extend_from_slice(digest.as_slice()),
        Err(_defect) => bytes.extend_from_slice(&[0_u8; 20]),
    }
    bytes
}

fn pack_header(objects: u32) -> Vec<u8> {
    let mut bytes = b"PACK".to_vec();
    bytes.extend_from_slice(&2_u32.to_be_bytes());
    bytes.extend_from_slice(&objects.to_be_bytes());
    bytes
}

fn ordinary(kind: u8, data: &[u8]) -> Vec<u8> {
    entry(
        kind,
        u64::try_from(data.len()).unwrap_or(u64::MAX),
        data,
        &[],
    )
}

fn entry(kind: u8, size: u64, data: &[u8], prefix: &[u8]) -> Vec<u8> {
    let mut bytes = entry_header(kind, size);
    bytes.extend_from_slice(prefix);
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    if encoder.write_all(data).is_err() {
        return Vec::new();
    }
    bytes.extend_from_slice(&encoder.finish().unwrap_or_default());
    bytes
}

fn entry_header(kind: u8, mut size: u64) -> Vec<u8> {
    let low = u8::try_from(size & 15).unwrap_or(0);
    size >>= 4;
    let mut first = kind.checked_shl(4).unwrap_or(0) | low;
    if size != 0 {
        first |= 128;
    }
    let mut bytes = vec![first];
    while size != 0 {
        let mut byte = u8::try_from(size & 127).unwrap_or(0);
        size >>= 7;
        if size != 0 {
            byte |= 128;
        }
        bytes.push(byte);
    }
    bytes
}

fn offset(mut distance: u64) -> Vec<u8> {
    let mut bytes = vec![u8::try_from(distance & 127).unwrap_or(0)];
    while distance > 127 {
        distance = (distance >> 7).saturating_sub(1);
        bytes.push(u8::try_from(distance & 127).unwrap_or(0) | 128);
    }
    bytes.reverse();
    bytes
}
