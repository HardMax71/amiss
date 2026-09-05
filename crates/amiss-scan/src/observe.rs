use amiss_wire::controls::SourceConstruct;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::json::Value;
use amiss_wire::model::{Adapter, RepoPath};
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

/// The wire target intent: one flat shape whose null pattern is fixed by the
/// kind, embedding the raw-destination digest and both component digests.
#[must_use]
pub fn intent_value(intent: &Intent, raw_destination_digest: Digest) -> Value {
    Value::object(
        intent
            .commit_oid
            .iter()
            .map(|oid| {
                (
                    "commit_oid".to_owned(),
                    Value::string(oid.as_str().to_owned()),
                )
            })
            .chain([
                (
                    "kind".to_owned(),
                    Value::string(intent.kind.as_ref().to_owned()),
                ),
                (
                    "raw_destination_digest".to_owned(),
                    Value::string(raw_destination_digest.to_string()),
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
                    intent.target_kind.map_or(Value::Null, |kind| {
                        Value::string(Into::<&'static str>::into(kind).to_owned())
                    }),
                ),
                (
                    "query_digest".to_owned(),
                    query_digest(intent)
                        .map_or(Value::Null, |digest| Value::string(digest.to_string())),
                ),
                (
                    "fragment_digest".to_owned(),
                    fragment_digest(intent)
                        .map_or(Value::Null, |digest| Value::string(digest.to_string())),
                ),
                (
                    "external_scheme".to_owned(),
                    external_scheme(intent)
                        .map_or(Value::Null, |scheme| Value::string(scheme.to_owned())),
                ),
            ])
            .collect(),
    )
}

/// The structural address: the child-index path to the syntax node itself,
/// with the two reserved indices fixed at zero by the structural-address
/// contract.
#[must_use]
pub fn address_value(adapter: Adapter, node_path: &[usize]) -> Value {
    Value::object(vec![
        (
            "schema".to_owned(),
            Value::string(STRUCTURAL_ADDRESS_SCHEMA.to_owned()),
        ),
        (
            "address_kind".to_owned(),
            Value::string(
                adapter
                    .metadata()
                    .structural_address
                    .map_or("none", Into::into)
                    .to_owned(),
            ),
        ),
        (
            "node_path".to_owned(),
            Value::array(
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

/// The complete strict observation-identity input retained by the report.
#[must_use]
pub fn observation_input(input: &ObservationIdentity<'_>) -> Value {
    Value::object(vec![
        (
            "schema".to_owned(),
            Value::string(OBSERVATION_ID_INPUT_SCHEMA.to_owned()),
        ),
        (
            "adapter_id".to_owned(),
            Value::string(input.adapter.as_ref().to_owned()),
        ),
        (
            "adapter_contract_digest".to_owned(),
            Value::string(input.contract_digest.to_string()),
        ),
        ("document".to_owned(), input.document.to_value()),
        (
            "source_construct".to_owned(),
            Value::string(Into::<&'static str>::into(input.construct).to_owned()),
        ),
        (
            "structural_address".to_owned(),
            address_value(input.adapter, input.node_path),
        ),
        (
            "source_projection_digest".to_owned(),
            Value::string(input.projection_digest.to_string()),
        ),
        (
            "extracted_intent".to_owned(),
            intent_value(input.intent, input.raw_destination_digest),
        ),
    ])
}
