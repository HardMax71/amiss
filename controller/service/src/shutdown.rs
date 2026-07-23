use std::io;

use tokio::sync::watch;

/// Waits for the process-wide interrupt or termination signal.
///
/// # Errors
///
/// The process handler cannot be installed or stops before a signal arrives.
pub async fn shutdown_signal() -> io::Result<()> {
    let (sender, mut receiver) = watch::channel(false);
    ctrlc::set_handler(move || {
        let _ignored = sender.send(true);
    })
    .map_err(|_defect| io::Error::other("shutdown signal handler cannot be installed"))?;
    receiver
        .changed()
        .await
        .map_err(|_defect| io::Error::other("shutdown signal handler stopped"))?;
    Ok(())
}
