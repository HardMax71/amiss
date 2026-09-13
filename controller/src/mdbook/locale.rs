use std::collections::BTreeMap;

use amiss_wire::assessment::Nullable;
use amiss_wire::envelope::{Payload as _, document_digest};
use amiss_wire::locale::{
    EvidencePayloadSchema, LocaleCoverageEvidence, LocaleCoveragePlan, LocalePageInventory,
    LocaleSourcePage, LocaleTargetInventory, LocaleTargetOrigin, LocaleTargetPage,
};
use amiss_wire::model::{ArtifactId, Digest, RepoPathText};
use amiss_wire::publication::PublicationProducer;

use super::context::{chapters, relative_path, render_context, repository_path};
use super::model::RenderContext;
use super::{MDBOOK_RENDER_CONTEXT_BYTES, MDBOOK_VERSION, MdBookEvidenceError};

pub const MDBOOK_LOCALE_PRODUCER: &str = "amiss-controller-mdbook-locale";
pub const MDBOOK_LOCALE_VERSION: &str = "1.0.0";
const CONTEXT_DOMAIN: &str = "amiss/controller-mdbook-locale-context-v1";
const INPUT_DOMAIN: &str = "amiss/controller-mdbook-locale-input-v1";
const RESOURCE_DOMAIN: &str = "amiss/controller-mdbook-locale-resource-v1";

/// One locale's rendered book: the `book.toml` that produced it and the locale
/// the operator claims it carries. mdBook records no locale of its own, so the
/// claim is the operator's and the context digest is what binds it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct LocaleBuild {
    pub configuration: RepoPathText,
    pub locale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct LocaleBuildContext {
    pub source: LocaleBuild,
    pub target: LocaleBuild,
}

#[derive(serde::Serialize)]
struct LocaleInput<'a> {
    mdbook_version: &'static str,
    projection_version: &'static str,
    build: &'a LocaleBuild,
    config_digest: Digest,
    pages: &'a BTreeMap<String, Digest>,
}

struct Inventory {
    input_digest: Digest,
    pages: BTreeMap<String, Digest>,
}

/// Projects two pinned mdBook renderer contexts into one plan-bound locale
/// coverage inventory.
///
/// The page key is the chapter source path relative to the book source
/// directory, which is what stays equal across a translated book; the resource
/// digest covers the chapter's Markdown as the renderer received it. Both
/// inventories are complete, because a renderer context enumerates the whole
/// book, and neither carries a published product or translation lineage: mdBook
/// records nothing that would prove either, and claiming them from a rendered
/// book would be a guess.
///
/// # Errors
///
/// The plan, the claimed locales, the renderer version, the HTML renderer
/// selection, a source path, or the resulting evidence is invalid, ambiguous,
/// or outside a fixed ceiling.
pub fn mdbook_locale_evidence(
    plan_bytes: &[u8],
    locales: &LocaleBuildContext,
    source_context: &[u8],
    target_context: &[u8],
) -> Result<Vec<u8>, MdBookEvidenceError> {
    let plan =
        LocaleCoveragePlan::parse(plan_bytes).map_err(|_defect| MdBookEvidenceError::Plan)?;
    if locales.source.locale != plan.payload.scope.source_locale
        || locales.target.locale != plan.payload.scope.target_locale
    {
        return Err(MdBookEvidenceError::ContextIdentity);
    }
    let producer = mdbook_locale_producer(locales)?;
    let source = inventory(&locales.source, source_context)?;
    let target = inventory(&locales.target, target_context)?;
    let evidence = LocaleCoverageEvidence {
        schema: EvidencePayloadSchema::Current,
        plan_payload_digest: plan.payload_digest,
        docs: plan.payload.docs,
        scope: plan.payload.scope,
        producer,
        source: LocalePageInventory {
            input_digest: source.input_digest,
            product: Nullable::Null,
            complete: true,
            pages: source
                .pages
                .into_iter()
                .map(|(key, resource_digest)| LocaleSourcePage {
                    key,
                    resource_digest,
                })
                .collect(),
        },
        target: LocaleTargetInventory {
            input_digest: target.input_digest,
            product: Nullable::Null,
            complete: true,
            pages: target
                .pages
                .into_iter()
                .map(|(key, resource_digest)| LocaleTargetPage {
                    key,
                    resource_digest,
                    origin: LocaleTargetOrigin::TargetResource {
                        based_on_source_digest: Nullable::Null,
                    },
                })
                .collect(),
        },
    };
    evidence
        .emit()
        .map_err(|_defect| MdBookEvidenceError::Evidence)
}

/// The producer identity a plan must name for evidence from this projection to
/// bind, so an operator can author the plan before the build runs.
///
/// # Errors
///
/// The claimed build context cannot be encoded.
pub fn mdbook_locale_producer(
    locales: &LocaleBuildContext,
) -> Result<PublicationProducer, MdBookEvidenceError> {
    Ok(PublicationProducer {
        identity: ArtifactId::new(MDBOOK_LOCALE_PRODUCER.to_owned())
            .ok_or(MdBookEvidenceError::Evidence)?,
        version: MDBOOK_LOCALE_VERSION.to_owned(),
        context_digest: document_digest(CONTEXT_DOMAIN, locales)
            .ok_or(MdBookEvidenceError::ContextShape)?,
    })
}

fn inventory(build: &LocaleBuild, context_bytes: &[u8]) -> Result<Inventory, MdBookEvidenceError> {
    if u64::try_from(context_bytes.len()).unwrap_or(u64::MAX) > MDBOOK_RENDER_CONTEXT_BYTES {
        return Err(MdBookEvidenceError::ContextBytes);
    }
    let context: RenderContext = serde_json::from_slice(context_bytes)
        .map_err(|_defect| MdBookEvidenceError::ContextShape)?;
    let (_source_directory, items, config_digest) = render_context(&context)?;
    let mut pages = BTreeMap::new();
    for chapter in chapters(items) {
        let Some(source_path) = chapter.source_path.as_deref() else {
            continue;
        };
        if chapter.path.is_none() {
            return Err(MdBookEvidenceError::UnsupportedBuild);
        }
        let key = repository_path(None, &relative_path(source_path, false)?)?
            .ok_or(MdBookEvidenceError::Path)?;
        let digest = document_digest(RESOURCE_DOMAIN, &chapter.content)
            .ok_or(MdBookEvidenceError::Evidence)?;
        if pages.insert(key, digest).is_some() {
            return Err(MdBookEvidenceError::UnsupportedBuild);
        }
    }
    let input_digest = document_digest(
        INPUT_DOMAIN,
        &LocaleInput {
            mdbook_version: MDBOOK_VERSION,
            projection_version: MDBOOK_LOCALE_VERSION,
            build,
            config_digest,
            pages: &pages,
        },
    )
    .ok_or(MdBookEvidenceError::Evidence)?;
    Ok(Inventory {
        input_digest,
        pages,
    })
}
