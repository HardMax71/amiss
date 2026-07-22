use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, DeliveryHeader, DeliveryRoute, GitHubWebhook,
    IngressCheck, OpaqueId, ProviderAdapter, ProviderError, ProviderIdentity, ProviderNamespace,
    Publication, SignedTimePolicy, UntrustedDelivery, VerifiedDelivery, WebhookKey, WebhookKeyring,
};

const FLOW_SIGNATURE: &[u8] =
    b"sha256=ac6a690197321dcf9b6291614f70f95fc93f096f646a1209c6d9de950ba0cb43";
const FLOW_HEADERS: &[DeliveryHeader<'_>] = &[DeliveryHeader {
    name: "x-hub-signature-256",
    value: FLOW_SIGNATURE,
}];
const FLOW_SECRET: &[u8] = b"It's a Secret to Everybody";

pub(crate) struct FakeAdapter {
    namespace: ProviderNamespace,
    route: DeliveryRoute,
    verifier: GitHubWebhook,
    pub(crate) authenticated: AuthenticatedDelivery,
    refreshes: Mutex<VecDeque<Result<ChangeSnapshot, ProviderError>>>,
    publications: Mutex<Vec<Publication>>,
    publish_results: Mutex<VecDeque<Result<(), ProviderError>>>,
    pub(crate) authentication_count: AtomicUsize,
    pub(crate) refresh_count: AtomicUsize,
}

impl FakeAdapter {
    pub(crate) fn new(
        authenticated: AuthenticatedDelivery,
        refreshes: impl IntoIterator<Item = Result<ChangeSnapshot, ProviderError>>,
    ) -> Self {
        let trust_set = OpaqueId::new("webhooks-main".to_owned()).unwrap();
        let anchor = OpaqueId::new("anchor-current".to_owned()).unwrap();
        let key = WebhookKey::new(anchor, FLOW_SECRET.to_vec(), 0, None).unwrap();
        Self {
            namespace: authenticated.identity.provider.namespace.clone(),
            route: DeliveryRoute {
                provider: authenticated.identity.provider.clone(),
                trust_set: trust_set.clone(),
                signed_time: SignedTimePolicy::ReplayOnly,
            },
            verifier: GitHubWebhook::new(WebhookKeyring::new(trust_set, vec![key]).unwrap()),
            authenticated,
            refreshes: Mutex::new(refreshes.into_iter().collect()),
            publications: Mutex::new(Vec::new()),
            publish_results: Mutex::new(VecDeque::new()),
            authentication_count: AtomicUsize::new(0),
            refresh_count: AtomicUsize::new(0),
        }
    }

    pub(crate) fn publications(&self) -> Vec<Publication> {
        self.publications.lock().unwrap().clone()
    }

    pub(crate) fn with_publish_results(
        self,
        results: impl IntoIterator<Item = Result<(), ProviderError>>,
    ) -> Self {
        *self.publish_results.lock().unwrap() = results.into_iter().collect();
        self
    }

    pub(crate) fn with_route_provider(mut self, provider: ProviderIdentity) -> Self {
        self.route.provider = provider;
        self
    }

    pub(crate) fn input(&self) -> UntrustedDelivery<'_> {
        self.input_with_body(br#"{"event":"change"}"#)
    }

    pub(crate) fn input_with_body<'a>(&'a self, body: &'a [u8]) -> UntrustedDelivery<'a> {
        UntrustedDelivery {
            route: &self.route,
            received_at_unix_millis: 1_800_000_000_000,
            headers: FLOW_HEADERS,
            body,
        }
    }
}

impl ProviderAdapter for FakeAdapter {
    fn namespace(&self) -> &ProviderNamespace {
        &self.namespace
    }

    fn authenticate(&self, delivery: IngressCheck<'_>) -> Result<VerifiedDelivery, ProviderError> {
        self.authentication_count.fetch_add(1, Ordering::Relaxed);
        self.verifier
            .verify(delivery)
            .map_err(|_| ProviderError::Authentication)
            .map(|proof| proof.bind(self.authenticated.clone()))
    }

    fn refresh(&self, _delivery: &AuthenticatedDelivery) -> Result<ChangeSnapshot, ProviderError> {
        self.refresh_count.fetch_add(1, Ordering::Relaxed);
        self.refreshes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(ProviderError::Unavailable))
    }

    fn publish(
        &self,
        _delivery: &AuthenticatedDelivery,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        self.publications.lock().unwrap().push(publication.clone());
        self.publish_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(()))
    }
}
