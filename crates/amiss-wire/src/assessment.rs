use garde::Validate;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use crate::codec::rule;
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::semantic::{PRODUCER_VERSION_BYTES, producer_version_valid};

/// The evaluator identity every assessment binds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct EngineBinding {
    #[garde(
        length(bytes, min = 1, max = PRODUCER_VERSION_BYTES),
        custom(producer_version)
    )]
    pub engine_version: String,
    pub engine_digest: Digest,
}

/// The report, plan, and optional evidence digests one assessment judges.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectBinding {
    pub report_payload_digest: Digest,
    pub plan_payload_digest: Digest,
    #[serde(deserialize_with = "crate::codec::nullable")]
    pub evidence_payload_digest: Option<Digest>,
}

fn producer_version<C>(value: &str, _context: &C) -> garde::Result {
    rule(
        producer_version_valid(value),
        "engine version must be a bounded producer version",
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString, Serialize, Deserialize)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AssessmentVerdict {
    Matched,
    Refuted,
    Unproven,
}

pub(crate) fn ordered<T, K: Ord + ?Sized>(
    path: &str,
    values: &[T],
    limit: usize,
    key: impl Fn(&T) -> &K,
) -> Result<(), Error> {
    if values.len() > limit {
        return fail(path, ErrorKind::LimitExceeded);
    }
    for pair in values.windows(2) {
        if let [left, right] = pair {
            match key(left).cmp(key(right)) {
                std::cmp::Ordering::Equal => return fail(path, ErrorKind::DuplicateMember),
                std::cmp::Ordering::Greater => return fail(path, ErrorKind::UnsortedSet),
                std::cmp::Ordering::Less => {}
            }
        }
    }
    Ok(())
}
