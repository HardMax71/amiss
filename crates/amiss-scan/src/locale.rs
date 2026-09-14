use std::collections::BTreeMap;

use amiss_git::{GitResources, ObjectKind, Repository, parse_commit};
use amiss_wire::artifact_id;
use amiss_wire::assessment::Nullable;
use amiss_wire::controls::{DOCUMENT_SUFFIX_BYTES, GitMode};
use amiss_wire::de::Document;
use amiss_wire::envelope::{Payload as _, document_digest};
use amiss_wire::locale::{
    EvidencePayloadSchema, LocaleCoverageEvidence, LocaleCoveragePlan, LocalePageInventory,
    LocaleSourcePage, LocaleTargetInventory, LocaleTargetOrigin, LocaleTargetPage,
};
use amiss_wire::model::{ArtifactId, Digest, Oid, RepoPath, RepoPathText};
use amiss_wire::publication::PublicationProducer;
use amiss_wire::semantic::producer_version_valid;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::discovery::{WalkMode, discover_walk};

pub const LOCALE_CONTEXT_BYTES: u64 = 65_536;
pub const PRODUCER_IDENTITY: ArtifactId = artifact_id!("amiss-locale-tree");
pub const PRODUCER_VERSION: &str = "1.0.0";
/// The fallback class a target page carries when its bytes are the source's.
pub const SOURCE_IDENTICAL_CLASS: ArtifactId = artifact_id!("source-identical");
const CONTEXT_DOMAIN: &str = "amiss/locale-tree-context-v1";
const RESOURCE_DOMAIN: &str = "amiss/locale-tree-resource-v1";
const INPUT_DOMAIN: &str = "amiss/locale-tree-input-v1";

/// Where one locale's pages live in the candidate tree. A layout that gives
/// each locale its own directory leaves `suffix` null; one that marks the
/// locale in the filename names it there, so `guide/start.fr.md` and
/// `guide/start.md` share the key `guide/start.md`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleSide {
    pub root: RepoPathText,
    pub locale: String,
    pub suffix: Option<String>,
}

/// The producer's whole input beyond the tree: which subtree each locale
/// owns and which file suffixes are pages at all.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleTreeContext {
    pub source: LocaleSide,
    pub target: LocaleSide,
    pub documents: Vec<String>,
}

/// The context's own grammar is its shape; what it claims is checked against
/// the plan it is read for.
impl Document for LocaleTreeContext {
    type Defect = amiss_wire::de::Error;
    const BYTES: u64 = LOCALE_CONTEXT_BYTES;

    fn validate(&self) -> Result<(), amiss_wire::de::Error> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InventoryError {
    Context,
    Plan,
    Snapshot(Error),
    Evidence,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Source,
    Target,
}

#[derive(Serialize)]
struct InventoryInput<'a> {
    producer_version: &'static str,
    tree: &'a Oid,
    side: &'a LocaleSide,
    pages: &'a BTreeMap<String, Digest>,
}

#[derive(Default)]
struct Pages {
    rows: BTreeMap<String, Digest>,
    complete: bool,
}

/// The producer identity a plan must name for this projection's evidence to
/// bind, so the plan can be written before the tree is walked.
///
/// # Errors
///
/// The context cannot be encoded.
pub fn tree_producer(context: &LocaleTreeContext) -> Result<PublicationProducer, InventoryError> {
    Ok(PublicationProducer {
        identity: PRODUCER_IDENTITY,
        version: PRODUCER_VERSION.to_owned(),
        context_digest: document_digest(CONTEXT_DOMAIN, context).ok_or(InventoryError::Context)?,
    })
}

/// Projects the tree of the commit the plan binds into the locale coverage
/// inventory pair its two declared subtrees hold.
///
/// The page key is the path under its own locale root, so the same document
/// carries one key in every locale. The resource digest names the blob the
/// path resolves to, which makes a target page whose bytes are still the
/// source's an exact `source-identical` fallback rather than a translation.
/// No file content is read.
///
/// # Errors
///
/// The plan is unreadable, names another object format, or names a commit
/// whose tree is not the one it binds; the context contradicts the plan; the
/// tree cannot be walked; or the evidence leaves the coverage contract.
pub fn tree_inventory(
    repo: &Repository,
    git: &mut GitResources,
    plan_bytes: &[u8],
    context: &LocaleTreeContext,
) -> Result<Vec<u8>, InventoryError> {
    let plan = LocaleCoveragePlan::parse(plan_bytes).map_err(|_defect| InventoryError::Plan)?;
    validate(context, &plan.payload)?;
    if plan.payload.docs.object_format != repo.object_format() {
        return Err(InventoryError::Plan);
    }
    let object = repo
        .read_expected(git, &plan.payload.docs.commit, ObjectKind::Commit)
        .map_err(|defect| InventoryError::Snapshot(Error::from(defect)))?;
    let tree = parse_commit(repo.object_format(), &object.body)
        .map_err(|defect| InventoryError::Snapshot(Error::from(defect)))?
        .tree;
    if plan.payload.docs.tree != tree {
        return Err(InventoryError::Plan);
    }
    let (source, target) = walk(repo, git, context, &tree)?;
    let producer = tree_producer(context)?;
    let identical = SOURCE_IDENTICAL_CLASS;
    let evidence = LocaleCoverageEvidence {
        schema: EvidencePayloadSchema::Current,
        plan_payload_digest: plan.payload_digest,
        docs: plan.payload.docs,
        scope: plan.payload.scope,
        producer,
        source: LocalePageInventory {
            input_digest: input_digest(&tree, &context.source, &source.rows)?,
            product: Nullable::Null,
            complete: source.complete,
            pages: source
                .rows
                .iter()
                .map(|(key, resource_digest)| LocaleSourcePage {
                    key: key.clone(),
                    resource_digest: *resource_digest,
                })
                .collect(),
        },
        target: LocaleTargetInventory {
            input_digest: input_digest(&tree, &context.target, &target.rows)?,
            product: Nullable::Null,
            complete: target.complete,
            pages: target
                .rows
                .iter()
                .map(|(key, resource_digest)| LocaleTargetPage {
                    key: key.clone(),
                    resource_digest: *resource_digest,
                    origin: origin(source.rows.get(key), *resource_digest, &identical),
                })
                .collect(),
        },
    };
    evidence.emit().map_err(|_defect| InventoryError::Evidence)
}

