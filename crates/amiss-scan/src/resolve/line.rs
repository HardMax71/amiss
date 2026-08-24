use amiss_md::lines::scan;
use amiss_wire::controls::GitMode;
use amiss_wire::digest::hb;
use amiss_wire::model::{ForgeDialect, RepoPath};
use amiss_wire::resolution::{BlobContent, BlobTarget, Missing, Target, UnsupportedSemantics};

use crate::Error;
use crate::resources::Aggregate;

use super::content::{Content, content_cache, read_target, target_projection};
use super::{RAW_EVIDENCE_DOMAIN, Resolution, Resolver, TARGET_LINE_PROJECTION_DOMAIN};

const MAX_SAFE: u64 = 9_007_199_254_740_991;

impl Resolver<'_> {
    /// Answers one value claim against the snapshot: the target must be a
    /// readable regular blob whose claimed line is exactly the expected text.
    ///
    /// # Errors
    ///
    /// A Git defect or a crossed resource ceiling while reading the target.
    pub fn resolve_claim(
        &mut self,
        claim: &crate::claim::ValueClaim,
    ) -> Result<crate::claim::ClaimVerdict, Error> {
        use crate::claim::{ClaimMissingReason, ClaimVerdict};

        let Some((mode, oid)) = self
            .snapshot
            .entries
            .get(claim.path.as_bytes())
            .map(|(mode, oid)| (*mode, oid.clone()))
        else {
            return Ok(ClaimVerdict::TargetMissing(ClaimMissingReason::Absent));
        };
        match mode {
            GitMode::Tree | GitMode::Gitlink | GitMode::Symlink => {
                return Ok(ClaimVerdict::TargetMissing(ClaimMissingReason::NotABlob));
            }
            GitMode::RegularFile | GitMode::ExecutableFile => {}
        }
        let evidence = read_target(self, &claim.path, mode, &oid)?;
        if matches!(evidence, BlobContent::LfsPointer { .. }) {
            return Ok(ClaimVerdict::TargetMissing(ClaimMissingReason::LfsPointer));
        }
        let Some(cached) = content_cache(self.cache, self.commit_oid.as_ref()).get_mut(&claim.path)
        else {
            return Err(Error::Internal);
        };
        if cached.mode != mode || cached.content.evidence() != evidence {
            return Err(Error::Internal);
        }
        let Content::Ordinary {
            body,
            line_projections,
            ..
        } = &mut cached.content
        else {
            return Err(Error::Internal);
        };
        let range = LineRange {
            first: claim.line,
            last: claim.line,
        };
        if let std::collections::btree_map::Entry::Vacant(slot) = line_projections.entry(range) {
            self.scan.charge(
                Aggregate::LineFragmentBytes,
                u64::try_from(body.len()).unwrap_or(u64::MAX),
            )?;
            let projection = selected_line_bytes(body, range).map(|selected| {
                target_projection(
                    TARGET_LINE_PROJECTION_DOMAIN,
                    mode,
                    hb(RAW_EVIDENCE_DOMAIN, selected),
                )
            });
            slot.insert(projection);
        }
        let Some(selected) = selected_line_bytes(body, range) else {
            return Ok(ClaimVerdict::TargetMissing(
                ClaimMissingReason::LineOutOfRange,
            ));
        };
        let observed = line_content(selected);
        if observed == claim.expected.as_bytes() {
            Ok(ClaimVerdict::Attested)
        } else {
            Ok(ClaimVerdict::Broken {
                observed_digest: hb(RAW_EVIDENCE_DOMAIN, observed),
                observed: observed.to_vec(),
            })
        }
    }
}

/// One line without the terminator that ended it.
fn line_content(selected: &[u8]) -> &[u8] {
    selected
        .strip_suffix(b"\r\n")
        .or_else(|| selected.strip_suffix(b"\n"))
        .or_else(|| selected.strip_suffix(b"\r"))
        .unwrap_or(selected)
}

