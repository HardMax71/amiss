use std::cmp::Ordering;
use std::ops::Bound;

use amiss_wire::controls::{GitMode, ProjectionKind, TreePathSelection};
use amiss_wire::digest::{Digest, hb, hb_stream};
use amiss_wire::model::RepoPath;

use crate::Error;
use crate::discovery::{Located, SnapshotDiscovery};
use crate::scan::SemanticCodeSink;

use super::{Difference, DriftReason, RowDifference, Verdict, unavailable};

const SOURCE_DOMAIN: &str = "amiss/scanner-sorted-rows-source";
const COUNT_SOURCE_DOMAIN: &str = "amiss/scanner-decimal-count-source";
const PREVIEW_ROWS_PER_SIDE: usize = 32;
const PREVIEW_BYTES_PER_SIDE: usize = 32 * 1024;

struct Preview {
    rows: Vec<String>,
    bytes: usize,
    closed: bool,
}

fn preview_row(preview: &mut Preview, row: &str) {
    if preview.closed {
        return;
    }
    let bytes = preview.bytes.saturating_add(row.len());
    if preview.rows.len() == PREVIEW_ROWS_PER_SIDE || bytes > PREVIEW_BYTES_PER_SIDE {
        preview.closed = true;
        return;
    }
    preview.rows.push(row.to_owned());
    preview.bytes = bytes;
}

fn selected_paths<'a>(
    discovery: &'a SnapshotDiscovery,
    selection: &'a TreePathSelection,
) -> Result<impl Iterator<Item = &'a [u8]> + 'a, DriftReason> {
    let root = RepoPath::from(&selection.root);
    let root_bytes = root.as_bytes();
    match discovery.locate(&root) {
        Some(Located::Entry(GitMode::Tree, _) | Located::ImpliedTree) => {}
        None => return Err(DriftReason::SourceTreeRootAbsent),
        Some(Located::Entry(
            GitMode::RegularFile | GitMode::ExecutableFile | GitMode::Symlink | GitMode::Gitlink,
            _,
        )) => return Err(DriftReason::SourceTreeRootNotATree),
    }

    let mut prefix = root_bytes.to_vec();
    prefix.push(b'/');
    let maximum_depth = selection.maximum_depth;
    let suffix = selection.suffix.as_deref().map(str::as_bytes);
    Ok(discovery
        .entries
        .range::<[u8], _>((Bound::Included(prefix.as_slice()), Bound::Unbounded))
        .map_while(move |(path, (mode, _oid))| {
            path.as_bytes()
                .strip_prefix(prefix.as_slice())
                .map(|relative| (relative, *mode))
        })
        .filter_map(move |(relative, mode)| {
            let depth =
                u64::try_from(relative.split(|byte| *byte == b'/').count()).unwrap_or(u64::MAX);
            let included_mode = match mode {
                GitMode::Tree => false,
                GitMode::RegularFile
                | GitMode::ExecutableFile
                | GitMode::Symlink
                | GitMode::Gitlink => true,
            };
            (included_mode
                && depth <= maximum_depth
                && suffix.is_none_or(|tail| relative.ends_with(tail)))
            .then_some(relative)
        }))
}

fn selected_rows<'a>(paths: impl Iterator<Item = &'a [u8]>) -> Result<Vec<&'a str>, DriftReason> {
    paths
        .map(|path| {
            let row =
                std::str::from_utf8(path).map_err(|_invalid| DriftReason::SourceTreePathNotUtf8)?;
            if row.chars().any(char::is_control) {
                return Err(DriftReason::SourceTreePathNotARow);
            }
            Ok(row)
        })
        .collect()
}

fn projected_bytes(rows: &[&str]) -> u64 {
    let separators = rows.len().saturating_sub(1);
    rows.iter().fold(
        u64::try_from(separators).unwrap_or(u64::MAX),
        |total, row| total.saturating_add(u64::try_from(row.len()).unwrap_or(u64::MAX)),
    )
}

fn projected_digest(rows: &[&str]) -> Digest {
    hb_stream(SOURCE_DOMAIN, |write| {
        for (index, row) in rows.iter().enumerate() {
            if index != 0 {
                write(b"\n");
            }
            write(row.as_bytes());
        }
    })
}

fn rows_match(rows: &[&str], observed: &str) -> bool {
    if rows.is_empty() {
        return observed.is_empty();
    }
    !observed.is_empty() && rows.iter().copied().eq(observed.split('\n'))
}

