use serde::Serialize;
use serde::ser::SerializeSeq;

use amiss_wire::codec;
use amiss_wire::controls::SourceConstruct;
use amiss_wire::de::Error;
use amiss_wire::digest::{Digest, hb, hj_stream};
use amiss_wire::json::{self, Value};
use amiss_wire::model::{Adapter, Oid, RepoPath};
use amiss_wire::report::IntentKind;

use crate::resolve::Intent;

pub const OBSERVATION_ID_DOMAIN: &str = "amiss/observation-id";
pub const OBSERVATION_ID_INPUT_SCHEMA: &str = "amiss/scanner-observation-id-input";
pub const STRUCTURAL_ADDRESS_SCHEMA: &str = "amiss/scanner-structural-address";
pub const LINK_QUERY_DOMAIN: &str = "amiss/scanner-link-query";
pub const LINK_FRAGMENT_DOMAIN: &str = "amiss/scanner-link-fragment";

fn external_scheme(intent: &Intent) -> Option<&str> {
    match intent.kind {
        IntentKind::ExternalUrl => intent.external_scheme.as_deref(),
        IntentKind::RepositoryPath
        | IntentKind::SameRepositoryGithub
        | IntentKind::SameRepositoryGitlab
        | IntentKind::SameRepositoryGitea
        | IntentKind::SameRepositoryBitbucketCloud
        | IntentKind::SameRepositoryBitbucketDataCenter
        | IntentKind::SiteRoute
        | IntentKind::Label
        | IntentKind::Unsupported => None,
    }
}

/// The query component digest, where a present empty component hashes the
/// empty byte string and an absent one is null.
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

/// The borrowed fields of one observation-identity preimage.
pub struct ObservationIdentity<'a> {
    pub adapter: Adapter,
    pub contract_digest: Digest,
    pub document: &'a RepoPath,
    pub construct: SourceConstruct,
    pub node_path: &'a [usize],
    pub projection_digest: Digest,
    pub intent: &'a Intent,
    pub raw_destination_digest: Digest,
}

#[derive(Serialize)]
pub(crate) struct ExtractedIntent<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    commit_oid: Option<&'a Oid>,
    external_scheme: Option<&'a str>,
    fragment_digest: Option<Digest>,
    kind: &'a str,
    query_digest: Option<Digest>,
    raw_destination_digest: Digest,
    repository_path: Option<&'a RepoPath>,
    target_kind: Option<&'static str>,
}

pub(crate) fn extracted_intent(
    intent: &Intent,
    raw_destination_digest: Digest,
) -> ExtractedIntent<'_> {
    ExtractedIntent {
        commit_oid: intent.commit_oid.as_ref(),
        external_scheme: external_scheme(intent),
        fragment_digest: fragment_digest(intent),
        kind: intent.kind.as_ref(),
        query_digest: query_digest(intent),
        raw_destination_digest,
        repository_path: intent.repository_path.as_ref(),
        target_kind: intent.target_kind.map(Into::into),
    }
}

#[derive(Serialize)]
struct StructuralAddress<'a> {
    address_kind: &'static str,
    construct_index: u8,
    duplicate_index: u8,
    node_path: NodePath<'a>,
    schema: &'static str,
}

struct NodePath<'a>(&'a [usize]);

impl Serialize for NodePath<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for index in self.0 {
            let index = u64::try_from(*index).map_err(serde::ser::Error::custom)?;
            if index > codec::MAX_SAFE_INTEGER {
                return Err(serde::ser::Error::custom(
                    "node index exceeds the strict JSON integer range",
                ));
            }
            sequence.serialize_element(&index)?;
        }
        sequence.end()
    }
}

fn structural_address(adapter: Adapter, node_path: &[usize]) -> StructuralAddress<'_> {
    StructuralAddress {
        address_kind: adapter.metadata().structural_address,
        construct_index: 0,
        duplicate_index: 0,
        node_path: NodePath(node_path),
        schema: STRUCTURAL_ADDRESS_SCHEMA,
    }
}

#[derive(Serialize)]
pub(crate) struct ObservationInput<'a> {
    adapter_contract_digest: Digest,
    adapter_id: Adapter,
    document: &'a RepoPath,
    extracted_intent: ExtractedIntent<'a>,
    schema: &'static str,
    source_construct: SourceConstruct,
    source_projection_digest: Digest,
    structural_address: StructuralAddress<'a>,
}

pub(crate) fn observation_projection<'a>(input: &ObservationIdentity<'a>) -> ObservationInput<'a> {
    ObservationInput {
        adapter_contract_digest: input.contract_digest,
        adapter_id: input.adapter,
        document: input.document,
        extracted_intent: extracted_intent(input.intent, input.raw_destination_digest),
        schema: OBSERVATION_ID_INPUT_SCHEMA,
        source_construct: input.construct,
        source_projection_digest: input.projection_digest,
        structural_address: structural_address(input.adapter, input.node_path),
    }
}

/// Projects the wire target intent and its required nullable components.
///
/// # Errors
///
/// A value cannot be represented in the strict JSON profile.
pub fn intent_value(intent: &Intent, raw_destination_digest: Digest) -> Result<Value, Error> {
    codec::to_value(&extracted_intent(intent, raw_destination_digest))
}

/// Projects the node path and its two reserved zero indices.
///
/// # Errors
///
/// A node index cannot be represented in the strict JSON profile.
pub fn address_value(adapter: Adapter, node_path: &[usize]) -> Result<Value, Error> {
    codec::to_value(&structural_address(adapter, node_path))
}

/// The complete strict observation-identity input retained by the report.
///
/// # Errors
///
/// A value cannot be represented in the strict JSON profile.
pub fn observation_input(input: &ObservationIdentity<'_>) -> Result<Value, Error> {
    codec::to_value(&observation_projection(input))
}

/// Hashes the borrowed observation input without materializing its JSON tree.
///
/// # Errors
///
/// The Serde projection cannot be emitted.
pub fn observation_digest(input: &ObservationIdentity<'_>) -> Result<Digest, serde_json::Error> {
    let mut result = Ok(());
    let digest = hj_stream(OBSERVATION_ID_DOMAIN, |sink| {
        result = json::serialize(&observation_projection(input), sink);
    });
    result.map(|()| digest)
}
