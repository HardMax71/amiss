mod tests;

use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use crate::InboxError;
use crate::limits::StoredLimits;
use sha2::{Digest as _, Sha256};

const CONTENT_DOMAIN: &str = "amiss/controller-inbox-content-v1";
const KEY_DOMAIN: &str = "amiss/controller-inbox-source-v1";

#[derive(Clone, Copy)]
pub struct IncomingHeader<'a> {
    pub name: &'a str,
    pub value: &'a [u8],
}

#[derive(Clone, Copy)]
pub struct IncomingDelivery<'a> {
    pub route: &'a str,
    pub source_id: &'a str,
    pub received_at_unix_millis: i64,
    pub headers: &'a [IncomingHeader<'a>],
    pub body: &'a [u8],
}

#[serde_as]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryHeader {
    pub name: String,
    #[serde(rename = "value_base64")]
    #[serde_as(as = "Base64")]
    pub value: Vec<u8>,
}

#[serde_as]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub route: String,
    pub source_id: String,
    pub received_at_unix_millis: i64,
    pub headers: Vec<DeliveryHeader>,
    #[serde(rename = "body_base64")]
    #[serde_as(as = "Base64")]
    pub body: Vec<u8>,
}

#[derive(Serialize)]
struct Source<'a> {
    route: &'a str,
    source_id: &'a str,
}

#[serde_as]
#[derive(Serialize)]
struct Content<'a> {
    route: &'a str,
    source_id: &'a str,
    headers: &'a [DeliveryHeader],
    #[serde(rename = "body_base64")]
    #[serde_as(as = "Base64")]
    body: &'a [u8],
}

impl Delivery {
    pub(crate) fn read(
        incoming: IncomingDelivery<'_>,
        limits: StoredLimits,
    ) -> Result<Self, InboxError> {
        normalize(&incoming, limits)
    }

    pub(crate) fn validate(&self, limits: StoredLimits) -> Result<(), InboxError> {
        validate_delivery(self, limits).map_err(|_defect| InboxError::Corrupt)
    }

    pub(crate) fn content_digest(&self) -> Result<[u8; 32], InboxError> {
        let content = Content {
            route: &self.route,
            source_id: &self.source_id,
            headers: &self.headers,
            body: &self.body,
        };
        let mut writer =
            digest_io::IoWrapper(Sha256::new_with_prefix(CONTENT_DOMAIN).chain_update([0_u8]));
        serde_json::to_writer(&mut writer, &content).map_err(|_defect| InboxError::Corrupt)?;
        Ok(writer.0.finalize().0)
    }

    pub(crate) fn key(&self) -> Result<String, InboxError> {
        source_key(&self.route, &self.source_id)
    }

    pub(crate) fn route(&self) -> &str {
        &self.route
    }

    pub(crate) fn source_id(&self) -> &str {
        &self.source_id
    }
}

pub(crate) fn source_key(route: &str, source_id: &str) -> Result<String, InboxError> {
    let mut writer = digest_io::IoWrapper(Sha256::new_with_prefix(KEY_DOMAIN).chain_update([0_u8]));
    serde_json::to_writer(&mut writer, &Source { route, source_id })
        .map_err(|_defect| InboxError::Corrupt)?;
    Ok(hex::encode(writer.0.finalize()))
}

pub(crate) fn validate_source(
    route: &str,
    source_id: &str,
    limits: StoredLimits,
) -> Result<(), InboxError> {
    validate_label(route, limits.max_route_bytes())?;
    validate_label(source_id, limits.max_source_id_bytes())
}

fn normalize(
    incoming: &IncomingDelivery<'_>,
    limits: StoredLimits,
) -> Result<Delivery, InboxError> {
    validate_envelope(
        incoming.route,
        incoming.source_id,
        incoming.received_at_unix_millis,
        incoming.body,
        limits,
    )?;
    validate_headers(incoming.headers.iter().copied(), limits)?;
    let mut headers = incoming
        .headers
        .iter()
        .map(|header| DeliveryHeader {
            name: header.name.to_ascii_lowercase(),
            value: header.value.to_vec(),
        })
        .collect::<Vec<_>>();
    headers.sort_unstable_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.value.cmp(&right.value))
    });
    Ok(Delivery {
        route: incoming.route.to_owned(),
        source_id: incoming.source_id.to_owned(),
        received_at_unix_millis: incoming.received_at_unix_millis,
        headers,
        body: incoming.body.to_vec(),
    })
}

fn validate_envelope(
    route: &str,
    source_id: &str,
    received_at_unix_millis: i64,
    body: &[u8],
    limits: StoredLimits,
) -> Result<(), InboxError> {
    if received_at_unix_millis < 0 {
        return Err(InboxError::InvalidDelivery);
    }
    validate_source(route, source_id, limits)?;
    length(body)?
        .le(&limits.max_body_bytes())
        .then_some(())
        .ok_or(InboxError::InvalidDelivery)
}

fn validate_headers<'a>(
    headers: impl ExactSizeIterator<Item = IncomingHeader<'a>>,
    limits: StoredLimits,
) -> Result<(), InboxError> {
    u64::try_from(headers.len())
        .map_err(|_defect| InboxError::InvalidDelivery)?
        .le(&limits.max_headers())
        .then_some(())
        .ok_or(InboxError::InvalidDelivery)?;
    let mut header_bytes = 0_u64;
    for header in headers {
        if !valid_header_name(header.name) || !valid_header_value(header.value) {
            return Err(InboxError::InvalidDelivery);
        }
        header_bytes = header_bytes
            .checked_add(length(header.name.as_bytes())?)
            .and_then(|bytes| bytes.checked_add(length(header.value).ok()?))
            .ok_or(InboxError::InvalidDelivery)?;
        if header_bytes > limits.max_header_bytes() {
            return Err(InboxError::InvalidDelivery);
        }
    }
    Ok(())
}

fn validate_delivery(delivery: &Delivery, limits: StoredLimits) -> Result<(), InboxError> {
    validate_envelope(
        &delivery.route,
        &delivery.source_id,
        delivery.received_at_unix_millis,
        &delivery.body,
        limits,
    )?;
    validate_headers(
        delivery.headers.iter().map(|header| IncomingHeader {
            name: &header.name,
            value: &header.value,
        }),
        limits,
    )?;
    (delivery
        .headers
        .iter()
        .all(|header| !header.name.bytes().any(|byte| byte.is_ascii_uppercase()))
        && delivery
            .headers
            .is_sorted_by(|left, right| (&left.name, &left.value) <= (&right.name, &right.value)))
    .then_some(())
    .ok_or(InboxError::InvalidDelivery)
}

fn validate_label(value: &str, maximum: u64) -> Result<(), InboxError> {
    let valid = !value.is_empty()
        && length(value.as_bytes())? <= maximum
        && value.bytes().all(|byte| matches!(byte, 0x21..=0x7e));
    valid.then_some(()).ok_or(InboxError::InvalidDelivery)
}

fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn valid_header_value(value: &[u8]) -> bool {
    !value
        .iter()
        .any(|byte| matches!(byte, b'\0' | b'\r' | b'\n'))
}

fn length<T>(values: &[T]) -> Result<u64, InboxError> {
    u64::try_from(values.len()).map_err(|_defect| InboxError::InvalidDelivery)
}
