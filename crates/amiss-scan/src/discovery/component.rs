use amiss_wire::extraction::{SourceConstruct, TransclusionKind};
use amiss_wire::model::RepoPath;

use super::{DocumentStatus, SnapshotDiscovery, declared_root, followed, regular_file};
use crate::route::{ANTORA, ANTORA_FAMILIES, directory, join, normalized_path_under, within};

/// An Antora resource ID, `[module:][family$]relative`, anchored at the family
/// directory of the named module in every source root of the document's own
/// component, its own root first. A cross reference defaults to the page
/// family and an image to the image family, while an include without a family
/// coordinate stays relative to the file that includes it. A version or
/// component coordinate names a catalogue this tree does not hold, and a `./`
/// or `../` relative is relative to the page.
pub(crate) fn antora_anchor(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    let Some(construct) = construct else {
        return Vec::new();
    };
    let default_family = if construct == SourceConstruct::AsciidocCrossReference {
        Some("page")
    } else if construct.is_image() {
        Some("image")
    } else if construct == SourceConstruct::AsciidocInclude {
        None
    } else {
        return Vec::new();
    };
    let Some((root, own_module)) = antora_module(snapshot, document.as_bytes()) else {
        return Vec::new();
    };
    let Some(coordinate) = coordinate(path_part) else {
        return Vec::new();
    };
    let module = match (coordinate.component, coordinate.module) {
        (_, Some(module)) => module.as_bytes(),
        (Some(_), None) => b"ROOT".as_slice(),
        (None, None) => own_module,
    };
    let resource = coordinate.resource;
    let Some((family, relative)) = resource
        .split_once('$')
        .or_else(|| default_family.map(|family| (family, resource)))
    else {
        return Vec::new();
    };
    let Some((_, family_directory)) = ANTORA_FAMILIES.iter().find(|(name, _)| *name == family)
    else {
        return Vec::new();
    };
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.starts_with("./")
        || relative.starts_with("../")
    {
        return Vec::new();
    }
    coordinate_roots(snapshot, root, &coordinate)
        .into_iter()
        .map(|root| {
            let module_directory = join(&join(&root, b"modules"), module);
            (
                join(&module_directory, family_directory),
                relative.to_owned(),
            )
        })
        .collect()
}

/// An Antora resource ID split at its coordinates,
/// `[version@][component:][module:][family$]relative`. A component coordinate
/// with an empty module, `component::page.adoc`, names its `ROOT` module.
struct Coordinate<'a> {
    version: Option<&'a str>,
    component: Option<&'a str>,
    module: Option<&'a str>,
    resource: &'a str,
}

fn coordinate(path_part: &str) -> Option<Coordinate<'_>> {
    let (version, rest) = match path_part.split_once('@') {
        Some((version, rest)) => (Some(version), rest),
        None => (None, path_part),
    };
    let parts: Vec<&str> = rest.splitn(3, ':').collect();
    let (component, module, resource) = match parts.as_slice() {
        [resource] => (None, None, *resource),
        [module, resource] => (None, Some(*module), *resource),
        [component, module, resource] => (Some(*component), Some(*module), *resource),
        _ => return None,
    };
    let named = |coordinate: Option<&str>| {
        coordinate.is_none_or(|name| !name.contains(['/', ':']) && !name.is_empty())
    };
    (named(version)
        && named(component)
        && named(module.filter(|module| !module.is_empty()))
        && component.is_none_or(|_| module.is_some()))
    .then_some(Coordinate {
        version,
        component,
        module: module.filter(|module| !module.is_empty()),
        resource,
    })
}

/// Every source root this tree holds for the component version a coordinate
/// names, the document's own root first when it is one of them. Antora
/// assembles one component version from every root whose descriptor spells
/// the same name and version, so a module's resources are stored across all of
/// them and a coordinate naming one is answered by whichever holds it. A
/// coordinate that leaves both out keeps the document's own component version,
/// and one naming a component alone means that component's latest version,
/// which is whichever version the tree holds.
fn coordinate_roots(
    snapshot: &SnapshotDiscovery,
    root: &[u8],
    coordinate: &Coordinate<'_>,
) -> Vec<Vec<u8>> {
    let own = descriptor(snapshot, root);
    let Some(own) = own else {
        return if coordinate.component.is_none() && coordinate.version.is_none() {
            vec![root.to_vec()]
        } else {
            Vec::new()
        };
    };
    let name = coordinate.component.unwrap_or(&own.name);
    let version = match (coordinate.component, coordinate.version) {
        (_, Some(version)) => Some(Some(version)),
        (None, None) => Some(own.version.as_deref()),
        (Some(_), None) => None,
    };
    let mut out: Vec<Vec<u8>> = snapshot
        .antora_components
        .iter()
        .filter(|(_, declared)| {
            declared.name == name
                && version.is_none_or(|version| declared.version.as_deref() == version)
        })
        .map(|(path, _)| directory(path.as_bytes()).to_vec())
        .collect();
    out.sort_by_key(|candidate| candidate.as_slice() != root);
    out
}

