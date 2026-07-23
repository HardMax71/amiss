use amiss_controller::{
    AcceptedDelivery, DeliveryHeader as IngressHeader, DeliveryRoute, IngressCheck, IngressPolicy,
    PlanRegistry, SystemClock, UntrustedDelivery, VerifiedDelivery, resolve_plan,
};

use crate::{AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission};

pub struct LaneAdmission<F> {
    pub route_id: String,
    pub route: DeliveryRoute,
    pub ingress: IngressPolicy,
    pub plans: PlanRegistry,
    pub authenticate: F,
}

pub fn lane_admission<F>(
    route_id: String,
    route: DeliveryRoute,
    ingress: IngressPolicy,
    plans: PlanRegistry,
    authenticate: F,
) -> LaneAdmission<F>
where
    F: for<'a> Fn(IngressCheck<'a>) -> Result<VerifiedDelivery, AdmissionRejection>
        + Send
        + Sync
        + 'static,
{
    LaneAdmission {
        route_id,
        route,
        ingress,
        plans,
        authenticate,
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
    authenticate: F,
) -> Result<AcceptedDelivery, AdmissionRejection>
where
    F: FnOnce(IngressCheck<'a>) -> Result<VerifiedDelivery, AdmissionRejection>,
{
    let checked = ingress
        .pre_auth(untrusted, &SystemClock)
        .map_err(|_defect| AdmissionRejection::Unauthorized)?;
    let verified = authenticate(checked)?;
    let accepted = ingress
        .post_auth(checked, verified)
        .map_err(|_defect| AdmissionRejection::Unauthorized)?;
    resolve_plan(plans, accepted.delivery()).map_err(|_defect| AdmissionRejection::Forbidden)?;
    Ok(accepted)
}

impl<F> DeliveryAdmission for LaneAdmission<F>
where
    F: for<'a> Fn(IngressCheck<'a>) -> Result<VerifiedDelivery, AdmissionRejection>
        + Send
        + Sync
        + 'static,
{
    fn admit(&self, request: AdmissionRequest<'_>) -> Result<AdmittedDelivery, AdmissionRejection> {
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
        let accepted = check_lane(&self.ingress, &self.plans, untrusted, &self.authenticate)?;
        Ok(AdmittedDelivery {
            route: self.route_id.clone(),
            source_id: accepted.delivery().identity.delivery.as_str().to_owned(),
        })
    }
}
