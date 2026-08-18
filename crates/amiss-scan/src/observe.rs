use std::fmt;

use amiss_wire::controls::SourceConstruct;
use amiss_wire::digest::{Digest, hb, hj_stream};
use amiss_wire::json::{Sink, Value, write_string};
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::report::IntentKind;

use crate::resolve::Intent;

pub const OBSERVATION_ID_DOMAIN: &str = "amiss/observation-id";
pub const OBSERVATION_ID_INPUT_SCHEMA: &str = "amiss/scanner-observation-id-input";
pub const STRUCTURAL_ADDRESS_SCHEMA: &str = "amiss/scanner-structural-address";
pub const LINK_QUERY_DOMAIN: &str = "amiss/scanner-link-query";
pub const LINK_FRAGMENT_DOMAIN: &str = "amiss/scanner-link-fragment";

struct SinkFormatter<'a>(&'a mut dyn Sink);

impl fmt::Write for SinkFormatter<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0.write(text);
        Ok(())
    }
}

fn external_scheme(intent: &Intent) -> Option<&str> {
    match intent.kind {
        IntentKind::ExternalUrl => intent.external_scheme.as_deref(),
        IntentKind::RepositoryPath
        | IntentKind::SameRepositoryGithub
        | IntentKind::SameRepositoryGitlab
        | IntentKind::SameRepositoryGitea
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

#[derive(Clone, Copy)]
enum IdentityValue<'a> {
    Null,
    Integer(i64),
    String(&'a str),
    Digest(Digest),
    Path(&'a RepoPath),
    IntegerArray(&'a [usize]),
    Object(&'a [(&'static str, IdentityValue<'a>)]),
}

fn write_identity_value(sink: &mut dyn Sink, value: &IdentityValue<'_>) {
    match value {
        IdentityValue::Null => sink.write("null"),
        IdentityValue::Integer(integer) => {
            let _infallible = fmt::write(&mut SinkFormatter(sink), format_args!("{integer}"));
        }
        IdentityValue::String(text) => write_string(sink, text),
        IdentityValue::Digest(digest) => {
            sink.write("\"");
            let _infallible = fmt::write(&mut SinkFormatter(sink), format_args!("{digest}"));
            sink.write("\"");
        }
        IdentityValue::Path(path) => write_path(sink, path),
        IdentityValue::IntegerArray(values) => {
            sink.write("[");
            for (position, value) in values.iter().enumerate() {
                if position != 0 {
                    sink.write(",");
                }
                let integer = i64::try_from(*value).unwrap_or(i64::MAX);
                let _infallible = fmt::write(&mut SinkFormatter(sink), format_args!("{integer}"));
            }
            sink.write("]");
        }
        IdentityValue::Object(members) => {
            sink.write("{");
            for (position, (key, value)) in members.iter().enumerate() {
                if position != 0 {
                    sink.write(",");
                }
                write_string(sink, key);
                sink.write(":");
                write_identity_value(sink, value);
            }
            sink.write("}");
        }
    }
}

fn write_path(sink: &mut dyn Sink, path: &RepoPath) {
    if let Some(text) = path.as_str() {
        write_string(sink, text);
        return;
    }
    sink.write("{\"bytes_hex\":\"");
    for byte in path.as_bytes() {
        let pair = [hex_digit(byte.wrapping_shr(4)), hex_digit(byte & 0x0f)];
        sink.write(std::str::from_utf8(&pair).unwrap_or("00"));
    }
    sink.write("\"}");
}

const fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0'.wrapping_add(nibble),
        _ => b'a'.wrapping_add(nibble.wrapping_sub(10)),
    }
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

fn with_observation_value<R>(
    input: &ObservationIdentity<'_>,
    consume: impl FnOnce(IdentityValue<'_>) -> R,
) -> R {
    let intent = [
        (
            "external_scheme",
            external_scheme(input.intent).map_or(IdentityValue::Null, IdentityValue::String),
        ),
        (
            "fragment_digest",
            fragment_digest(input.intent).map_or(IdentityValue::Null, IdentityValue::Digest),
        ),
        ("kind", IdentityValue::String(input.intent.kind.as_str())),
        (
            "query_digest",
            query_digest(input.intent).map_or(IdentityValue::Null, IdentityValue::Digest),
        ),
        (
            "raw_destination_digest",
            IdentityValue::Digest(input.raw_destination_digest),
        ),
        (
            "repository_path",
            input
                .intent
                .repository_path
                .as_ref()
                .map_or(IdentityValue::Null, IdentityValue::Path),
        ),
        (
            "target_kind",
            input
                .intent
                .target_kind
                .map(amiss_wire::controls::TargetKind::as_str)
                .map_or(IdentityValue::Null, IdentityValue::String),
        ),
    ];
    let address = [
        (
            "address_kind",
            IdentityValue::String(input.adapter.structural_address()),
        ),
        ("construct_index", IdentityValue::Integer(0)),
        ("duplicate_index", IdentityValue::Integer(0)),
        ("node_path", IdentityValue::IntegerArray(input.node_path)),
        ("schema", IdentityValue::String(STRUCTURAL_ADDRESS_SCHEMA)),
    ];
    let members = [
        (
            "adapter_contract_digest",
            IdentityValue::Digest(input.contract_digest),
        ),
        (
            "adapter_id",
            IdentityValue::String(input.adapter.adapter_id()),
        ),
        ("document", IdentityValue::Path(input.document)),
        ("extracted_intent", IdentityValue::Object(&intent)),
        ("schema", IdentityValue::String(OBSERVATION_ID_INPUT_SCHEMA)),
        (
            "source_construct",
            IdentityValue::String(input.construct.as_str()),
        ),
        (
            "source_projection_digest",
            IdentityValue::Digest(input.projection_digest),
        ),
        ("structural_address", IdentityValue::Object(&address)),
    ];
    consume(IdentityValue::Object(&members))
}

/// The wire target intent: one flat shape whose null pattern is fixed by the
/// kind, embedding the raw-destination digest and both component digests.
#[must_use]
pub fn intent_value(intent: &Intent, raw_destination_digest: Digest) -> Value {
    Value::object(vec![
        (
            "kind".to_owned(),
            Value::string(intent.kind.as_str().to_owned()),
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
            intent
                .target_kind
                .map_or(Value::Null, |kind| Value::string(kind.as_str().to_owned())),
        ),
        (
            "query_digest".to_owned(),
            query_digest(intent).map_or(Value::Null, |digest| Value::string(digest.to_string())),
        ),
        (
            "fragment_digest".to_owned(),
            fragment_digest(intent).map_or(Value::Null, |digest| Value::string(digest.to_string())),
        ),
        (
            "external_scheme".to_owned(),
            external_scheme(intent).map_or(Value::Null, |scheme| Value::string(scheme.to_owned())),
        ),
    ])
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
            Value::string(adapter.structural_address().to_owned()),
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
            Value::string(input.adapter.adapter_id().to_owned()),
        ),
        (
            "adapter_contract_digest".to_owned(),
            Value::string(input.contract_digest.to_string()),
        ),
        ("document".to_owned(), input.document.to_value()),
        (
            "source_construct".to_owned(),
            Value::string(input.construct.as_str().to_owned()),
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

/// Hashes the borrowed observation input without materializing its JSON tree.
#[must_use]
pub fn observation_digest(input: &ObservationIdentity<'_>) -> Digest {
    with_observation_value(input, |value| {
        hj_stream(OBSERVATION_ID_DOMAIN, |sink| {
            write_identity_value(sink, &value);
        })
    })
}
