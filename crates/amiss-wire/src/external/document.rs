use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::Map;

use crate::assessment::ordered;
use crate::codec::{self, Document, Schema};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::ForgeDialect;

use super::{PLAN_ENVELOPE_SCHEMA, PLAN_PAYLOAD_SCHEMA};

#[derive(Debug, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub(super) struct Plan {
    pub schema: Schema<Self>,
    #[garde(dive)]
    pub engine: Engine,
    #[garde(dive)]
    pub report: ReportBinding,
    #[garde(dive)]
    pub introduced: Vec<DestinationRow>,
    #[garde(dive)]
    pub removed: Vec<DestinationRow>,
    #[garde(range(max = codec::MAX_SAFE_INTEGER))]
    pub retained_count: u64,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub(super) struct Engine {
    #[garde(length(chars, min = 1))]
    pub engine_version: String,
    pub engine_digest: Digest,
}

impl Engine {
    pub(super) fn new(version: &str, digest: &str) -> Result<Self, Error> {
        if version.is_empty() {
            return fail("$.engine.engine_version", ErrorKind::InvalidValue);
        }
        Ok(Self {
            engine_version: version.to_owned(),
            engine_digest: Digest::from_wire(digest)
                .ok_or_else(|| Error::new("$.engine.engine_digest", ErrorKind::InvalidValue))?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Validate)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub(super) struct ReportBinding {
    pub payload_digest: Digest,
    pub base: Map<String, Value>,
    pub candidate: Map<String, Value>,
    #[garde(length(chars, min = 1))]
    pub mode: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub(super) struct DestinationRow {
    #[garde(length(chars, min = 1, max = 16_384))]
    pub destination: String,
    #[garde(custom(valid_scheme))]
    pub scheme: String,
    #[garde(
        length(min = 1),
        inner(length(chars, min = 1)),
        custom(sorted_documents)
    )]
    pub documents: Vec<String>,
    #[serde(
        default,
        deserialize_with = "codec::non_null",
        skip_serializing_if = "Option::is_none"
    )]
    #[garde(dive)]
    pub repository: Option<RepositoryShape>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub(super) struct RepositoryShape {
    pub dialect: ForgeDialect,
    #[garde(length(chars, min = 1))]
    pub host: String,
    #[garde(length(chars, min = 1))]
    pub owner: String,
    #[garde(length(chars, min = 1))]
    pub name: String,
    #[serde(
        default,
        deserialize_with = "codec::non_null",
        skip_serializing_if = "Option::is_none"
    )]
    #[garde(inner(length(chars, min = 1)))]
    pub form: Option<String>,
    #[serde(
        default,
        deserialize_with = "codec::non_null",
        skip_serializing_if = "Option::is_none"
    )]
    #[garde(inner(length(chars, min = 1)))]
    pub tail: Option<String>,
}

impl Document for Plan {
    const PAYLOAD_SCHEMA: &'static str = PLAN_PAYLOAD_SCHEMA;
    const ENVELOPE_SCHEMA: &'static str = PLAN_ENVELOPE_SCHEMA;
    const LIMIT: u64 = u64::MAX;

    fn check(&self, root: &str) -> Result<(), Error> {
        for (name, rows) in [("introduced", &self.introduced), ("removed", &self.removed)] {
            ordered(&format!("{root}.{name}"), rows, usize::MAX, |row| {
                &row.destination
            })?;
        }
        Ok(())
    }
}

fn valid_scheme<C>(scheme: &str, _context: &C) -> garde::Result {
    let mut bytes = scheme.bytes();
    codec::rule(
        bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
            && bytes.all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'+' | b'-' | b'.')
            }),
        "invalid external scheme",
    )
}

fn sorted_documents<C>(documents: &[String], _context: &C) -> garde::Result {
    ordered("$", documents, usize::MAX, |document| document)
        .map_err(|defect| garde::Error::new(defect.kind.to_string()))
}
