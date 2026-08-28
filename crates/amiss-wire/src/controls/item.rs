use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::Value;
use crate::model::{ObjectFormat, Oid, OwnerId, RepoPathText, UtcInstant};
use crate::resolution::{
    BlobContent, BlobContentTag, BlobTarget, Missing, MissingTag, Resolution, ResolutionTag,
    Target, TargetTag,
};

use super::fact::{Fact, FindingKeyInput, FindingScope, TargetIntent};
use super::{
    FACT_DOMAIN, FINDING_KEY_DOMAIN, decode_enum, decode_instant, decode_owner, decode_repo_path,
};

const FINDING_KEY_INPUT_SCHEMA: &str = "amiss/scanner-finding-key-input";
const FACT_SCHEMA: &str = "amiss/scanner-fact";

fn decode_reason(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    let length = raw.chars().count();
    if (1..=1024).contains(&length) && raw.chars().any(|c| !c.is_whitespace()) {
        Ok(raw)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn decode_oid(path: &str, value: Value) -> Result<Oid, Error> {
    let raw = de::string(path, value)?;
    let object_format = match raw.len() {
        40 => ObjectFormat::Sha1,
        64 => ObjectFormat::Sha256,
        _ => return fail(path, ErrorKind::InvalidValue),
    };
    Oid::new(object_format, raw).ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_scope(path: &str, value: Value) -> Result<FindingScope, Error> {
    let mut obj = Obj::new(path, value)?;
    obj.required("kind", |path, value| {
        de::const_str(path, value, "reference")
    })?;
    let document = obj.required("document", decode_repo_path)?;
    let source_construct = obj.required("source_construct", decode_enum)?;
    let normalized_target_intent = obj.required("normalized_target_intent", |path, value| {
        let mut intent = Obj::new(path, value)?;
        intent.required("kind", |path, value| {
            de::const_str(path, value, "repository-path")
        })?;
        let target_path = intent.required("path", decode_repo_path)?;
        let target_kind = intent.required("target_kind", decode_enum)?;
        let nullable_digest = |path: &str, value| {
            de::nullable(value)
                .map(|value| de::digest(path, value))
                .transpose()
        };
        let query_digest = intent.required("query_digest", nullable_digest)?;
        let fragment_digest = intent.required("fragment_digest", nullable_digest)?;
        let commit_path = intent.field("commit_oid");
        let commit_oid = intent
            .take_optional("commit_oid")
            .map(|value| decode_oid(&commit_path, value))
            .transpose()?;
        intent.finish()?;
        Ok(TargetIntent {
            commit_oid,
            path: target_path,
            target_kind,
            query_digest,
            fragment_digest,
        })
    })?;
    let occurrence_path = obj.field("occurrence");
    let mut occurrence = Obj::new(&occurrence_path, obj.take("occurrence")?)?;
    occurrence.required("kind", |path, value| {
        de::const_str(path, value, "source-projection")
    })?;
    let source_projection_digest = occurrence.required("source_projection_digest", de::digest)?;
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
                MissingTag::PathNotFound => {
                    let near = obj.required("near", |path, value| {
                        de::nullable(value)
                            .map(|value| decode_repo_path(path, value))
                            .transpose()
                    })?;
                    let same_object_at = obj
                        .take_optional("same_object_at")
                        .filter(|value| !matches!(value, Value::Null))
                        .map(|value| decode_repo_path(&obj.field("same_object_at"), value))
                        .transpose()?;
                    Missing::PathNotFound {
                        path: resolved_path,
                        near,
                        same_object_at,
                    }
                }
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
    let kind = obj.required("kind", decode_enum)?;
    let resolved_path = obj.required("path", decode_repo_path)?;
    let target = match kind {
        TargetTag::Tree => Target::Tree {
            path: resolved_path,
        },
        TargetTag::Blob => {
            let mode = obj.required("mode", decode_enum)?;
            let content = obj.required("content", |path, value| {
                let mut content = Obj::new(path, value)?;
                let kind = content.required("kind", decode_enum)?;
                let raw_digest = content.required("raw_digest", de::digest)?;
                let decoded = match kind {
                    BlobContentTag::Available => {
                        let projection_digest =
                            content.required("projection_digest", de::digest)?;
                        BlobContent::Available {
                            raw_digest,
                            projection_digest,
                        }
                    }
                    BlobContentTag::LfsPointer => BlobContent::LfsPointer { raw_digest },
                };
                content.finish()?;
                Ok(decoded)
            })?;
            Target::Blob(BlobTarget {
                path: resolved_path,
                mode,
                content,
            })
        }
    };
    obj.finish()?;
    Ok(target)
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

pub(super) struct ItemCore {
    pub(super) finding_key: Digest,
    pub(super) fact: Fact,
    pub(super) fact_digest: Digest,
    pub(super) owner: OwnerId,
    pub(super) reason: String,
    pub(super) created_at: UtcInstant,
    pub(super) expires_at: UtcInstant,
}

pub(super) fn decode_item_core(obj: &mut Obj, fact_field: &str) -> Result<ItemCore, Error> {
    let finding_key_path = obj.field("finding_key");
    let finding_key = de::digest(&finding_key_path, obj.take("finding_key")?)?;
    let fact_path = obj.field(fact_field);
    let decoded_fact = decode_fact(&fact_path, obj.take(fact_field)?)?;
    if finding_key != decoded_fact.finding_key {
        return fail(&finding_key_path, ErrorKind::DigestMismatch);
    }
    let fact_digest_field = format!("{fact_field}_digest");
    let fact_digest_path = obj.field(&fact_digest_field);
    let fact_digest = de::digest(&fact_digest_path, obj.take(&fact_digest_field)?)?;
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
