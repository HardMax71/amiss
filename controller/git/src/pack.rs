mod tests;
mod validation;

use std::io::{BufRead, BufReader, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use validation::validate_and_spool;

use crate::{GitFetchLimits, GitFetchUsage};

const INDEX_INTERRUPT_POLL: Duration = Duration::from_millis(5);

#[derive(Clone, Copy)]
pub(super) struct PackLimits {
    pack_bytes: u64,
    objects: u32,
    object_bytes: u64,
    inflated_bytes: u64,
    resolved_bytes: u64,
    delta_depth: u16,
}

const DEFAULT_LIMITS: PackLimits = PackLimits {
    pack_bytes: crate::DEFAULT_GIT_FETCH_LIMITS.bytes,
    objects: 2_000_000,
    object_bytes: 134_217_728,
    inflated_bytes: 4_294_967_296,
    resolved_bytes: 4_294_967_296,
    delta_depth: 128,
};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(super) struct PackError(&'static str);

pub(super) struct InstalledPack {
    pub(super) keep_path: Option<PathBuf>,
    pub(super) usage: GitFetchUsage,
}

pub(super) fn fetch_limits(requested: GitFetchLimits) -> Result<PackLimits, PackError> {
    if requested.objects == 0 || requested.bytes == 0 {
        return Err(PackError("the exact Git fetch resource limit is invalid"));
    }
    Ok(PackLimits {
        pack_bytes: requested.bytes.min(DEFAULT_LIMITS.pack_bytes),
        objects: u32::try_from(requested.objects.min(u64::from(DEFAULT_LIMITS.objects)))
            .map_err(|_defect| PackError("the exact Git object limit is unsupported"))?,
        ..DEFAULT_LIMITS
    })
}

pub(super) fn validate_and_install(
    input: &mut dyn BufRead,
    pack_directory: &Path,
    progress: &mut dyn gix::progress::DynNestedProgress,
    cancelled: &AtomicBool,
    started: Instant,
    timeout: Duration,
    limits: PackLimits,
) -> Result<InstalledPack, PackError> {
    let mut spool = tempfile::tempfile_in(pack_directory)
        .map_err(|_defect| PackError("the pack stream is unreadable"))?;
    let usage = validate_and_spool(input, &mut spool, limits, cancelled, started, timeout)?;
    active(cancelled, started, timeout)?;
    spool
        .seek(SeekFrom::Start(0))
        .map_err(|_defect| PackError("the pack stream is unreadable"))?;
    let outcome = with_index_interrupt(cancelled, started, timeout, |interrupted| {
        gix_pack::Bundle::write_to_directory(
            &mut BufReader::new(spool),
            Some(pack_directory),
            progress,
            interrupted,
            None::<gix::objs::find::Never>,
            gix::hash::Kind::Sha1,
            index_options(limits),
        )
        .map_err(|_defect| PackError("the validated pack could not be indexed"))
    })?;
    Ok(InstalledPack {
        keep_path: outcome.keep_path,
        usage,
    })
}

fn with_index_interrupt<T>(
    cancelled: &AtomicBool,
    started: Instant,
    timeout: Duration,
    index: impl FnOnce(&AtomicBool) -> Result<T, PackError>,
) -> Result<T, PackError> {
    active(cancelled, started, timeout)?;
    let interrupted = AtomicBool::new(false);
    let outcome = std::thread::scope(|scope| {
        let (finished, completion) = mpsc::sync_channel(0);
        let watcher_interrupted = &interrupted;
        let watcher = std::thread::Builder::new()
            .name("amiss-pack-deadline".to_owned())
            .spawn_scoped(scope, move || {
                watch_index(
                    cancelled,
                    watcher_interrupted,
                    started,
                    timeout,
                    &completion,
                );
            })
            .map_err(|_defect| PackError("the pack deadline watcher cannot start"))?;
        let outcome = index(&interrupted);
        drop(finished);
        watcher
            .join()
            .map_err(|_defect| PackError("the pack deadline watcher stopped"))?;
        Ok(outcome)
    })??;
    active(cancelled, started, timeout)?;
    Ok(outcome)
}

fn watch_index(
    cancelled: &AtomicBool,
    interrupted: &AtomicBool,
    started: Instant,
    timeout: Duration,
    completion: &mpsc::Receiver<()>,
) {
    loop {
        let elapsed = started.elapsed();
        if cancelled.load(Ordering::Acquire) || elapsed >= timeout {
            interrupted.store(true, Ordering::Release);
            return;
        }
        match completion.recv_timeout(timeout.saturating_sub(elapsed).min(INDEX_INTERRUPT_POLL)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn index_options(limits: PackLimits) -> gix_pack::bundle::write::Options {
    gix_pack::bundle::write::Options {
        thread_limit: Some(1),
        iteration_mode: gix_pack::data::input::Mode::Verify,
        index_version: gix_pack::index::Version::default(),
        alloc_limit_bytes: Some(usize::try_from(limits.object_bytes).unwrap_or(usize::MAX)),
        compression: gix::zlib::Compression::BEST_SPEED,
    }
}

fn active(cancelled: &AtomicBool, started: Instant, timeout: Duration) -> Result<(), PackError> {
    if cancelled.load(Ordering::Acquire) {
        Err(PackError("pack receipt was cancelled"))
    } else if started.elapsed() >= timeout {
        Err(PackError("the Git fetch deadline elapsed"))
    } else {
        Ok(())
    }
}
