use amiss_wire::controls::SourceConstruct;
use amiss_wire::digest::{Digest, hb, hj};
use amiss_wire::json::Value;
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::report::{EngineProvenance, IntentKind, adapter_contract};

use crate::resolve::Intent;
use crate::scan::ScannedOccurrence;

pub const OBSERVATION_ID_DOMAIN: &str = "amiss/observation-id";
pub const OBSERVATION_ID_INPUT_SCHEMA: &str = "amiss/scanner-observation-id-input";
pub const STRUCTURAL_ADDRESS_SCHEMA: &str = "amiss/scanner-structural-address";
pub const LINK_QUERY_DOMAIN: &str = "amiss/scanner-link-query";
pub const LINK_FRAGMENT_DOMAIN: &str = "amiss/scanner-link-fragment";

fn nullable_digest(digest: Option<Digest>) -> Value {
    digest.map_or(Value::Null, |value| Value::String(value.to_string()))
}

fn nullable_string(text: Option<&str>) -> Value {
    text.map_or(Value::Null, |value| Value::String(value.to_owned()))
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

/// The wire target intent: one flat shape whose null pattern is fixed by the
/// kind, embedding the raw-destination digest and both component digests.
#[must_use]
pub fn intent_value(intent: &Intent, raw_destination_digest: Digest) -> Value {
    let external_scheme = match intent.kind {
        IntentKind::ExternalUrl => intent.external_scheme.as_deref(),
        IntentKind::RepositoryPath
        | IntentKind::SameRepositoryGithub
        | IntentKind::SameRepositoryGitlab
        | IntentKind::SameRepositoryGitea
        | IntentKind::SiteRoute
        | IntentKind::Unsupported => None,
    };
    Value::Object(vec![
        (
            "kind".to_owned(),
            Value::String(intent.kind.as_str().to_owned()),
        ),
        (
            "raw_destination_digest".to_owned(),
            Value::String(raw_destination_digest.to_string()),
        ),
        (
            "repository_path".to_owned(),
            intent
                .repository_path
                .as_ref()
                .map_or(Value::Null, RepoPath::to_value),
        ),
        (
            "target_kind".to_owned(),
            nullable_string(
                intent
                    .target_kind
                    .map(amiss_wire::controls::TargetKind::as_str),
            ),
        ),
        (
            "query_digest".to_owned(),
            nullable_digest(query_digest(intent)),
        ),
        (
            "fragment_digest".to_owned(),
            nullable_digest(fragment_digest(intent)),
        ),
        (
            "external_scheme".to_owned(),
            nullable_string(external_scheme),
        ),
    ])
}

/// The structural address: the child-index path to the syntax node itself,
/// with the two reserved indices fixed at zero by the structural-address
/// contract.
#[must_use]
pub fn address_value(adapter: Adapter, node_path: &[usize]) -> Value {
    Value::Object(vec![
        (
            "schema".to_owned(),
            Value::String(STRUCTURAL_ADDRESS_SCHEMA.to_owned()),
        ),
        (
            "address_kind".to_owned(),
            Value::String(adapter.structural_address().to_owned()),
        ),
        (
            "node_path".to_owned(),
            Value::Array(
                node_path
                    .iter()
                    .map(|index| Value::Integer(i64::try_from(*index).unwrap_or(i64::MAX)))
                    .collect(),
            ),
        ),
        ("construct_index".to_owned(), Value::Integer(0)),
        ("duplicate_index".to_owned(), Value::Integer(0)),
    ])
}

/// The complete strict observation-identity input and its digest.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "the identity input is the contract's"
)]
pub fn observation_id(
    engine: &EngineProvenance,
    adapter: Adapter,
    document: &RepoPath,
    construct: SourceConstruct,
    node_path: &[usize],
    projection_digest: Digest,
    intent: &Intent,
    raw_destination_digest: Digest,
) -> (Value, Digest) {
    let (_descriptor, contract_digest) = adapter_contract(engine, adapter);
    let input = Value::Object(vec![
        (
            "schema".to_owned(),
            Value::String(OBSERVATION_ID_INPUT_SCHEMA.to_owned()),
        ),
        (
            "adapter_id".to_owned(),
            Value::String(adapter.adapter_id().to_owned()),
        ),
        (
            "adapter_contract_digest".to_owned(),
            Value::String(contract_digest.to_string()),
        ),
        ("document".to_owned(), document.to_value()),
        (
            "source_construct".to_owned(),
            Value::String(construct.as_str().to_owned()),
        ),
        (
            "structural_address".to_owned(),
            address_value(adapter, node_path),
        ),
        (
            "source_projection_digest".to_owned(),
            Value::String(projection_digest.to_string()),
        ),
        (
            "extracted_intent".to_owned(),
            intent_value(intent, raw_destination_digest),
        ),
    ]);
    let id = hj(OBSERVATION_ID_DOMAIN, &input);
    (input, id)
}

/// Builds one occurrence's identity from its scanned form.
#[must_use]
pub fn occurrence_id(
    engine: &EngineProvenance,
    adapter: Adapter,
    document: &RepoPath,
    scanned: &ScannedOccurrence,
    intent: &Intent,
) -> Digest {
    observation_id(
        engine,
        adapter,
        document,
        scanned.occurrence.construct,
        &scanned.occurrence.node_path,
        scanned.projection_digest,
        intent,
        scanned.raw_destination_digest,
    )
    .1
}
