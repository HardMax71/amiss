use std::cmp::Ordering;
use std::str::FromStr;

use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::{self, Value};
use crate::model::{
    ArtifactId, BranchRef, OwnerId, RepoPathText, RepositoryIdentity, TreeIdentity, UtcInstant,
};
use crate::resolution::{
    BlobContent, BlobContentTag, BlobMode, BlobTarget, Missing, MissingTag, Resolution,
    ResolutionTag, Target, TargetTag,
};

mod debt;
/// Execution-constraint descriptor, forge-neutral action-repository
/// identity, and closed platform grammar.
mod execution_constraint;
mod fact;
mod floor;
mod policy;
mod resources;
mod taxonomy;
/// Trusted-time statement grammar, digest, and bounded-lifetime parser.
mod trusted_time;
pub(crate) mod value;
mod waiver;

pub use debt::{DebtItem, DebtSnapshot};
pub use execution_constraint::{
    ConstraintPlatform, ExecutionConstraintDescriptor, ExecutionConstraintInput,
    valid_required_status_name,
};
pub use fact::{Fact, FindingKeyInput, FindingScope, TargetIntent};
pub use floor::{
    FloorDefect, FloorDisposition, ORGANIZATION_POLICY_ENTRIES_LIMIT, OrganizationFloor,
    ResourceLimit,
};
pub use policy::{DocumentInclude, FindingDisposition, ScannerPolicy};
pub use resources::{ResourceName, ResourceNameIter};
pub use taxonomy::{
    ContentAvailability, Disposition, EligibleFindingKind, EntryKind, GitMode, IncludeKind,
    Profile, PromotableFindingKind, SourceConstruct, TargetKind,
};
pub use trusted_time::{STATEMENT_TTL_MAX_SECONDS, TrustedTimeInput, TrustedTimeStatement};
pub use waiver::{WaiverBundle, WaiverItem};

pub const SCANNER_POLICY_PATH: &str = ".amiss/scanner-policy.json";

const SCANNER_POLICY_SCHEMA: &str = "amiss/scanner-policy";
const ORGANIZATION_FLOOR_SCHEMA: &str = "amiss/organization-floor";
const DEBT_SNAPSHOT_SCHEMA: &str = "amiss/debt-snapshot";
const WAIVER_BUNDLE_SCHEMA: &str = "amiss/waiver-bundle";

const FINDING_KEY_INPUT_SCHEMA: &str = "amiss/scanner-finding-key-input";
const FACT_SCHEMA: &str = "amiss/scanner-fact";
pub const FINDING_KEY_DOMAIN: &str = "amiss/scanner-finding-key";
pub const FACT_DOMAIN: &str = "amiss/scanner-fact";

pub(crate) fn decode_enum<T: FromStr>(path: &str, value: Value) -> Result<T, Error> {
    let raw = de::string(path, value)?;
    raw.parse()
        .map_err(|_unknown| Error::new(path, ErrorKind::InvalidValue))
}

/// The one restricted-JSON root every control document parses through.
///
/// # Errors
///
/// Any strict-JSON defect, carried as `ErrorKind::Json`.
pub fn root(bytes: &[u8]) -> Result<Value, Error> {
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))
}

fn decode_items<T>(
    path: &str,
    raw: Vec<Value>,
    limit: usize,
    decode: impl Fn(&str, Value) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    if raw.len() > limit {
        return fail(path, ErrorKind::LimitExceeded);
    }
    raw.into_iter()
        .enumerate()
        .map(|(index, value)| decode(&format!("{path}[{index}]"), value))
        .collect()
}

fn sorted_set<T>(
    path: &str,
    items: &[T],
    compare: impl Fn(&T, &T) -> Ordering,
) -> Result<(), Error> {
    for pair in items.windows(2) {
        if let [left, right] = pair {
            match compare(left, right) {
                Ordering::Less => {}
                Ordering::Equal => return fail(path, ErrorKind::DuplicateMember),
                Ordering::Greater => return fail(path, ErrorKind::UnsortedSet),
            }
        }
    }
    Ok(())
}