fn difference(rows: &[&str], observed: &str) -> RowDifference {
    let mut observed_rows: Vec<&str> = if observed.is_empty() {
        Vec::new()
    } else {
        observed.split('\n').collect()
    };
    observed_rows.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));

    let mut expected_at = 0_usize;
    let mut observed_at = 0_usize;
    let mut missing_records = 0_u64;
    let mut extra_records = 0_u64;
    let mut missing = Preview {
        rows: Vec::new(),
        bytes: 0,
        closed: false,
    };
    let mut extra = Preview {
        rows: Vec::new(),
        bytes: 0,
        closed: false,
    };
    while let (Some(expected), Some(actual)) =
        (rows.get(expected_at), observed_rows.get(observed_at))
    {
        match expected.as_bytes().cmp(actual.as_bytes()) {
            Ordering::Less => {
                missing_records = missing_records.saturating_add(1);
                preview_row(&mut missing, expected);
                expected_at = expected_at.saturating_add(1);
            }
            Ordering::Equal => {
                expected_at = expected_at.saturating_add(1);
                observed_at = observed_at.saturating_add(1);
            }
            Ordering::Greater => {
                extra_records = extra_records.saturating_add(1);
                preview_row(&mut extra, actual);
                observed_at = observed_at.saturating_add(1);
            }
        }
    }
    for row in rows.get(expected_at..).unwrap_or_default() {
        missing_records = missing_records.saturating_add(1);
        preview_row(&mut missing, row);
    }
    for row in observed_rows.get(observed_at..).unwrap_or_default() {
        extra_records = extra_records.saturating_add(1);
        preview_row(&mut extra, row);
    }
    RowDifference {
        ordering_only: missing_records == 0 && extra_records == 0,
        expected_records: u64::try_from(rows.len()).unwrap_or(u64::MAX),
        observed_records: u64::try_from(observed_rows.len()).unwrap_or(u64::MAX),
        missing_records,
        extra_records,
        missing_omitted: missing_records
            .saturating_sub(u64::try_from(missing.rows.len()).unwrap_or(u64::MAX)),
        extra_omitted: extra_records
            .saturating_sub(u64::try_from(extra.rows.len()).unwrap_or(u64::MAX)),
        missing_preview: missing.rows,
        extra_preview: extra.rows,
    }
}

fn canonical_count(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    if bytes == b"0" {
        return Some(0);
    }
    if !matches!(bytes.first(), Some(b'1'..=b'9'))
        || !bytes
            .get(1..)
            .unwrap_or_default()
            .iter()
            .all(u8::is_ascii_digit)
    {
        return None;
    }
    value.parse().ok().filter(|count| {
        u64::try_from(amiss_wire::json::MAX_SAFE_INTEGER).is_ok_and(|maximum| *count <= maximum)
    })
}

fn mismatch(
    sink: &SemanticCodeSink,
    expected_digest: Digest,
    expected_bytes: u64,
    difference: Difference,
) -> Verdict {
    Verdict::Drift {
        reason: DriftReason::ContentDiffers,
        expected_digest: Some(expected_digest),
        observed_digest: Some(sink.digest),
        expected_bytes: Some(expected_bytes),
        observed_bytes: Some(u64::try_from(sink.value.len()).unwrap_or(u64::MAX)),
        difference: Some(difference),
    }
}

pub(super) fn evaluate(
    discovery: &SnapshotDiscovery,
    selection: &TreePathSelection,
    projection: ProjectionKind,
    sink: &SemanticCodeSink,
) -> Result<Verdict, Error> {
    let paths = match selected_paths(discovery, selection) {
        Ok(paths) => paths,
        Err(reason) => return Ok(unavailable(reason, sink)),
    };
    match projection {
        ProjectionKind::SortedRowsV1 => {
            let rows = match selected_rows(paths) {
                Ok(rows) => rows,
                Err(reason) => return Ok(unavailable(reason, sink)),
            };
            if rows_match(&rows, &sink.value) {
                return Ok(Verdict::Attested);
            }
            Ok(mismatch(
                sink,
                projected_digest(&rows),
                projected_bytes(&rows),
                Difference::Rows(Box::new(difference(&rows, &sink.value))),
            ))
        }
        ProjectionKind::DecimalCountV1 => {
            let expected_count = u64::try_from(paths.count()).unwrap_or(u64::MAX);
            let expected = expected_count.to_string();
            if expected == sink.value {
                return Ok(Verdict::Attested);
            }
            Ok(mismatch(
                sink,
                hb(COUNT_SOURCE_DOMAIN, expected.as_bytes()),
                u64::try_from(expected.len()).unwrap_or(u64::MAX),
                Difference::Count {
                    expected_count,
                    observed_count: canonical_count(&sink.value),
                },
            ))
        }
        ProjectionKind::CodeTextV1 => Err(Error::Internal),
    }
}
