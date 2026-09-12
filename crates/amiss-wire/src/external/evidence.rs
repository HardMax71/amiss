use serde::{Deserialize, Serialize};
use strum::EnumString;

use crate::codec;
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;

use super::{EVIDENCE_SCHEMA, destination_valid};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence {
    pub schema: String,
    pub plan_payload_digest: Digest,
    pub producer: Producer,
    pub rows: Vec<EvidenceRow>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Producer {
    pub name: String,
    pub version: String,
}

impl Evidence {
    pub(super) fn check_header(&self) -> Result<(), Error> {
        if self.schema != EVIDENCE_SCHEMA
            || self.producer.name.is_empty()
            || self.producer.version.is_empty()
        {
            return fail("$", ErrorKind::InvalidValue);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
#[serde(remote = "Self")]
pub(super) enum EvidenceRow {
    HttpProbe(Probe),
    ForgeApi(Forge),
}

impl Serialize for EvidenceRow {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for EvidenceRow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::object(deserializer))
    }
}

impl EvidenceRow {
    pub(super) fn destination(&self) -> &str {
        match self {
            Self::HttpProbe(row) => &row.destination,
            Self::ForgeApi(row) => &row.destination,
        }
    }

    pub(super) fn check(&self, root: &str) -> Result<(), Error> {
        let checked_at = match self {
            Self::HttpProbe(row) => {
                let outcome = matches!(
                    (row.status, row.failure),
                    (Some(100..=999), None) | (None, Some(_))
                );
                if !outcome
                    || row
                        .final_destination
                        .as_ref()
                        .is_some_and(|value| !destination_valid(value))
                    || row
                        .redirect_chain_permanent
                        .is_some_and(|permanent| !permanent || row.final_destination.is_none())
                {
                    return fail(root, ErrorKind::InvalidValue);
                }
                &row.checked_at
            }
            Self::ForgeApi(row) => {
                if row.tail.is_some() && row.repository != Repository::Readable {
                    return fail(root, ErrorKind::InvalidValue);
                }
                &row.checked_at
            }
        };
        if !destination_valid(self.destination()) || checked_at.is_empty() {
            return fail(root, ErrorKind::InvalidValue);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Probe {
    pub destination: String,
    pub method: Method,
    #[serde(
        default,
        deserialize_with = "codec::non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<i64>,
    #[serde(
        default,
        deserialize_with = "codec::non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub failure: Option<Failure>,
    #[serde(
        default,
        deserialize_with = "codec::non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub final_destination: Option<String>,
    #[serde(
        default,
        deserialize_with = "codec::non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub redirect_chain_permanent: Option<bool>,
    pub checked_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Forge {
    pub destination: String,
    pub repository: Repository,
    #[serde(
        default,
        deserialize_with = "codec::non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub tail: Option<Tail>,
    pub checked_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[serde(remote = "Self")]
pub(super) enum Method {
    Head,
    Get,
}

impl Serialize for Method {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Method {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::string(deserializer))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[serde(remote = "Self")]
pub(super) enum Failure {
    Dns,
    Tls,
    Timeout,
    Refused,
}

impl Serialize for Failure {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Failure {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::string(deserializer))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[serde(remote = "Self")]
pub(super) enum Repository {
    Readable,
    Missing,
    Denied,
}

impl Serialize for Repository {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Repository {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::string(deserializer))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
#[serde(remote = "Self")]
pub(super) enum Tail {
    Resolved,
    PathMissing,
    RevisionMissing,
}

impl Serialize for Tail {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Tail {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::string(deserializer))
    }
}
