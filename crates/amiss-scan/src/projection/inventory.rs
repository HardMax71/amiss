use std::ops::Bound;

use amiss_wire::controls::{GitMode, TreePathSelection};
use amiss_wire::digest::hb_stream;
use amiss_wire::model::RepoPath;

use crate::discovery::{Located, SnapshotDiscovery};
use crate::scan::SemanticCodeSink;

use super::{DriftReason, Verdict, unavailable};

const SOURCE_DOMAIN: &str = "amiss/scanner-sorted-rows-source";

fn selected_rows<'a>(
    discovery: &'a SnapshotDiscovery,
    selection: &TreePathSelection,
) -> Result<Vec<&'a str>, DriftReason> {
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
    let mut rows = Vec::new();
    for (path, (mode, _oid)) in discovery
        .entries
        .range::<[u8], _>((Bound::Included(prefix.as_slice()), Bound::Unbounded))
    {
        let Some(relative) = path.as_bytes().strip_prefix(prefix.as_slice()) else {
            break;
        };
        match mode {
            GitMode::Tree => continue,
            GitMode::RegularFile
            | GitMode::ExecutableFile
            | GitMode::Symlink
            | GitMode::Gitlink => {}
        }
        let depth = u64::try_from(relative.split(|byte| *byte == b'/').count()).unwrap_or(u64::MAX);
        if depth > selection.maximum_depth
            || selection
                .suffix
                .as_ref()
                .is_some_and(|suffix| !relative.ends_with(suffix.as_bytes()))
        {
            continue;
        }
        let row =
            std::str::from_utf8(relative).map_err(|_invalid| DriftReason::SourceTreePathNotUtf8)?;
        if row.chars().any(char::is_control) {
            return Err(DriftReason::SourceTreePathNotARow);
        }
        rows.push(row);
    }
    Ok(rows)
}

fn projected_bytes(rows: &[&str]) -> u64 {
    let separators = rows.len().saturating_sub(1);
    rows.iter().fold(
        u64::try_from(separators).unwrap_or(u64::MAX),
        |total, row| total.saturating_add(u64::try_from(row.len()).unwrap_or(u64::MAX)),
    )
}

fn projected_digest(rows: &[&str]) -> amiss_wire::digest::Digest {
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

pub(super) fn evaluate(
    discovery: &SnapshotDiscovery,
    selection: &TreePathSelection,
    sink: &SemanticCodeSink,
) -> Verdict {
    let rows = match selected_rows(discovery, selection) {
        Ok(rows) => rows,
        Err(reason) => return unavailable(reason, sink),
    };
    if rows_match(&rows, &sink.value) {
        return Verdict::Attested;
    }
    Verdict::Drift {
        reason: DriftReason::ContentDiffers,
        expected_digest: Some(projected_digest(&rows)),
        observed_digest: Some(sink.digest),
        expected_bytes: Some(projected_bytes(&rows)),
        observed_bytes: Some(u64::try_from(sink.value.len()).unwrap_or(u64::MAX)),
    }
}
