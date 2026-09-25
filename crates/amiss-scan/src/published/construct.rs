use std::collections::BTreeSet;

use amiss_wire::extraction::{SourceConstruct, TransclusionKind};
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::uri::scheme;

use crate::discovery::{DocumentStatus, SnapshotDiscovery, followed, site_root, snippet_root};
use crate::route::{HUGO, JEKYLL, directory, join, normalized_path_under, within};

/// The rules the document or its construct selects whatever else the tree
/// declares: a file a Sphinx or Antora page includes reads a relative path
/// from that page, all but an Antora include, which Antora reads from the
/// file that writes it, and then a Sphinx docname in either format, an mkdocs snippet under
/// the directory declaring mkdocs, and a Jekyll or Hugo template under the
/// site of its own generator. None where nothing selects one.
pub(super) fn anchors(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    is_image: bool,
    path_part: &str,
) -> Option<Vec<(Vec<u8>, String)>> {
    if let Some(pages) = snapshot.fragment_pages.get(document)
        && !(adapter == Adapter::AsciiDoc && construct == Some(SourceConstruct::AsciidocInclude))
        && !path_part.is_empty()
        && !path_part.starts_with('/')
        && scheme(path_part).is_none()
    {
        return Some(fragment_anchors(
            snapshot, adapter, pages, construct, is_image, path_part,
        ));
    }
    if matches!(
        construct,
        Some(
            SourceConstruct::RstDocRole
                | SourceConstruct::RstDownloadRole
                | SourceConstruct::RstImageDirective
                | SourceConstruct::RstIncludeDirective
        )
    ) {
        return Some(super::sphinx::anchors(
            snapshot, adapter, document, construct, path_part,
        ));
    }
    if construct == Some(SourceConstruct::MkdocsSnippet) {
        return Some(
            snippet_root(snapshot, adapter, document)
                .map(|root| vec![(root, path_part.to_owned())])
                .unwrap_or_default(),
        );
    }
    if construct == Some(SourceConstruct::MarkdownLiquidLink) {
        return Some(
            site_root(snapshot, document.as_bytes(), &JEKYLL)
                .map_or_else(Vec::new, |root| liquid_spellings(&root, path_part)),
        );
    }
    (construct == Some(SourceConstruct::MarkdownHugoRef))
        .then(|| hugo_anchors(snapshot, document, path_part))
}

/// Whether a document is an Antora page, which renders the partials it
/// includes: a file in the `pages` family of its module.
pub(crate) fn antora_page(snapshot: &SnapshotDiscovery, document: &RepoPath) -> bool {
    let raw = document.as_bytes();
    super::antora_module(snapshot, raw).is_some_and(|(root, module)| {
        within(raw, &join(&join(&join(root, b"modules"), module), b"pages"))
    })
}

/// The files one `AsciiDoc` file includes, each named by an Antora resource ID
/// or a path, and read from the file itself, which is how Antora reads an
/// include nested in a partial.
pub(crate) fn antora_includes(snapshot: &SnapshotDiscovery, file: &RepoPath) -> Vec<RepoPath> {
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
            let (parent, relative) = super::anchors(
                snapshot,
                Adapter::AsciiDoc,
                file,
                Some(SourceConstruct::AsciidocInclude),
                false,
                &entry.target,
            )
            .into_iter()
            .next()
            .unwrap_or_else(|| (directory(file.as_bytes()).to_vec(), entry.target.clone()));
            normalized_path_under(&parent, false, &relative)
                .ok()
                .map(|(path, _)| path)
        })
        .collect()
}

/// A relative reference in a file some pages render is read from each of
/// those pages, and every one of them has to reach its target: the first
/// page whose reading reaches no file answers, so its own target is the one
/// reported missing, and otherwise every reading stands.
fn fragment_anchors(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    pages: &BTreeSet<RepoPath>,
    construct: Option<SourceConstruct>,
    is_image: bool,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    let readings: Vec<Vec<(Vec<u8>, String)>> = pages
        .iter()
        .map(|page| {
            let read = super::anchors(snapshot, adapter, page, construct, is_image, path_part);
            if read.is_empty() {
                vec![(directory(page.as_bytes()).to_vec(), path_part.to_owned())]
            } else {
                read
            }
        })
        .collect();
    let reaches = |reading: &&Vec<(Vec<u8>, String)>| {
        reading.iter().any(|(parent, relative)| {
            normalized_path_under(parent, is_image, relative).is_ok_and(|(path, kind)| {
                snapshot
                    .locate(&super::routed(snapshot, &path, kind))
                    .is_some()
            })
        })
    };
    match readings.iter().find(|reading| !reaches(reading)) {
        Some(unreached) => unreached.clone(),
        None => readings.concat(),
    }
}

