use std::time::Duration;

use axum::body::{self, Body, Bytes};

const READ_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) enum ReadError {
    Invalid,
    TimedOut,
}

pub(crate) async fn read(body: Body, limit: usize) -> Result<Bytes, ReadError> {
    tokio::time::timeout(READ_TIMEOUT, body::to_bytes(body, limit))
        .await
        .map_err(|_elapsed| ReadError::TimedOut)?
        .map_err(|_error| ReadError::Invalid)
}
