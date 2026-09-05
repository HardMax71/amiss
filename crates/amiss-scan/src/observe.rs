use amiss_wire::controls::SourceConstruct;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::Adapter;
use amiss_wire::report::IntentKind;
use amiss_wire::report::model::{
    ObservationIdInput, ObservationIdInputSchema, StructuralAddress, StructuralAddressSchema,
    TargetIntent,
};

use crate::resolve::Intent;

pub const OBSERVATION_ID_DOMAIN: &str = "amiss/observation-id";
pub const OBSERVATION_ID_INPUT_SCHEMA: &str = "amiss/scanner-observation-id-input";
pub const STRUCTURAL_ADDRESS_SCHEMA: &str = "amiss/scanner-structural-address";
pub const LINK_QUERY_DOMAIN: &str = "amiss/scanner-link-query";
pub const LINK_FRAGMENT_DOMAIN: &str = "amiss/scanner-link-fragment";

#[must_use]
pub fn query_digest(intent: &Intent) -> Option<Digest> {
    intent
        .query
        .as_deref()
        .map(|text| hb(LINK_QUERY_DOMAIN, text.as_bytes()))
}

#[must_use]
pub fn fragment_digest(intent: &Intent) -> Option<Digest> {
    intent
        .fragment
        .as_deref()
        .map(|text| hb(LINK_FRAGMENT_DOMAIN, text.as_bytes()))
}

pub struct ObservationIdentity<'a, P> {
    pub adapter: Adapter,
    pub contract_digest: Digest,
    pub document: P,
    pub repository_path: Option<P>,
    pub construct: SourceConstruct,
    pub node_path: &'a [usize],
    pub projection_digest: Digest,
    pub intent: &'a Intent,
    pub raw_destination_digest: Digest,
}

#[must_use]
pub fn target_intent<P>(
    intent: &Intent,
    raw_destination_digest: Digest,
    repository_path: Option<P>,
) -> TargetIntent<P> {
    TargetIntent {
        commit_oid: intent.commit_oid.clone(),
        external_scheme: intent
            .external_scheme
            .clone()
            .filter(|_scheme| intent.kind == IntentKind::ExternalUrl),
        fragment_digest: fragment_digest(intent),
        kind: intent.kind,
        query_digest: query_digest(intent),
        raw_destination_digest,
        repository_path,
        target_kind: intent.target_kind,
    }
}

/// # Errors
///
/// Rejects adapters without a structural observation address.
pub fn observation_input<P>(
    input: ObservationIdentity<'_, P>,
) -> Result<ObservationIdInput<P>, crate::Error> {
    Ok(ObservationIdInput {
        adapter_contract_digest: input.contract_digest,
        adapter_id: input.adapter,
        document: input.document,
        extracted_intent: target_intent(
            input.intent,
            input.raw_destination_digest,
            input.repository_path,
        ),
        schema: ObservationIdInputSchema::Current,
        source_construct: input.construct,
        source_projection_digest: input.projection_digest,
        structural_address: StructuralAddress {
            address_kind: input
                .adapter
                .metadata()
                .structural_address
                .ok_or(crate::Error::Internal)?,
            construct_index: 0,
            duplicate_index: 0,
            node_path: input
                .node_path
                .iter()
                .map(|index| i64::try_from(*index).unwrap_or(i64::MAX).unsigned_abs())
                .collect(),
            schema: StructuralAddressSchema::Current,
        },
    })
}
