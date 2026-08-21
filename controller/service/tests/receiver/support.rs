use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_controller_service::{
    AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission, EndpointConfig,
    Inbox, InboxLimits, Operations, router,
};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request};
use tempfile::TempDir;

pub(crate) const DELIVERY_PATH: &str = "/provider/delivery";
pub(crate) const SECRET: &str = "receiver-secret-4f61";
pub(crate) const BODY: &[u8] = b"provider-body-82a9";

pub(crate) struct TestAdmission {
    calls: AtomicUsize,
    rejection: Option<AdmissionRejection>,
    work: bool,
}

impl TestAdmission {
    pub(crate) const fn accepting() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            rejection: None,
            work: true,
        }
    }

    pub(crate) const fn no_work() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            rejection: None,
            work: false,
        }
    }

    pub(crate) const fn rejecting(rejection: AdmissionRejection) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            rejection: Some(rejection),
            work: true,
        }
    }

    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl DeliveryAdmission for TestAdmission {
    fn admit(
        &self,
        request: AdmissionRequest<'_>,
    ) -> Result<Option<AdmittedDelivery>, AdmissionRejection> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(rejection) = self.rejection {
            return Err(rejection);
        }
        if request.received_at_unix_millis <= 0
            || header(request.headers, "x-provider-secret") != Some(SECRET.as_bytes())
        {
            return Err(AdmissionRejection::Unauthorized);
        }
        if request.body.is_empty() {
            return Err(AdmissionRejection::Malformed);
        }
        if !self.work {
            return Ok(None);
        }
        let source_id = header(request.headers, "x-source-id")
            .and_then(|value| std::str::from_utf8(value).ok())
            .filter(|value| !value.is_empty())
            .ok_or(AdmissionRejection::Malformed)?;
        Ok(Some(AdmittedDelivery {
            route: "github-main".to_owned(),
            source_id: source_id.to_owned(),
        }))
    }
}

pub(crate) struct Fixture {
    pub(crate) app: Router,
    pub(crate) inbox: Arc<Mutex<Inbox>>,
    pub(crate) admission: Arc<TestAdmission>,
    pub(crate) ready: Arc<AtomicBool>,
    pub(crate) operations: Operations,
    _directory: TempDir,
}

impl Fixture {
    pub(crate) fn new(
        receiver_config: &EndpointConfig,
        inbox_limits: InboxLimits,
        admission: TestAdmission,
    ) -> Self {
        let directory = TempDir::new().unwrap();
        let inbox = Arc::new(Mutex::new(
            Inbox::open(directory.path(), inbox_limits).unwrap(),
        ));
        let admission = Arc::new(admission);
        let receiver_admission: Arc<dyn DeliveryAdmission> = admission.clone();
        let ready = Arc::new(AtomicBool::new(true));
        let operations = Operations::default();
        let (app, _drain) = router(
            receiver_config,
            inbox.clone(),
            receiver_admission,
            Arc::clone(&ready),
            operations.clone(),
        )
        .unwrap();
        Self {
            app,
            inbox,
            admission,
            ready,
            operations,
            _directory: directory,
        }
    }
}

pub(crate) fn receiver_config() -> EndpointConfig {
    EndpointConfig {
        path: DELIVERY_PATH.to_owned(),
        max_body_bytes: 1_024,
        max_headers: 16,
        max_header_bytes: 2_048,
        max_concurrent_requests: 4,
    }
}

pub(crate) fn inbox_limits() -> InboxLimits {
    InboxLimits {
        lease_duration: Duration::from_millis(100),
        max_records: 8,
        max_bytes: 262_144,
        max_record_bytes: 131_072,
        max_body_bytes: 4_096,
        max_headers: 16,
        max_header_bytes: 2_048,
        max_route_bytes: 128,
        max_source_id_bytes: 128,
    }
}

pub(crate) fn delivery_request(
    method: Method,
    path: &str,
    source_id: &str,
    body: &[u8],
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("x-provider-secret", SECRET)
        .header("x-source-id", source_id)
        .body(Body::from(body.to_vec()))
        .unwrap()
}

fn header<'a>(
    headers: &'a [amiss_controller_service::DeliveryHeader],
    name: &str,
) -> Option<&'a [u8]> {
    headers
        .iter()
        .find(|header| header.name == name)
        .map(|header| header.value.as_slice())
}