fn decode_include(path: &str, value: Value) -> Result<DocumentInclude, Error> {
    let mut obj = Obj::new(path, value)?;
    let include_path = obj.required("path", decode_repo_path)?;
    let kind = obj.required("kind", decode_enum)?;
    let adapter = obj
        .take_optional("adapter")
        .map(|value| decode_enum(&obj.field("adapter"), value))
        .transpose()?;
    obj.finish()?;
    Ok(DocumentInclude {
        path: include_path,
        kind,
        adapter,
    })
}

fn decode_disposition_rule(path: &str, value: Value) -> Result<FindingDisposition, Error> {
    let mut obj = Obj::new(path, value)?;
    let finding_kind = obj.required("finding_kind", decode_enum)?;
    let disposition = obj.required("disposition", decode_enum)?;
    obj.finish()?;
    Ok(FindingDisposition {
        finding_kind,
        disposition,
    })
}

fn decode_resource_limit(path: &str, value: Value) -> Result<ResourceLimit, Error> {
    let mut obj = Obj::new(path, value)?;
    let resource = obj.required("resource", ResourceName::decode)?;
    let maximum_path = obj.field("maximum");
    let maximum = de::integer(&maximum_path, obj.take("maximum")?)?;
    obj.finish()?;
    if in_bounds(resource, maximum) {
        Ok(ResourceLimit { resource, maximum })
    } else {
        fail(&maximum_path, ErrorKind::InvalidValue)
    }
}

/// Two resources fix their own maximum: the retained-error count is a small
/// range, and the report reservation may be declared but never moved.
fn in_bounds(resource: ResourceName, maximum: i64) -> bool {
    if resource == ResourceName::TypedAnalysisErrorsRetained {
        (1..=64).contains(&maximum)
    } else if resource == ResourceName::MachineJsonBytes {
        u64::try_from(maximum).is_ok_and(|value| value == crate::report::MACHINE_JSON_BYTES)
    } else {
        maximum >= 0
    }
}

fn decode_path_set(path: &str, value: Value) -> Result<Vec<RepoPathText>, Error> {
    decode_path_items(path, de::array(path, value)?)
}

fn decode_path_items(path: &str, raw: Vec<Value>) -> Result<Vec<RepoPathText>, Error> {
    let paths = decode_items(path, raw, 100_000, decode_repo_path)?;
    sorted_set(path, &paths, |a, b| a.as_str().cmp(b.as_str()))?;
    Ok(paths)
}

fn decode_owner_items(path: &str, raw: Vec<Value>) -> Result<Vec<OwnerId>, Error> {
    let owners = decode_items(path, raw, 10_000, decode_owner)?;
    sorted_set(path, &owners, |a, b| a.as_str().cmp(b.as_str()))?;
    Ok(owners)
}

