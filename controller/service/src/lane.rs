use std::sync::Arc;

use amiss_controller::{
    AcceptedDelivery, ControllerClock, DeliveryHeader as IngressHeader, DeliveryRoute,
    IngressCheck, IngressPolicy, PlanRegistry, ProviderError, UntrustedDelivery, VerifiedDelivery,
    resolve_plan,
};

use crate::{AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission};

struct RepositoryAdmission<F> {
    route_id: String,
    route: DeliveryRoute,
    ingress: IngressPolicy,
    plans: PlanRegistry,
    clock: Arc<dyn ControllerClock>,
    repository_prefix: String,
    authenticate: F,
}

/// Binds authenticated deliveries to one provider repository identifier.
pub fn repository_admission<F>(
    route_id: String,
    route: DeliveryRoute,
    ingress: IngressPolicy,
    plans: PlanRegistry,
    clock: Arc<dyn ControllerClock>,
    repository_id: u64,
    authenticate: F,
) -> Arc<dyn DeliveryAdmission>
where
    F: for<'a> Fn(IngressCheck<'a>) -> Result<Option<VerifiedDelivery>, ProviderError>
        + Send
        + Sync
        + 'static,
{
    let repository_prefix = format!("repository/{repository_id}/");
    Arc::new(RepositoryAdmission {
        route_id,
        route,
        ingress,
        plans,
        clock,
        repository_prefix,
        authenticate,
    })
}

const fn provider_rejection(error: ProviderError) -> AdmissionRejection {
    match error {
        ProviderError::AuthorizationRevoked => AdmissionRejection::Forbidden,
        ProviderError::Authentication
        | ProviderError::Unavailable
        | ProviderError::InvalidResponse => AdmissionRejection::Unauthorized,
    }
}

/// Applies provider-neutral ingress, authentication, and plan checks without
/// touching durable state.
///
/// # Errors
///
/// The raw request, provider proof, authenticated route, or plan binding is
/// invalid.
pub fn check_lane<'a, F>(
    ingress: &IngressPolicy,
    plans: &PlanRegistry,
    untrusted: UntrustedDelivery<'a>,
    clock: &dyn ControllerClock,
    authenticate: F,
) -> Result<AcceptedDelivery, AdmissionRejection>
where
    F: FnOnce(IngressCheck<'a>) -> Result<VerifiedDelivery, AdmissionRejection>,
{
    let checked = ingress
        .pre_auth(untrusted, clock)
        .map_err(|_defect| AdmissionRejection::Unauthorized)?;
    let verified = authenticate(checked)?;
    accept_verified(ingress, plans, checked, verified)
}

fn accept_verified(
    ingress: &IngressPolicy,
    plans: &PlanRegistry,
    checked: IngressCheck<'_>,
    verified: VerifiedDelivery,
) -> Result<AcceptedDelivery, AdmissionRejection> {
    let accepted = ingress
        .post_auth(checked, verified)
        .map_err(|_defect| AdmissionRejection::Unauthorized)?;
    resolve_plan(plans, accepted.delivery()).map_err(|_defect| AdmissionRejection::Forbidden)?;
    Ok(accepted)
}

impl<F> DeliveryAdmission for RepositoryAdmission<F>
where
    F: for<'a> Fn(IngressCheck<'a>) -> Result<Option<VerifiedDelivery>, ProviderError>
        + Send
        + Sync
        + 'static,
{
    fn admit(
        &self,
        request: AdmissionRequest<'_>,
    ) -> Result<Option<AdmittedDelivery>, AdmissionRejection> {
        let headers = request
            .headers
            .iter()
            .map(|header| IngressHeader {
                name: &header.name,
                value: &header.value,
            })
            .collect::<Vec<_>>();
        let untrusted = UntrustedDelivery {
            route: &self.route,
            received_at_unix_millis: request.received_at_unix_millis,
            headers: &headers,
            body: request.body,
        };
        let checked = self
            .ingress
            .pre_auth(untrusted, self.clock.as_ref())
            .map_err(|_defect| AdmissionRejection::Unauthorized)?;
        let Some(verified) = (self.authenticate)(checked).map_err(provider_rejection)? else {
            return Ok(None);
        };
        let verified = verified
            .delivery()
            .change
            .change
            .as_str()
            .starts_with(&self.repository_prefix)
            .then_some(verified)
            .ok_or(AdmissionRejection::Forbidden)?;
        let accepted = accept_verified(&self.ingress, &self.plans, checked, verified)?;
        Ok(Some(AdmittedDelivery {
            route: self.route_id.clone(),
            source_id: accepted.delivery().identity.delivery.as_str().to_owned(),
        }))
    }
}