/// Whether an Antora coordinate names a component version no source root in
/// this tree holds: another version, or a component whose descriptor lives
/// in another repository. Its resources sit in a catalogue this engine does
/// not build, so the reference is declined rather than read as a path.
#[must_use]
pub(crate) fn antora_elsewhere(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    path_part: &str,
) -> bool {
    let reads = construct.is_some_and(|construct| {
        matches!(
            construct,
            SourceConstruct::AsciidocCrossReference | SourceConstruct::AsciidocInclude
        ) || construct.is_image()
    });
    let Some((root, _)) = antora_module(snapshot, document.as_bytes()).filter(|_| reads) else {
        return false;
    };
    coordinate(path_part).is_some_and(|coordinate| {
        (coordinate.component.is_some() || coordinate.version.is_some())
            && coordinate_roots(snapshot, root, &coordinate).is_empty()
    })
}

/// What the descriptor of one Antora source root declares, when the tree holds
/// one there and this reader spelled it out.
fn descriptor<'a>(
    snapshot: &'a SnapshotDiscovery,
    root: &[u8],
) -> Option<&'a crate::route::AntoraComponent> {
    ANTORA.declared_by.iter().find_map(|name| {
        RepoPath::from_bytes(join(root, name.as_bytes()))
            .and_then(|path| snapshot.antora_components.get(&path))
    })
}

/// Whether the component a path belongs to is assembled by an extension rather
/// than by the tree. Antora's `ext` block is where a component descriptor
/// names the extensions that add resources to it while the site is built, and
/// this engine runs none of them, so the resources such a component serves are
/// not the ones a tree walk can count.
pub(crate) fn assembled(snapshot: &SnapshotDiscovery, path: &RepoPath) -> bool {
    let Some(root) = declared_root(snapshot, path.as_bytes(), ANTORA.declared_by) else {
        return false;
    };
    let own = Coordinate {
        version: None,
        component: None,
        module: None,
        resource: "",
    };
    coordinate_roots(snapshot, &root, &own)
        .iter()
        .filter_map(|root| descriptor(snapshot, root))
        .any(|component| component.extended)
}

/// Whether an Antora component keeps this document among the resources only an
/// include renders, its partials and examples, rather than among its pages.
pub(crate) fn antora_fragment(snapshot: &SnapshotDiscovery, document: &[u8]) -> bool {
    let Some((root, module)) = antora_module(snapshot, document) else {
        return false;
    };
    let module_directory = join(&join(root, b"modules"), module);
    document
        .strip_prefix(module_directory.as_slice())
        .and_then(|rest| rest.strip_prefix(b"/"))
        .is_some_and(|rest| rest.starts_with(b"partials/") || rest.starts_with(b"examples/"))
}

/// The component root and module a document belongs to: the nearest
/// `modules/<name>/` on its path whose parent directory holds `antora.yml`.
pub(crate) fn antora_module<'a>(
    snapshot: &SnapshotDiscovery,
    document: &'a [u8],
) -> Option<(&'a [u8], &'a [u8])> {
    let mut offset = 0_usize;
    let segments: Vec<(usize, &[u8])> = document
        .split(|byte| *byte == b'/')
        .map(|segment| {
            let start = offset;
            offset = offset.saturating_add(segment.len()).saturating_add(1);
            (start, segment)
        })
        .collect();
    segments
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, (start, segment))| {
            if *segment != b"modules" {
                return None;
            }
            let (_, module) = segments.get(index.saturating_add(1))?;
            segments.get(index.saturating_add(2))?;
            if module.is_empty() {
                return None;
            }
            let root = document.get(..start.saturating_sub(1))?;
            ANTORA
                .declared_by
                .iter()
                .any(|name| regular_file(snapshot, join(root, name.as_bytes())))
                .then_some((root, *module))
        })
}

/// Whether a document is an Antora page, which renders the partials it
/// includes: a file in the `pages` family of its module.
pub(crate) fn antora_page(snapshot: &SnapshotDiscovery, document: &RepoPath) -> bool {
    let raw = document.as_bytes();
    antora_module(snapshot, raw).is_some_and(|(root, module)| {
        within(raw, &join(&join(&join(root, b"modules"), module), b"pages"))
    })
}

/// The files one `AsciiDoc` file includes, each named by an Antora resource ID
/// or a path, and read from the file itself, which is how Antora reads an
/// include nested in a partial. Each is marked when it selects nothing, since
/// a tag or line selection renders only part of the file.
pub(crate) fn antora_includes(
    snapshot: &SnapshotDiscovery,
    file: &RepoPath,
) -> Vec<(RepoPath, bool)> {
    let Some(DocumentStatus::Scanned(scanned)) = snapshot
        .document(file.as_bytes())
        .map(|record| &record.status)
    else {
        return Vec::new();
    };
    let Some(source) = scanned.anchor_source.as_ref() else {
        return Vec::new();
    };
    followed(&source.transclusions)
        .into_iter()
        .filter(|entry| entry.kind != Ok(TransclusionKind::Literal))
        .filter_map(|entry| {
            let (parent, relative) = antora_anchor(
                snapshot,
                file,
                Some(SourceConstruct::AsciidocInclude),
                &entry.target,
            )
            .into_iter()
            .next()
            .unwrap_or_else(|| (directory(file.as_bytes()).to_vec(), entry.target.clone()));
            normalized_path_under(&parent, false, &relative)
                .ok()
                .map(|(path, _)| (path, entry.kind.is_ok()))
        })
        .collect()
}
