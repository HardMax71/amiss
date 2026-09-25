use sha2::Digest as _;
use std::collections::BTreeMap;

use amiss_git::{ObjectKind, ValueCap};
use amiss_wire::controls::{GitMode, ResourceName};
use amiss_wire::model::Digest;
use amiss_wire::model::{Oid, RepoPath};
use amiss_wire::resolution::BlobContent;

use crate::resources::Aggregate;
use crate::{Error, lfs};

use super::{Anchors, LineRange, Resolver, TARGET_PROJECTION_DOMAIN, TargetCache};

use amiss_wire::model::RAW_EVIDENCE_DOMAIN;

#[derive(serde::Serialize)]
struct TargetProjection {
    git_mode: GitMode,
    raw_digest: Digest,
}

#[derive(Debug)]
pub(super) struct CachedContent {
    pub(super) mode: GitMode,
    pub(super) oid: Oid,
    pub(super) content: Content,
}

#[derive(Debug)]
pub(super) enum Content {
    Ordinary {
        raw_digest: Digest,
        projection_digest: Digest,
        body: Box<[u8]>,
        line_projections: BTreeMap<LineRange, Option<Digest>>,
        anchors: Anchors,
    },
    LfsPointer {
        raw_digest: Digest,
    },
    /// A blob past the per-target ceiling, never read: its digests stand on
    /// its object id, and the crossing is kept for a reader that needs bytes.
    Unread {
        raw_digest: Digest,
        projection_digest: Digest,
        configured_limit: u64,
        observed_lower_bound: u64,
    },
}

/// The domain a never-read target's evidence is digested under, apart from
/// every digest over bytes, since the preimage is an object id.
const UNREAD_TARGET_DOMAIN: &str = "amiss/scanner-unread-target";

impl Content {
    pub(super) const fn evidence(&self) -> BlobContent {
        match self {
            Self::Ordinary {
                raw_digest,
                projection_digest,
                ..
            }
            | Self::Unread {
                raw_digest,
                projection_digest,
                ..
            } => BlobContent::Available {
                raw_digest: *raw_digest,
                projection_digest: *projection_digest,
            },
            Self::LfsPointer { raw_digest } => BlobContent::LfsPointer {
                raw_digest: *raw_digest,
            },
        }
    }
}

pub(super) fn target_projection(
    domain: &str,
    mode: GitMode,
    raw_digest: Digest,
) -> Result<Digest, Error> {
    {
        let mut writer =
            digest_io::IoWrapper(sha2::Sha256::new_with_prefix(domain).chain_update([0_u8]));
        serde_json_canonicalizer::to_writer(
            &TargetProjection {
                git_mode: mode,
                raw_digest,
            },
            &mut writer,
        )
        .map(|()| Digest::from(writer.0.finalize().0))
    }
    .map_err(|_defect| Error::Internal)
}

pub(super) fn content_cache<'a>(
    cache: &'a mut TargetCache,
    commit_oid: Option<&Oid>,
) -> &'a mut BTreeMap<RepoPath, CachedContent> {
    match commit_oid {
        Some(oid) => cache.historical_read.entry(oid.clone()).or_default(),
        None => &mut cache.read,
    }
}

/// Reads one referenced regular blob once per exact path, mode, and object
/// identity in the bound scan scope. Pointer content keeps its raw digest and
/// no projection; ordinary content carries both.
pub(super) fn read_target(
    resolver: &mut Resolver<'_>,
    path: &RepoPath,
    mode: GitMode,
    oid: &Oid,
) -> Result<BlobContent, Error> {
    if let Some(cached) = content_cache(resolver.cache, resolver.commit_oid.as_ref()).get(path)
        && cached.mode == mode
        && &cached.oid == oid
    {
        if let Content::Unread {
            configured_limit,
            observed_lower_bound,
            ..
        } = cached.content
        {
            return Err(Error::ResourceLimit {
                resource: ResourceName::ReferencedTargetBlobBytes,
                configured_limit,
                observed_lower_bound,
            });
        }
        return Ok(cached.content.evidence());
    }
    let cap = ValueCap {
        resource: ResourceName::ReferencedTargetBlobBytes,
        limit: resolver.scan.limits().referenced_target_blob_bytes,
    };
    let object = match resolver
        .repo
        .read_expected_capped(resolver.git, oid, ObjectKind::Blob, cap)
    {
        Ok(object) => object,
        Err(defect) => {
            let defect = Error::from(defect);
            if let Error::ResourceLimit {
                resource: ResourceName::ReferencedTargetBlobBytes,
                configured_limit,
                observed_lower_bound,
            } = defect
            {
                let raw_digest = Digest::from(
                    sha2::Sha256::new_with_prefix(UNREAD_TARGET_DOMAIN)
                        .chain_update([0_u8])
                        .chain_update(oid.as_str())
                        .finalize()
                        .0,
                );
                let content = Content::Unread {
                    raw_digest,
                    projection_digest: target_projection(
                        TARGET_PROJECTION_DOMAIN,
                        mode,
                        raw_digest,
                    )?,
                    configured_limit,
                    observed_lower_bound,
                };
                content_cache(resolver.cache, resolver.commit_oid.as_ref()).insert(
                    path.clone(),
                    CachedContent {
                        mode,
                        oid: oid.clone(),
                        content,
                    },
                );
            }
            return Err(defect);
        }
    };
    resolver.scan.charge(
        Aggregate::ReferencedTargetBytes,
        u64::try_from(object.body.len()).unwrap_or(u64::MAX),
    )?;
    let raw = Digest::from(
        sha2::Sha256::new_with_prefix(RAW_EVIDENCE_DOMAIN)
            .chain_update([0_u8])
            .chain_update(&object.body)
            .finalize()
            .0,
    );
    let content = if lfs::is_pointer(&object.body) {
        Content::LfsPointer { raw_digest: raw }
    } else {
        Content::Ordinary {
            raw_digest: raw,
            projection_digest: target_projection(TARGET_PROJECTION_DOMAIN, mode, raw)?,
            body: object.body.into_boxed_slice(),
            line_projections: BTreeMap::new(),
            anchors: Anchors::Unread,
        }
    };
    let evidence = content.evidence();
    let cached = CachedContent {
        mode,
        oid: oid.clone(),
        content,
    };
    content_cache(resolver.cache, resolver.commit_oid.as_ref()).insert(path.clone(), cached);
    Ok(evidence)
}

/// A located target's content: read, or past its ceiling never read. The
/// path still resolves, a change to its object still reaches every
/// comparison, and a fragment into it stays unsupported for want of bytes.
pub(super) fn located_content(
    resolver: &mut Resolver<'_>,
    path: &RepoPath,
    mode: GitMode,
    oid: &Oid,
) -> Result<BlobContent, Error> {
    read_target(resolver, path, mode, oid).or_else(|defect| {
        content_cache(resolver.cache, resolver.commit_oid.as_ref())
            .get(path)
            .filter(|cached| {
                matches!(cached.content, Content::Unread { .. })
                    && cached.mode == mode
                    && &cached.oid == oid
            })
            .map(|cached| cached.content.evidence())
            .ok_or(defect)
    })
}
