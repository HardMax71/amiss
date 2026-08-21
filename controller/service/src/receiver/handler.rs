use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::StatusCode;

use super::admission::{AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission};
use crate::endpoint::{self, EndpointState};
use crate::{DeliveryHeader, EnqueueOutcome, Inbox, InboxError, IncomingDelivery, IncomingHeader};

#[derive(Clone)]
pub(super) struct ReceiverState {
    pub(super) admission: Arc<dyn DeliveryAdmission>,
    pub(super) inbox: Arc<Mutex<Inbox>>,
    pub(super) endpoint: EndpointState,
}

pub(super) async fn receive(State(state): State<ReceiverState>, request: Request) -> StatusCode {
    let ReceiverState {
        admission,
        inbox,
        endpoint,
    } = state;
    endpoint::bounded_request(
        &endpoint,
        request,
        move |received_at_unix_millis, headers, body| {
            dispatch(
                admission.as_ref(),
                inbox.as_ref(),
                received_at_unix_millis,
                headers,
                body,
            )
        },
    )
    .await
    .map_or_else(std::convert::identity, |outcome| status(&outcome))
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
    admission: &dyn DeliveryAdmission,
    inbox: &Mutex<Inbox>,
    received_at_unix_millis: i64,
    headers: &[DeliveryHeader],
    body: &[u8],
) -> DispatchOutcome {
    let admitted = match admission.admit(AdmissionRequest {
        received_at_unix_millis,
        headers,
        body,
    }) {
        Ok(Some(admitted)) => admitted,
        Ok(None) => return DispatchOutcome::Accepted,
        Err(rejection) => return DispatchOutcome::Rejected(rejection),
    };
    enqueue(inbox, &admitted, received_at_unix_millis, headers, body)
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