/// Where a Liquid `link` or `post_url` tag names a file: under the source of
/// the Jekyll site holding the page, which fails its build on a file it does
/// not hold. A post is named without its extension, so each one Jekyll
/// converts is asked. No site above the page answers nothing, which leaves
/// the reference undecided.
fn liquid_spellings(root: &[u8], path_part: &str) -> Vec<(Vec<u8>, String)> {
    let path = path_part.trim_start_matches('/');
    let post = path
        .strip_prefix("_posts/")
        .map(|post| post.rsplit('/').next().unwrap_or(post));
    let extensions: &[&str] = if post.is_some_and(|name| !name.contains('.')) {
        &POST_EXTENSIONS
    } else {
        &[""]
    };
    extensions
        .iter()
        .map(|extension| (root.to_vec(), format!("{path}{extension}")))
        .collect()
}

const POST_EXTENSIONS: [&str; 3] = [".md", ".markdown", ".html"];

/// Where a Hugo `ref` or `relref` names a page, which fails the build when it
/// names none: beside the page for a relative path, then under the language's
/// content directory and the content directory itself, where a leading slash
/// starts. A path without an extension names a page file or a section's
/// index, and a bare name nothing else answers is the one page the content
/// holds under that name. Hugo compares page paths without case.
fn hugo_anchors(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    let raw = document.as_bytes();
    if path_part.is_empty() {
        return vec![(directory(raw).to_vec(), String::new())];
    }
    let Some((content, language)) = hugo_content(snapshot, raw) else {
        return Vec::new();
    };
    let page = path_part.trim_end_matches('/');
    let written = page.strip_prefix('/').unwrap_or(page);
    let beside = (written.len() == page.len()).then(|| directory(raw).to_vec());
    let spellings = page_spellings(written);
    let mut anchors: Vec<(Vec<u8>, String)> = beside
        .into_iter()
        .chain(language)
        .chain([content.clone()])
        .flat_map(|base| {
            spellings
                .iter()
                .map(move |spelling| (base.clone(), spelling.clone()))
        })
        .collect();
    if !written.contains('/') {
        anchors.extend(named_page(snapshot, &content, &spellings));
    }
    anchors
}

/// The content directory of the Hugo site holding a page, when the page sits
/// under it, and the directory directly under it the page sits in, which is
/// the content of its language on a site that splits them.
fn hugo_content(snapshot: &SnapshotDiscovery, raw: &[u8]) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
    let content = site_root(snapshot, raw, &HUGO)
        .map(|root| join(&root, b"content"))
        .filter(|content| within(raw, content))?;
    let language = raw
        .strip_prefix(content.as_slice())
        .and_then(|rest| rest.strip_prefix(b"/"))
        .and_then(|rest| {
            rest.split(|byte| *byte == b'/')
                .next()
                .filter(|_| rest.contains(&b'/'))
        })
        .map(|segment| join(&content, segment));
    Some((content, language))
}

/// The files a page path names: itself where it carries an extension, and
/// otherwise a page file or a section's index, each as written and lowercased.
fn page_spellings(written: &str) -> Vec<String> {
    let lowered = written.to_lowercase();
    let forms: &[&str] = if written.rsplit('/').next().unwrap_or(written).contains('.') {
        &["{}"]
    } else {
        &HUGO_PAGE_SPELLINGS
    };
    [written, lowered.as_str()]
        .iter()
        .flat_map(|name| forms.iter().map(move |form| form.replace("{}", name)))
        .collect()
}

const HUGO_PAGE_SPELLINGS: [&str; 3] = ["{}.md", "{}/_index.md", "{}/index.md"];

/// The one page under the content directory whose path ends in one of these
/// spellings, which is how Hugo answers a bare name no other reading reaches.
fn named_page(
    snapshot: &SnapshotDiscovery,
    content: &[u8],
    spellings: &[String],
) -> Option<(Vec<u8>, String)> {
    let mut named = snapshot.entries.keys().filter(|path| {
        let raw = path.as_bytes();
        within(raw, content)
            && spellings.iter().any(|spelling| {
                let cut = raw.len().checked_sub(spelling.len());
                cut.and_then(|cut| raw.get(cut..))
                    .is_some_and(|tail| tail.eq_ignore_ascii_case(spelling.as_bytes()))
                    && cut.and_then(|cut| raw.get(cut.checked_sub(1)?)) == Some(&b'/')
            })
    });
    let only = named.next()?;
    named.next().is_none().then(|| {
        (
            Vec::new(),
            String::from_utf8_lossy(only.as_bytes()).into_owned(),
        )
    })
}
