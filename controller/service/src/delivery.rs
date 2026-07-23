use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

use crate::InboxError;
use crate::hash::digest_hex;
use crate::limits::StoredLimits;

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

#[derive(Clone, PartialEq, Eq)]
pub struct DeliveryHeader {
    pub name: String,
    pub value: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Delivery {
    pub route: String,
    pub source_id: String,
    pub received_at_unix_millis: i64,
    pub headers: Vec<DeliveryHeader>,
    pub body: Vec<u8>,
}

#[derive(Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredHeader {
    name: String,
    value_base64: String,
}

#[derive(Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredDelivery {
    route: String,
    source_id: String,
    received_at_unix_millis: i64,
    headers: Vec<StoredHeader>,
    body_base64: String,
}

#[derive(Serialize)]
struct Source<'a> {
    route: &'a str,
    source_id: &'a str,
}

#[derive(Serialize)]
struct Content<'a> {
    route: &'a str,
    source_id: &'a str,
    headers: &'a [StoredHeader],
    body_base64: &'a str,
}

impl StoredDelivery {
    pub(crate) fn read(
        incoming: IncomingDelivery<'_>,
        limits: StoredLimits,
    ) -> Result<Self, InboxError> {
        let delivery = normalize(&incoming, limits)?;
        Ok(Self::from_delivery(&delivery))
    }

    pub(crate) fn materialize(&self, limits: StoredLimits) -> Result<Delivery, InboxError> {
        let headers = self
            .headers
            .iter()
            .map(|header| {
                STANDARD
                    .decode(&header.value_base64)
                    .map(|value| DeliveryHeader {
                        name: header.name.clone(),
                        value,
                    })
                    .map_err(|_| InboxError::Corrupt)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let body = STANDARD
            .decode(&self.body_base64)
            .map_err(|_| InboxError::Corrupt)?;
        let delivery = Delivery {
            route: self.route.clone(),
            source_id: self.source_id.clone(),
            received_at_unix_millis: self.received_at_unix_millis,
            headers,
            body,
        };
        validate_delivery(&delivery, limits).map_err(|_| InboxError::Corrupt)?;
        if Self::from_delivery(&delivery) != *self {
            return Err(InboxError::Corrupt);
        }
        Ok(delivery)
    }

    pub(crate) fn content_digest(&self) -> Result<String, InboxError> {
        let content = Content {
            route: &self.route,
            source_id: &self.source_id,
            headers: &self.headers,
            body_base64: &self.body_base64,
        };
        let bytes = serde_json::to_vec(&content).map_err(|_| InboxError::Corrupt)?;
        Ok(digest_hex(CONTENT_DOMAIN, &bytes))
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

    fn from_delivery(delivery: &Delivery) -> Self {
        let headers = delivery
            .headers
            .iter()
            .map(|header| StoredHeader {
                name: header.name.clone(),
                value_base64: STANDARD.encode(&header.value),
            })
            .collect();
        Self {
            route: delivery.route.clone(),
            source_id: delivery.source_id.clone(),
            received_at_unix_millis: delivery.received_at_unix_millis,
            headers,
            body_base64: STANDARD.encode(&delivery.body),
        }
    }
}

pub(crate) fn source_key(route: &str, source_id: &str) -> Result<String, InboxError> {
    let bytes =
        serde_json::to_vec(&Source { route, source_id }).map_err(|_| InboxError::Corrupt)?;
    Ok(digest_hex(KEY_DOMAIN, &bytes))
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
    if incoming.received_at_unix_millis < 0 {
        return Err(InboxError::InvalidDelivery);
    }
    validate_source(incoming.route, incoming.source_id, limits)?;
    length(incoming.body)?
        .le(&limits.max_body_bytes())
        .then_some(())
        .ok_or(InboxError::InvalidDelivery)?;
    length(incoming.headers)?
        .le(&limits.max_headers())
        .then_some(())
        .ok_or(InboxError::InvalidDelivery)?;

    let mut header_bytes = 0_u64;
    let mut headers = Vec::with_capacity(incoming.headers.len());
    for header in incoming.headers {
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
        headers.push(DeliveryHeader {
            name: header.name.to_ascii_lowercase(),
            value: header.value.to_vec(),
        });
    }
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

fn validate_delivery(delivery: &Delivery, limits: StoredLimits) -> Result<(), InboxError> {
    let incoming_headers = delivery
        .headers
        .iter()
        .map(|header| IncomingHeader {
            name: &header.name,
            value: &header.value,
        })
        .collect::<Vec<_>>();
    let normalized = normalize(
        &IncomingDelivery {
            route: &delivery.route,
            source_id: &delivery.source_id,
            received_at_unix_millis: delivery.received_at_unix_millis,
            headers: &incoming_headers,
            body: &delivery.body,
        },
        limits,
    )?;
    (normalized == *delivery)
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
    u64::try_from(values.len()).map_err(|_| InboxError::InvalidDelivery)
}
