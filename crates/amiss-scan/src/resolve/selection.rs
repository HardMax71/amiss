use amiss_wire::extraction::SourceConstruct;
use amiss_wire::model::RepoPath;
use amiss_wire::resolution::{Missing, Resolution, Target};
use amiss_wire::uri::decode_component;
use memchr::memmem;

use crate::Error;
use crate::resources::Aggregate;

use super::Resolver;
use super::content::{Content, content_cache, read_target};

/// The file and the marker name one include selects by, where the include's
/// grammar selects part of a file by a marker the file carries rather than by
/// its lines: an mdBook anchor, a snippet section, an `AsciiDoc` tag, or the
/// text a `literalinclude` starts or ends at. A line selection stays the line
/// fragment every other destination is read by.
pub(super) fn marked(construct: SourceConstruct, semantic: &str) -> Option<(&str, Marked)> {
    let marker = [
        (SourceConstruct::MdbookInclude, Marker::Anchor),
        (SourceConstruct::MkdocsSnippet, Marker::Section),
        (SourceConstruct::AsciidocInclude, Marker::Tag),
        (SourceConstruct::RstIncludeDirective, Marker::Text),
    ]
    .into_iter()
    .find_map(|(owner, marker)| (owner == construct).then_some(marker))?;
    let (path, fragment) = semantic.split_once('#')?;
    let (marker, fragment) = match marker {
        Marker::Text => {
            if let Some(name) = fragment.strip_prefix("pyobject=") {
                (Marker::Object, name)
            } else {
                (Marker::Text, fragment.strip_prefix("text=")?)
            }
        }
        Marker::Anchor | Marker::Section | Marker::Tag | Marker::Object => (marker, fragment),
    };
    let lines = fragment
        .strip_prefix('L')
        .is_some_and(|rest| rest.starts_with(|first: char| first.is_ascii_digit()));
    let mut decoded = Vec::with_capacity(fragment.len());
    decode_component(fragment, &mut decoded, |_byte| None).ok()?;
    let name = String::from_utf8(decoded).ok()?;
    (!lines && !name.is_empty()).then_some((path, Marked { marker, name }))
}

/// The first line alone of a line range an include selects, since every
/// include grammar stops a range at the end of the file rather than refusing
/// it, and only a range starting past the end selects nothing.
pub(super) fn opening_line(construct: SourceConstruct, semantic: &str) -> Option<String> {
    let included = matches!(
        construct,
        SourceConstruct::MdbookInclude
            | SourceConstruct::MkdocsSnippet
            | SourceConstruct::AsciidocInclude
            | SourceConstruct::RstIncludeDirective
    );
    let (path, fragment) = semantic.split_once('#').filter(|_| included)?;
    let (first, _last) = fragment.strip_prefix('L')?.split_once("-L")?;
    Some(format!("{path}#L{first}"))
}

/// How one include grammar marks the part of a file it selects.
#[derive(Clone, Copy)]
pub(super) enum Marker {
    Anchor,
    Section,
    Tag,
    Text,
    Object,
}

/// The marker one include selects by, and the name it spells.
pub(super) struct Marked {
    pub(super) marker: Marker,
    pub(super) name: String,
}

impl Resolver<'_> {
    /// A selection answered by the file the include names: the resolution the
    /// file alone earned where it earned no blob, and otherwise that blob or,
    /// where the file carries no marker of that name, the selection missing.
    pub(super) fn selected(
        &mut self,
        resolution: Resolution<RepoPath>,
        marked: &Marked,
    ) -> Result<Resolution<RepoPath>, Error> {
        let Resolution::Resolved {
            target: Target::Blob(blob),
        } = &resolution
        else {
            return Ok(resolution);
        };
        let Some((mode, oid)) = self
            .snapshot
            .entries
            .get(blob.path.as_bytes())
            .map(|(mode, oid)| (*mode, oid.clone()))
        else {
            return Ok(resolution);
        };
        read_target(self, &blob.path, mode, &oid)?;
        let Some(Content::Ordinary { body, .. }) =
            content_cache(self.cache, self.commit_oid.as_ref())
                .get(&blob.path)
                .map(|cached| &cached.content)
        else {
            return Ok(resolution);
        };
        let carried = carries(marked.marker, body, marked.name.as_bytes());
        self.scan.charge(
            Aggregate::LineFragmentBytes,
            u64::try_from(body.len()).unwrap_or(u64::MAX),
        )?;
        if carried {
            return Ok(resolution);
        }
        Ok(Resolution::Missing(Missing::SelectionNotFound {
            path: blob.path.clone(),
        }))
    }
}

/// Whether a file carries the marker one include grammar selects by, spelled
/// the way that grammar reads it.
fn carries(marker: Marker, body: &[u8], name: &[u8]) -> bool {
    let word = |byte: &u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-');
    match marker {
        Marker::Anchor => memmem::find_iter(body, b"ANCHOR:").any(|at| {
            let rest = body.get(at.saturating_add(7)..).unwrap_or_default();
            let rest = rest.trim_ascii_start();
            rest.strip_prefix(name)
                .is_some_and(|after| !after.first().is_some_and(word))
        }),
        Marker::Section => memmem::find(body, &[b"--8<-- [start:", name, b"]"].concat()).is_some(),
        Marker::Tag => memmem::find(body, &[b"tag::", name, b"[]"].concat()).is_some(),
        Marker::Text => memmem::find(body, name).is_some(),
        Marker::Object => {
            let last = name.rsplit(|byte| *byte == b'.').next().unwrap_or(name);
            [b"def ".as_slice(), b"class ".as_slice()]
                .iter()
                .any(|keyword| {
                    memmem::find_iter(body, &[keyword, last].concat()).any(|at| {
                        let after = at.saturating_add(keyword.len()).saturating_add(last.len());
                        !body.get(after).is_some_and(word)
                    })
                })
        }
    }
}
