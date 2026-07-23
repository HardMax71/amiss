use std::fmt;
use std::io;

#[derive(Debug)]
pub enum InboxError {
    Configuration,
    AlreadyOpen,
    Full,
    Conflict,
    InvalidDelivery,
    Clock,
    Random,
    Corrupt,
    Io(io::Error),
}

impl fmt::Display for InboxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration => formatter.write_str("inbox configuration differs"),
            Self::AlreadyOpen => formatter.write_str("inbox already has an active owner"),
            Self::Full => formatter.write_str("inbox capacity is full"),
            Self::Conflict => formatter.write_str("delivery source identifies different bytes"),
            Self::InvalidDelivery => formatter.write_str("delivery exceeds or violates its limits"),
            Self::Clock => formatter.write_str("inbox time cannot be trusted"),
            Self::Random => formatter.write_str("inbox owner identity could not be created"),
            Self::Corrupt => formatter.write_str("inbox storage is corrupt"),
            Self::Io(error) => write!(formatter, "inbox I/O failed: {error}"),
        }
    }
}

impl std::error::Error for InboxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Configuration
            | Self::AlreadyOpen
            | Self::Full
            | Self::Conflict
            | Self::InvalidDelivery
            | Self::Clock
            | Self::Random
            | Self::Corrupt => None,
        }
    }
}

impl From<io::Error> for InboxError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