fn origin(source: Option<&Digest>, target: Digest, class: &ArtifactId) -> LocaleTargetOrigin {
    match source {
        Some(source) if *source == target => LocaleTargetOrigin::Fallback {
            class: class.clone(),
            source_resource_digest: target,
        },
        Some(_) | None => LocaleTargetOrigin::TargetResource {
            based_on_source_digest: Nullable::Null,
        },
    }
}

fn input_digest(
    tree: &Oid,
    side: &LocaleSide,
    pages: &BTreeMap<String, Digest>,
) -> Result<Digest, InventoryError> {
    document_digest(
        INPUT_DOMAIN,
        &InventoryInput {
            producer_version: PRODUCER_VERSION,
            tree,
            side,
            pages,
        },
    )
    .ok_or(InventoryError::Evidence)
}

fn walk(
    repo: &Repository,
    git: &mut GitResources,
    context: &LocaleTreeContext,
    tree: &Oid,
) -> Result<(Pages, Pages), InventoryError> {
    let discovery = discover_walk(repo, git, tree, WalkMode::Entries { selection: None })
        .map_err(InventoryError::Snapshot)?;
    let mut source = Pages::new();
    let mut target = Pages::new();
    for (path, (mode, oid)) in &discovery.entries {
        let Some(text) = path.as_str() else {
            for (side, pages) in [
                (&context.source, &mut source),
                (&context.target, &mut target),
            ] {
                if under(side, path) {
                    pages.complete = false;
                }
            }
            continue;
        };
        let Some((owner, key)) = assign(context, text) else {
            continue;
        };
        let pages = match owner {
            Owner::Source => &mut source,
            Owner::Target => &mut target,
        };
        if *mode != GitMode::RegularFile {
            pages.complete = false;
            continue;
        }
        let digest = document_digest(RESOURCE_DOMAIN, oid).ok_or(InventoryError::Evidence)?;
        pages.rows.insert(key, digest);
    }
    Ok((source, target))
}

fn under(side: &LocaleSide, path: &RepoPath) -> bool {
    path.as_bytes()
        .strip_prefix(side.root.as_str().as_bytes())
        .is_some_and(|rest| rest.starts_with(b"/"))
}

/// The side that owns a path and the key it carries there. A root nested
/// inside the other side's root is the more specific claim, so it wins.
fn assign(context: &LocaleTreeContext, path: &str) -> Option<(Owner, String)> {
    let source = context.source.root.as_str().len();
    let target = context.target.root.as_str().len();
    let order = if target >= source {
        [
            (Owner::Target, &context.target),
            (Owner::Source, &context.source),
        ]
    } else {
        [
            (Owner::Source, &context.source),
            (Owner::Target, &context.target),
        ]
    };
    order
        .into_iter()
        .find_map(|(owner, side)| keyed(side, &context.documents, path).map(|key| (owner, key)))
}

fn keyed(side: &LocaleSide, documents: &[String], path: &str) -> Option<String> {
    let relative = path
        .strip_prefix(side.root.as_str())
        .and_then(|rest| rest.strip_prefix('/'))?;
    let document = documents
        .iter()
        .filter(|suffix| relative.ends_with(suffix.as_str()))
        .max_by_key(|suffix| suffix.len())?;
    let stem = relative.strip_suffix(document.as_str())?;
    match &side.suffix {
        Some(locale) => stem
            .strip_suffix(locale.as_str())
            .and_then(|head| head.strip_suffix('.'))
            .map(|head| format!("{head}{document}")),
        None => (!basename(stem).contains('.')).then(|| relative.to_owned()),
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn validate(context: &LocaleTreeContext, plan: &LocaleCoveragePlan) -> Result<(), InventoryError> {
    if context.source.locale != plan.scope.source_locale
        || context.target.locale != plan.scope.target_locale
    {
        return Err(InventoryError::Context);
    }
    if context.source.root == context.target.root && context.source.suffix == context.target.suffix
    {
        return Err(InventoryError::Context);
    }
    for side in [&context.source, &context.target] {
        let valid = side.suffix.as_ref().is_none_or(|suffix| {
            producer_version_valid(suffix) && !suffix.contains('.') && !suffix.contains('/')
        });
        if !valid {
            return Err(InventoryError::Context);
        }
    }
    if context.documents.is_empty() {
        return Err(InventoryError::Context);
    }
    let ordered = context
        .documents
        .iter()
        .zip(context.documents.iter().skip(1))
        .all(|(previous, current)| previous < current);
    let shaped = context.documents.iter().all(|suffix| {
        suffix.starts_with('.') && suffix.len() > 1 && suffix.len() <= DOCUMENT_SUFFIX_BYTES
    });
    (ordered && shaped)
        .then_some(())
        .ok_or(InventoryError::Context)
}

impl Pages {
    fn new() -> Self {
        Self {
            rows: BTreeMap::new(),
            complete: true,
        }
    }
}
