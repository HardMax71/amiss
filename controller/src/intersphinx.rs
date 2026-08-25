use std::io::Cursor;
use std::sync::Arc;

use amiss_wire::digest::{hb, hj};
use amiss_wire::json::Value;
use amiss_wire::model::ArtifactId;
use flate2::{Decompress, FlushDecompress, Status};
use sphinx_inv::{SphinxInventoryReader, SphinxType, StdRole};
use url::Url;

use crate::bootstrap_job::SemanticEvidenceTemplate;

pub const INTERSPHINX_INVENTORY_BYTES: u64 = 16_777_216;
const INTERSPHINX_DECODED_BYTES: u64 = 16_777_216;
const INTERSPHINX_HEADER_BYTES: usize = 4_096;
const INTERSPHINX_INVENTORIES: usize = 64;
const LABEL_BYTES: usize = 4_096;
const DESTINATION_BYTES: usize = 16_384;
const INPUT_DOMAIN: &str = "amiss/controller-intersphinx-input-v1";
const SOURCE_DOMAIN: &str = "amiss/controller-intersphinx-source-v1";

pub struct IntersphinxInventory {
    pub identity: String,
    pub base_url: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum IntersphinxError {
    #[error("the Intersphinx inventory set is invalid")]
    InventorySet,
    #[error("an Intersphinx inventory identity is invalid")]
    Identity,
    #[error("an Intersphinx base URL is invalid")]
    BaseUrl,
    #[error("an Intersphinx inventory exceeds its byte ceiling")]
    InventoryBytes,
    #[error("an Intersphinx inventory is not a bounded zlib inventory")]
    Decode(#[source] std::io::Error),
    #[error("an Intersphinx inventory cannot be parsed")]
    Parse(#[source] sphinx_inv::SphinxInvError),
    #[error("an Intersphinx label is invalid")]
    Label,
    #[error("an Intersphinx destination is invalid")]
    Destination,
    #[error("the Intersphinx evidence exceeds the semantic wire contract")]
    Evidence,
}

/// Produces one candidate-independent complete label table from a bounded
/// set of operator-owned Sphinx inventories.
///
/// # Errors
///
/// An identity, base URL, inventory body, parsed label, or destination is
/// invalid, incomplete, duplicated, or outside its ceiling.
pub fn intersphinx_evidence(
    mut inventories: Vec<IntersphinxInventory>,
) -> Result<Vec<SemanticEvidenceTemplate>, IntersphinxError> {
    if inventories.is_empty() {
        return Ok(Vec::new());
    }
    if inventories.len() > INTERSPHINX_INVENTORIES {
        return Err(IntersphinxError::InventorySet);
    }
    let source_bytes = inventories.iter().try_fold(0_u64, |total, inventory| {
        total.checked_add(u64::try_from(inventory.bytes.len()).unwrap_or(u64::MAX))
    });
    if source_bytes.is_none_or(|total| total > INTERSPHINX_INVENTORY_BYTES) {
        return Err(IntersphinxError::InventoryBytes);
    }
    inventories.sort_by(|left, right| left.identity.cmp(&right.identity));
    if inventories
        .windows(2)
        .any(|pair| matches!(pair, [left, right] if left.identity == right.identity))
    {
        return Err(IntersphinxError::InventorySet);
    }

    let mut observations = Vec::new();
    let mut inputs = Vec::with_capacity(inventories.len());
    let mut decoded_bytes = 0_u64;
    for inventory in inventories {
        let identity = ArtifactId::new(inventory.identity).ok_or(IntersphinxError::Identity)?;
        let base_url = base_url(&inventory.base_url)?;
        let (labels, decoded) = labels(
            &identity,
            &base_url,
            &inventory.bytes,
            INTERSPHINX_DECODED_BYTES.saturating_sub(decoded_bytes),
        )?;
        decoded_bytes = decoded_bytes
            .checked_add(decoded)
            .ok_or(IntersphinxError::InventoryBytes)?;
        observations.extend(labels);
        if observations.len() > amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT {
            return Err(IntersphinxError::Evidence);
        }
        inputs.push(Value::object(vec![
            (
                "inventory".to_owned(),
                Value::string(identity.as_str().to_owned()),
            ),
            (
                "base_url".to_owned(),
                Value::string(base_url.as_str().to_owned()),
            ),
            (
                "source_digest".to_owned(),
                Value::string(hb(SOURCE_DOMAIN, &inventory.bytes).to_string()),
            ),
        ]));
    }

    let input_digest = hj(INPUT_DOMAIN, &Value::array(inputs));
    let evidence = vec![SemanticEvidenceTemplate {
        producer_kind: ArtifactId::new("sphinx-inventory-set".to_owned())
            .ok_or(IntersphinxError::Identity)?,
        producer_identity: ArtifactId::new("amiss-controller-intersphinx".to_owned())
            .ok_or(IntersphinxError::Identity)?,
        producer_version: "1".to_owned(),
        context_digest: input_digest,
        input_digest,
        complete: true,
        observations: Arc::from(observations),
    }];
    crate::bind_semantic_evidence(&evidence, &[], &[], input_digest)
        .map_err(|_defect| IntersphinxError::Evidence)?;
    Ok(evidence)
}

fn base_url(raw: &str) -> Result<Url, IntersphinxError> {
    let mut base = Url::parse(raw).map_err(|_defect| IntersphinxError::BaseUrl)?;
    let valid = matches!(base.scheme(), "http" | "https")
        && base.username().is_empty()
        && base.password().is_none()
        && base.query().is_none()
        && base.fragment().is_none()
        && amiss_wire::uri::http_destination_valid(base.as_str());
    if !valid {
        return Err(IntersphinxError::BaseUrl);
    }
    if !base.path().ends_with('/') {
        let mut directory = base.as_str().to_owned();
        directory.push('/');
        base = Url::parse(&directory).map_err(|_defect| IntersphinxError::BaseUrl)?;
    }
    Ok(base)
}

fn labels(
    identity: &ArtifactId,
    base_url: &Url,
    bytes: &[u8],
    decoded_limit: u64,
) -> Result<(Vec<Value>, u64), IntersphinxError> {
    let (plain, decoded_bytes) = bounded_plain_inventory(bytes, decoded_limit)?;
    let reader =
        SphinxInventoryReader::from_reader(Cursor::new(plain)).map_err(IntersphinxError::Parse)?;
    let labels = reader
        .filter_map(|reference| match reference {
            Ok(reference) if matches!(reference.sphinx_type, SphinxType::Std(StdRole::Label)) => {
                Some(label(identity, base_url, reference))
            }
            Ok(_reference) => None,
            Err(defect) => Some(Err(IntersphinxError::Parse(defect))),
        })
        .collect::<Result<Vec<_>, IntersphinxError>>()?;
    Ok((labels, decoded_bytes))
}

fn label(
    identity: &ArtifactId,
    base_url: &Url,
    reference: sphinx_inv::SphinxReference,
) -> Result<Value, IntersphinxError> {
    let location = reference.expanded_location();
    let name = reference.name;
    if name.len() > LABEL_BYTES
        || name.chars().any(char::is_control)
        || amiss_rst::normalized_label(&name).is_empty()
    {
        return Err(IntersphinxError::Label);
    }
    let destination = base_url
        .join(&location)
        .map_err(|_defect| IntersphinxError::Destination)?;
    let destination = destination.as_str();
    if destination.len() > DESTINATION_BYTES
        || !destination.starts_with(base_url.as_str())
        || !amiss_wire::uri::http_destination_valid(destination)
    {
        return Err(IntersphinxError::Destination);
    }
    Ok(Value::object(vec![
        ("kind".to_owned(), Value::string("sphinx-label".to_owned())),
        (
            "inventory".to_owned(),
            Value::string(identity.as_str().to_owned()),
        ),
        ("name".to_owned(), Value::string(name)),
        (
            "destination".to_owned(),
            Value::string(destination.to_owned()),
        ),
    ]))
}

fn bounded_plain_inventory(
    bytes: &[u8],
    decoded_limit: u64,
) -> Result<(Vec<u8>, u64), IntersphinxError> {
    let mut lines = bytes.split_inclusive(|byte| *byte == b'\n');
    let first = lines.next().ok_or(IntersphinxError::InventoryBytes)?;
    let second = lines.next().ok_or(IntersphinxError::InventoryBytes)?;
    let third = lines.next().ok_or(IntersphinxError::InventoryBytes)?;
    let fourth = lines.next().ok_or(IntersphinxError::InventoryBytes)?;
    let header_bytes = [first, second, third, fourth]
        .into_iter()
        .try_fold(0_usize, |total, line| total.checked_add(line.len()))
        .ok_or(IntersphinxError::InventoryBytes)?;
    if header_bytes > INTERSPHINX_HEADER_BYTES
        || !std::str::from_utf8(fourth).is_ok_and(|line| line.contains("zlib"))
    {
        return Err(IntersphinxError::InventoryBytes);
    }
    let body = bytes
        .get(header_bytes..)
        .ok_or(IntersphinxError::InventoryBytes)?;
    let mut inflater = Decompress::new(true);
    let mut body_bytes = Vec::new();
    let mut output = [0_u8; 8_192];
    loop {
        let consumed = usize::try_from(inflater.total_in()).unwrap_or(usize::MAX);
        let remaining = body
            .get(consumed..)
            .ok_or(IntersphinxError::InventoryBytes)?;
        let before_in = inflater.total_in();
        let before_out = inflater.total_out();
        let status = inflater
            .decompress(
                remaining,
                &mut output,
                if remaining.is_empty() {
                    FlushDecompress::Finish
                } else {
                    FlushDecompress::None
                },
            )
            .map_err(|defect| IntersphinxError::Decode(defect.into()))?;
        let produced = inflater
            .total_out()
            .checked_sub(before_out)
            .and_then(|length| usize::try_from(length).ok())
            .and_then(|length| output.get(..length))
            .ok_or(IntersphinxError::InventoryBytes)?;
        if inflater.total_out() > decoded_limit {
            return Err(IntersphinxError::InventoryBytes);
        }
        body_bytes.extend_from_slice(produced);
        if status == Status::StreamEnd {
            if inflater.total_in() != u64::try_from(body.len()).unwrap_or(u64::MAX) {
                return Err(IntersphinxError::InventoryBytes);
            }
            break;
        }
        if inflater.total_in() == before_in && inflater.total_out() == before_out {
            return Err(IntersphinxError::InventoryBytes);
        }
    }
    let decoded_length = inflater.total_out();
    let body_text = String::from_utf8(body_bytes).map_err(|defect| {
        IntersphinxError::Decode(std::io::Error::new(std::io::ErrorKind::InvalidData, defect))
    })?;
    let mut plain = Vec::with_capacity(header_bytes.saturating_add(body_text.len()));
    plain.extend_from_slice(first);
    plain.extend_from_slice(second);
    plain.extend_from_slice(third);
    plain.extend_from_slice(b"# The remainder of this file is compressed using plain-text.\n");
    for line in body_text.lines().filter(|line| {
        line.split_ascii_whitespace()
            .any(|word| word == "std:label")
    }) {
        plain.extend_from_slice(line.as_bytes());
        plain.push(b'\n');
    }
    Ok((plain, decoded_length))
}
