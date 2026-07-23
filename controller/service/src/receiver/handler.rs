use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Request, State};
use axum::http::StatusCode;
use tokio::sync::Semaphore;

use super::admission::{AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission};
use super::headers;
use crate::request_body::{self, ReadError};
use crate::{DeliveryHeader, EnqueueOutcome, Inbox, InboxError, IncomingDelivery, IncomingHeader};

#[derive(Clone)]
pub(super) struct ReceiverState {
    pub(super) admission: Arc<dyn DeliveryAdmission>,
    pub(super) inbox: Arc<Mutex<Inbox>>,
    pub(super) max_body_bytes: usize,
    pub(super) max_headers: u64,
    pub(super) max_header_bytes: u64,
    pub(super) permits: Arc<Semaphore>,
}

pub(super) async fn health() -> StatusCode {
    StatusCode::OK
}

pub(super) async fn receive(State(state): State<ReceiverState>, request: Request) -> StatusCode {
    let Some(received_at_unix_millis) = controller_time() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    let (parts, body) = request.into_parts();
    if parts.uri.query().is_some() {
        return StatusCode::BAD_REQUEST;
    }
    if !headers::within_limits(&parts.headers, state.max_headers, state.max_header_bytes) {
        return StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE;
    }
    let Ok(permit) = Arc::clone(&state.permits).try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    let body = match request_body::read(body, state.max_body_bytes).await {
        Ok(body) => body,
        Err(ReadError::Invalid) => return StatusCode::PAYLOAD_TOO_LARGE,
        Err(ReadError::TimedOut) => return StatusCode::REQUEST_TIMEOUT,
    };
    let headers = headers::materialize(&parts.headers);
    let outcome = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        dispatch(&state, received_at_unix_millis, &headers, body.as_ref())
    })
    .await;
    match outcome {
        Ok(outcome) => status(&outcome),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

enum DispatchOutcome {
    Accepted,
    Rejected(AdmissionRejection),
    Inbox(InboxError),
    Unavailable,
}

fn status(outcome: &DispatchOutcome) -> StatusCode {
    match outcome {
        DispatchOutcome::Accepted => StatusCode::ACCEPTED,
        DispatchOutcome::Rejected(AdmissionRejection::Malformed)
        | DispatchOutcome::Inbox(InboxError::InvalidDelivery) => StatusCode::BAD_REQUEST,
        DispatchOutcome::Rejected(AdmissionRejection::Unauthorized) => StatusCode::UNAUTHORIZED,
        DispatchOutcome::Rejected(AdmissionRejection::Forbidden) => StatusCode::FORBIDDEN,
        DispatchOutcome::Inbox(InboxError::Conflict) => StatusCode::CONFLICT,
        DispatchOutcome::Inbox(
            InboxError::Configuration
            | InboxError::AlreadyOpen
            | InboxError::Full
            | InboxError::Clock
            | InboxError::Random
            | InboxError::Corrupt
            | InboxError::Io(_),
        )
        | DispatchOutcome::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn dispatch(
    state: &ReceiverState,
    received_at_unix_millis: i64,
    headers: &[DeliveryHeader],
    body: &[u8],
) -> DispatchOutcome {
    let admitted = match state.admission.admit(AdmissionRequest {
        received_at_unix_millis,
        headers,
        body,
    }) {
        Ok(admitted) => admitted,
        Err(rejection) => return DispatchOutcome::Rejected(rejection),
    };
    enqueue(
        &state.inbox,
        &admitted,
        received_at_unix_millis,
        headers,
        body,
    )
}

fn enqueue(
    inbox: &Mutex<Inbox>,
    admitted: &AdmittedDelivery,
    received_at_unix_millis: i64,
    headers: &[DeliveryHeader],
    body: &[u8],
) -> DispatchOutcome {
    let incoming_headers = headers
        .iter()
        .map(|header| IncomingHeader {
            name: &header.name,
            value: &header.value,
        })
        .collect::<Vec<_>>();
    let incoming = IncomingDelivery {
        route: &admitted.route,
        source_id: &admitted.source_id,
        received_at_unix_millis,
        headers: &incoming_headers,
        body,
    };
    let Ok(mut inbox) = inbox.lock() else {
        return DispatchOutcome::Unavailable;
    };
    match inbox.enqueue(incoming) {
        Ok(EnqueueOutcome::Stored | EnqueueOutcome::Duplicate) => DispatchOutcome::Accepted,
        Err(error) => DispatchOutcome::Inbox(error),
    }
}

fn controller_time() -> Option<i64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(elapsed.as_millis()).ok()
}
