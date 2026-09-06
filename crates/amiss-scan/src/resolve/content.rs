use std::collections::BTreeMap;

use amiss_git::{ObjectKind, ValueCap};
use amiss_wire::controls::{GitMode, ResourceName};
use amiss_wire::digest::{Digest, hb, hj_serde};
use amiss_wire::model::{Oid, RepoPath};
use amiss_wire::resolution::BlobContent;

use crate::resources::Aggregate;
use crate::{Error, lfs};

use super::anchor::Anchors;
use super::line::LineRange;
use super::{RAW_EVIDENCE_DOMAIN, Resolver, TARGET_PROJECTION_DOMAIN, TargetCache};

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
}

impl Content {
    pub(super) const fn evidence(&self) -> BlobContent {
        match self {
            Self::Ordinary {
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
    hj_serde(domain, |mut writer| {
        serde_json_canonicalizer::to_writer(
            &TargetProjection {
                git_mode: mode,
                raw_digest,
            },
            &mut writer,
        )
    })
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
        return Ok(cached.content.evidence());
    }
    let cap = ValueCap {
        resource: ResourceName::ReferencedTargetBlobBytes,
        limit: resolver.scan.limits().referenced_target_blob_bytes,
    };
    let object = resolver
        .repo
        .read_expected_capped(resolver.git, oid, ObjectKind::Blob, cap)
        .map_err(Error::from)?;
    resolver.scan.charge(
        Aggregate::ReferencedTargetBytes,
        u64::try_from(object.body.len()).unwrap_or(u64::MAX),
    )?;
    let raw = hb(RAW_EVIDENCE_DOMAIN, &object.body);
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