/// An inclusive, one-indexed selection of raw source lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LineRange {
    first: u64,
    last: u64,
}

pub(super) fn line_resolution(
    resolver: &mut Resolver<'_>,
    path: &RepoPath,
    mode: GitMode,
    mut blob: BlobTarget<RepoPath>,
    range: LineRange,
) -> Result<Resolution, Error> {
    let Some(cached) = content_cache(resolver.cache, resolver.commit_oid.as_ref()).get_mut(path)
    else {
        return Err(Error::Internal);
    };
    if cached.mode != mode || cached.content.evidence() != blob.content {
        return Err(Error::Internal);
    }
    let Content::Ordinary {
        body,
        line_projections,
        ..
    } = &mut cached.content
    else {
        return Ok(Resolution::UnsupportedSemantics(
            UnsupportedSemantics::CodeFragment(Target::Blob(blob)),
        ));
    };

    let projection = if let Some(cached) = line_projections.get(&range).copied() {
        cached
    } else {
        resolver.scan.charge(
            Aggregate::LineFragmentBytes,
            u64::try_from(body.len()).unwrap_or(u64::MAX),
        )?;
        let projection = selected_line_bytes(body, range).map(|selected| {
            target_projection(
                TARGET_LINE_PROJECTION_DOMAIN,
                mode,
                hb(RAW_EVIDENCE_DOMAIN, selected),
            )
        });
        line_projections.insert(range, projection);
        projection
    };

    let Some(projection_digest) = projection else {
        return Ok(Resolution::Missing(Missing::LineFragmentOutOfRange {
            path: path.clone(),
        }));
    };
    let BlobContent::Available { raw_digest, .. } = blob.content else {
        return Err(Error::Internal);
    };
    blob.content = BlobContent::Available {
        raw_digest,
        projection_digest,
    };
    Ok(Resolution::Resolved(Target::Blob(blob)))
}

/// One safe line number: nonzero first digit, at most sixteen digits, and
/// within the range every consumer of the report can hold exactly.
pub(crate) fn safe_line_number(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > 16 || bytes.first() == Some(&b'0') {
        return None;
    }
    if !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    text.parse::<u64>().ok().filter(|value| *value <= MAX_SAFE)
}

/// Line-fragment syntax after one decode, in the dialect's spelling:
/// `L<n>` alone, or the range form the forge renders. Each number safe, and
/// a range end at least its start. A native reference uses the declared run
/// dialect, falling back to the GitHub/Gitea spelling when no forge context
/// exists.
pub(super) fn line_fragment(forge: Option<ForgeDialect>, decoded: &str) -> Option<LineRange> {
    let number = safe_line_number;
    let rest = decoded.strip_prefix('L')?;
    let range = match forge {
        None | Some(ForgeDialect::Github | ForgeDialect::Gitea) => rest.split_once("-L"),
        Some(ForgeDialect::Gitlab) => rest.split_once('-'),
    };
    match range {
        None => number(rest).map(|line| LineRange {
            first: line,
            last: line,
        }),
        Some((start, end)) => match (number(start), number(end)) {
            (Some(first), Some(last)) if last >= first => Some(LineRange { first, last }),
            _ => None,
        },
    }
}

/// Returns the exact byte span from the first selected line through the last,
/// including every original CRLF, bare CR, or LF terminator. The shared line
/// scanner deliberately does not synthesize an empty line after a final
/// terminator, so a range beyond the bytes is absent rather than empty.
fn selected_line_bytes(source: &[u8], range: LineRange) -> Option<&[u8]> {
    let mut line_number = 0_u64;
    let mut selection_start = None;
    for line in scan(source) {
        line_number = line_number.saturating_add(1);
        if line_number == range.first {
            selection_start = Some(line.start);
        }
        if line_number == range.last {
            return source.get(selection_start?..line.end);
        }
    }
    None
}
