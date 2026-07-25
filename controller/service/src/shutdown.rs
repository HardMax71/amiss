use std::future::Future;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::watch;

/// Installs the process-wide signal handler and returns its first-signal future.
///
/// # Errors
///
/// The process handler cannot be installed.
pub fn shutdown_signal() -> io::Result<impl Future<Output = io::Result<()>>> {
    let (sender, mut receiver) = watch::channel(false);
    let signaled = AtomicBool::new(false);
    ctrlc::set_handler(move || {
        if signaled.swap(true, Ordering::AcqRel) {
            std::process::abort();
        }
        let _ignored = sender.send(true);
    })
    .map_err(|_defect| io::Error::other("shutdown signal handler cannot be installed"))?;
    Ok(async move {
        receiver
            .changed()
            .await
            .map_err(|_closed| io::Error::other("shutdown signal handler stopped"))
    })
}