fn decode_repo_path(path: &str, value: Value) -> Result<RepoPathText, Error> {
    RepoPathText::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_artifact_id(path: &str, value: Value) -> Result<ArtifactId, Error> {
    ArtifactId::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_owner(path: &str, value: Value) -> Result<OwnerId, Error> {
    OwnerId::new(de::string(path, value)?).ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_branch_ref(path: &str, value: Value) -> Result<BranchRef, Error> {
    BranchRef::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_instant(path: &str, value: Value) -> Result<UtcInstant, Error> {
    UtcInstant::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_digest(path: &str, value: Value) -> Result<Digest, Error> {
    let raw = de::string(path, value)?;
    Digest::from_wire(&raw).ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_nullable_digest(path: &str, value: Value) -> Result<Option<Digest>, Error> {
    de::nullable(value)
        .map(|v| decode_digest(path, v))
        .transpose()
}

pub(crate) fn decode_repository(path: &str, value: Value) -> Result<RepositoryIdentity, Error> {
    let mut obj = Obj::new(path, value)?;
    let host = obj.required("host", de::string)?;
    let owner = obj.required("owner", de::string)?;
    let name = obj.required("name", de::string)?;
    obj.finish()?;
    RepositoryIdentity::new(host, owner, name)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

pub(crate) fn decode_provider_run_id(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    let bytes = raw.as_bytes();
    let allowed = |byte: &u8| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
    };
    if bytes.is_empty()
        || bytes.len() > 128
        || !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.iter().all(allowed)
    {
        return fail(path, ErrorKind::InvalidValue);
    }
    Ok(raw)
}

pub(crate) fn decode_provider_id(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    if ArtifactId::new(raw.clone()).is_some() {
        Ok(raw)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn decode_tree(path: &str, value: Value) -> Result<TreeIdentity, Error> {
    let mut obj = Obj::new(path, value)?;
    let object_format = obj.required("object_format", decode_enum)?;
    let tree_oid = obj.required("tree_oid", de::string)?;
    obj.finish()?;
    TreeIdentity::new(object_format, tree_oid)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_reason(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    let length = raw.chars().count();
    if (1..=1024).contains(&length) && raw.chars().any(|c| !c.is_whitespace()) {
        Ok(raw)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn decode_intent(path: &str, value: Value) -> Result<TargetIntent, Error> {
    let mut obj = Obj::new(path, value)?;
    obj.required("kind", |path, value| {
        de::const_str(path, value, "repository-path")
    })?;
    let target_path = obj.required("path", decode_repo_path)?;
    let target_kind = obj.required("target_kind", decode_enum)?;
    let query_digest = obj.required("query_digest", decode_nullable_digest)?;
    let fragment_digest = obj.required("fragment_digest", decode_nullable_digest)?;
    obj.finish()?;
    Ok(TargetIntent {
        path: target_path,
        target_kind,
        query_digest,
        fragment_digest,
    })
}

fn decode_scope(path: &str, value: Value) -> Result<FindingScope, Error> {
    let mut obj = Obj::new(path, value)?;
    obj.required("kind", |path, value| {
        de::const_str(path, value, "reference")
    })?;
    let document = obj.required("document", decode_repo_path)?;
    let source_construct = obj.required("source_construct", decode_enum)?;
    let normalized_target_intent = obj.required("normalized_target_intent", decode_intent)?;
    let occurrence_path = obj.field("occurrence");
    let mut occurrence = Obj::new(&occurrence_path, obj.take("occurrence")?)?;
    occurrence.required("kind", |path, value| {
        de::const_str(path, value, "source-projection")
    })?;
    let source_projection_digest =
        occurrence.required("source_projection_digest", decode_digest)?;
    occurrence.finish()?;
    obj.finish()?;
    Ok(FindingScope {
        document,
        source_construct,
        normalized_target_intent,
        source_projection_digest,
    })
}

fn decode_key_input(path: &str, value: Value) -> Result<(FindingKeyInput, Digest), Error> {
    let digest = hj(FINDING_KEY_DOMAIN, &value);
    let mut obj = Obj::new(path, value)?;
    obj.required("schema", |path, value| {
        de::const_str(path, value, FINDING_KEY_INPUT_SCHEMA)
    })?;
    let finding_kind = obj.required("finding_kind", decode_enum)?;
    let scope = obj.required("scope", decode_scope)?;
    obj.finish()?;
    Ok((
        FindingKeyInput {
            finding_kind,
            scope,
        },
        digest,
    ))
}

fn decode_resolution(path: &str, value: Value) -> Result<Resolution<RepoPathText>, Error> {
    let mut obj = Obj::new(path, value)?;
    let kind_path = obj.field("kind");
    let kind_text = de::string(&kind_path, obj.take("kind")?)?;
    let Ok(kind) = kind_text.parse::<ResolutionTag>() else {
        return fail(&kind_path, ErrorKind::InvalidValue);
    };
    match kind {
        ResolutionTag::Missing => {
            let reason_path = obj.field("reason");
            let reason_text = de::string(&reason_path, obj.take("reason")?)?;
            let Ok(reason) = reason_text.parse::<MissingTag>() else {
                return fail(&reason_path, ErrorKind::InvalidValue);
            };
            if matches!(reason, MissingTag::LabelNotDeclared) {
                obj.finish()?;
                return Ok(Resolution::Missing(Missing::LabelNotDeclared));
            }
            let resolved_path = obj.required("path", decode_repo_path)?;
            let missing = match reason {
                MissingTag::PathNotFound => Missing::PathNotFound {
                    path: resolved_path,
                    near: obj.required("near", |path, value| {
                        de::nullable(value)
                            .map(|value| decode_repo_path(path, value))
                            .transpose()
                    })?,
                },
                MissingTag::LineFragmentOutOfRange => Missing::LineFragmentOutOfRange {
                    path: resolved_path,
                },
                MissingTag::HeadingAnchorNotFound => Missing::HeadingAnchorNotFound {
                    path: resolved_path,
                    near: obj.required("near", |path, value| {
                        de::nullable(value)
                            .map(|value| de::string(path, value))
                            .transpose()
                    })?,
                },
                MissingTag::LabelNotDeclared => Missing::LabelNotDeclared,
            };
            obj.finish()?;
            Ok(Resolution::Missing(missing))
        }
        ResolutionTag::TypeMismatch => {
            let target = obj.required("target", decode_resolution_target)?;
            obj.finish()?;
            Ok(Resolution::TypeMismatch(target))
        }
        ResolutionTag::Resolved
        | ResolutionTag::DeclaredUntracked
        | ResolutionTag::UnsupportedTarget
        | ResolutionTag::UnsupportedSemantics
        | ResolutionTag::UnsupportedVersion
        | ResolutionTag::Invalid
        | ResolutionTag::External => fail(&kind_path, ErrorKind::InvalidValue),
    }
}

fn decode_resolution_target(path: &str, value: Value) -> Result<Target<RepoPathText>, Error> {
    let mut obj = Obj::new(path, value)?;
    let kind_path = obj.field("kind");
    let kind_text = de::string(&kind_path, obj.take("kind")?)?;
    let Ok(kind) = kind_text.parse::<TargetTag>() else {
        return fail(&kind_path, ErrorKind::InvalidValue);
    };
    let resolved_path = obj.required("path", decode_repo_path)?;
    match kind {
        TargetTag::Tree => {
            obj.finish()?;
            Ok(Target::Tree {
                path: resolved_path,
            })
        }
        TargetTag::Blob => {
            let mode_path = obj.field("mode");
            let mode_text = de::string(&mode_path, obj.take("mode")?)?;
            let Ok(mode) = mode_text.parse::<BlobMode>() else {
                return fail(&mode_path, ErrorKind::InvalidValue);
            };
            let content = obj.required("content", decode_resolution_content)?;
            obj.finish()?;
            Ok(Target::Blob(BlobTarget {
                path: resolved_path,
                mode,
                content,
            }))
        }
    }
}

fn decode_resolution_content(path: &str, value: Value) -> Result<BlobContent, Error> {
    let mut obj = Obj::new(path, value)?;
    let kind_path = obj.field("kind");
    let kind_text = de::string(&kind_path, obj.take("kind")?)?;
    let Ok(kind) = kind_text.parse::<BlobContentTag>() else {
        return fail(&kind_path, ErrorKind::InvalidValue);
    };
    let raw_digest = obj.required("raw_digest", decode_digest)?;
    match kind {
        BlobContentTag::Available => {
            let projection_digest = obj.required("projection_digest", decode_digest)?;
            obj.finish()?;
            Ok(BlobContent::Available {
                raw_digest,
                projection_digest,
            })
        }
        BlobContentTag::LfsPointer => {
            obj.finish()?;
            Ok(BlobContent::LfsPointer { raw_digest })
        }
    }
}

struct DecodedFact {
    fact: Fact,
    fact_digest: Digest,
    finding_key: Digest,
}

fn decode_fact(path: &str, value: Value) -> Result<DecodedFact, Error> {
    let fact_digest = hj(FACT_DOMAIN, &value);
    let mut obj = Obj::new(path, value)?;
    obj.required("schema", |path, value| {
        de::const_str(path, value, FACT_SCHEMA)
    })?;
    let finding_kind = obj.required("finding_kind", decode_enum)?;
    let key_path = obj.field("key_input");
    let (key_input, finding_key) = decode_key_input(&key_path, obj.take("key_input")?)?;
    let evidence_path = obj.field("evidence");
    let mut evidence = Obj::new(&evidence_path, obj.take("evidence")?)?;
    evidence.required("kind", |path, value| {
        de::const_str(path, value, "reference")
    })?;
    let resolution = evidence.required("resolution", decode_resolution)?;
    let multiplicity_path = evidence.field("occurrence_multiplicity");
    if de::integer(
        &multiplicity_path,
        evidence.take("occurrence_multiplicity")?,
    )? != 1
    {
        return fail(&multiplicity_path, ErrorKind::InvalidValue);
    }
    evidence.finish()?;
    obj.finish()?;

    let Some(fact) = Fact::new(key_input, resolution) else {
        return fail(path, ErrorKind::Inconsistent);
    };
    if fact.finding_kind() != finding_kind {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(DecodedFact {
        fact,
        fact_digest,
        finding_key,
    })
}

struct ItemCore {
    finding_key: Digest,
    fact: Fact,
    fact_digest: Digest,
    owner: OwnerId,
    reason: String,
    created_at: UtcInstant,
    expires_at: UtcInstant,
}

fn decode_item_core(obj: &mut Obj, fact_field: &str) -> Result<ItemCore, Error> {
    let finding_key_path = obj.field("finding_key");
    let finding_key = decode_digest(&finding_key_path, obj.take("finding_key")?)?;
    let fact_path = obj.field(fact_field);
    let decoded_fact = decode_fact(&fact_path, obj.take(fact_field)?)?;
    if finding_key != decoded_fact.finding_key {
        return fail(&finding_key_path, ErrorKind::DigestMismatch);
    }
    let fact_digest_field = format!("{fact_field}_digest");
    let fact_digest_path = obj.field(&fact_digest_field);
    let fact_digest = decode_digest(&fact_digest_path, obj.take(&fact_digest_field)?)?;
    if fact_digest != decoded_fact.fact_digest {
        return fail(&fact_digest_path, ErrorKind::DigestMismatch);
    }
    let owner = obj.required("owner", decode_owner)?;
    let reason = obj.required("reason", decode_reason)?;
    let created_at = obj.required("created_at", decode_instant)?;
    let expires_at = obj.required("expires_at", decode_instant)?;
    Ok(ItemCore {
        finding_key,
        fact: decoded_fact.fact,
        fact_digest,
        owner,
        reason,
        created_at,
        expires_at,
    })
}

fn decode_debt_item(path: &str, value: Value) -> Result<DebtItem, Error> {
    let mut obj = Obj::new(path, value)?;
    let debt_id = obj.required("debt_id", decode_artifact_id)?;
    let core = decode_item_core(&mut obj, "accepted_fact")?;
    obj.finish()?;
    if core.created_at >= core.expires_at {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(DebtItem {
        debt_id,
        finding_key: core.finding_key,
        accepted_fact: core.fact,
        accepted_fact_digest: core.fact_digest,
        owner: core.owner,
        reason: core.reason,
        created_at: core.created_at,
        expires_at: core.expires_at,
    })
}

fn decode_waiver_item(path: &str, value: Value) -> Result<WaiverItem, Error> {
    let mut obj = Obj::new(path, value)?;
    let waiver_id = obj.required("waiver_id", decode_artifact_id)?;
    let core = decode_item_core(&mut obj, "authorized_fact")?;
    let candidate_tree = obj.required("candidate_tree", decode_tree)?;
    let issuer = obj.required("issuer", decode_owner)?;
    let not_before = obj.required("not_before", decode_instant)?;
    obj.required("residual_disposition", |path, value| {
        de::const_str(path, value, "warn")
    })?;
    obj.finish()?;
    if core.created_at > not_before || not_before >= core.expires_at {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(WaiverItem {
        waiver_id,
        finding_key: core.finding_key,
        authorized_fact: core.fact,
        authorized_fact_digest: core.fact_digest,
        candidate_tree,
        owner: core.owner,
        issuer,
        reason: core.reason,
        created_at: core.created_at,
        not_before,
        expires_at: core.expires_at,
    })
}
