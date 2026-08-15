use std::io;

#[derive(Debug, thiserror::Error)]
pub enum InboxError {
    #[error("inbox configuration differs")]
    Configuration,
    #[error("inbox already has an active owner")]
    AlreadyOpen,
    #[error("inbox capacity is full")]
    Full,
    #[error("delivery source identifies different bytes")]
    Conflict,
    #[error("delivery exceeds or violates its limits")]
    InvalidDelivery,
    #[error("inbox time cannot be trusted")]
    Clock,
    #[error("inbox owner identity could not be created")]
    Random,
    #[error("inbox storage is corrupt")]
    Corrupt,
    #[error("inbox I/O failed: {0}")]
    Io(#[from] io::Error),
}

impl From<amiss_controller::atomic_write_recovery::AtomicWriteDirectoryError> for InboxError {
    fn from(error: amiss_controller::atomic_write_recovery::AtomicWriteDirectoryError) -> Self {
        match error {
            amiss_controller::atomic_write_recovery::AtomicWriteDirectoryError::Io(error) => {
                Self::Io(error)
            }
            amiss_controller::atomic_write_recovery::AtomicWriteDirectoryError::Malformed => {
                Self::Corrupt
            }
        }
    }
}
