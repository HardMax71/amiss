use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use amiss_git::{GitLimits, GitResources, Repository, parse_index_file};
use amiss_scan::report::Built;
use amiss_wire::json::Value;
use amiss_wire::model::ObjectFormat;

struct Fix {
    start: usize,
    end: usize,
    replacement: String,
}

enum DocumentOutcome {
    Applied(usize),
    AlreadyApplied(usize),
    Refused(&'static str),
}

/// Applies the evaluation's fixes to the working tree and reports each
/// document's outcome. Exit 0 when every carried fix is applied or already
/// present, 1 when any document refused, 2 when the evaluation itself could
/// not be trusted and nothing was touched.
#[expect(
    clippy::print_stdout,
    reason = "the repair rows are the command's output"
)]
pub(crate) fn run(
    worktree: &Path,
    repo: &Repository,
    object_format: ObjectFormat,
    built: &Built,
) -> ExitCode {
    if built.exit_code == 2 {
        println!("amiss fix: the evaluation could not be trusted; nothing applied");
        return ExitCode::from(2);
    }
    let (fixes, bare) = collect(&built.envelope);
    if fixes.is_empty() {
        println!("amiss fix: no fixes to apply; {bare} findings carry none");
        return ExitCode::SUCCESS;
    }
    let Ok(staged) = staged_blobs(repo, object_format, &fixes) else {
        println!("amiss fix: the staged index could not be read; nothing applied");
        return ExitCode::from(2);
    };
    let mut applied = 0_usize;
    let mut present = 0_usize;
    let mut refused = 0_usize;
    for (document, rows) in &fixes {
        let outcome = repair_document(worktree, document, rows, staged.get(document));
        match outcome {
            DocumentOutcome::Applied(count) => {
                applied = applied.saturating_add(count);
                println!("fixed {document}: {count} replacement(s)");
            }
            DocumentOutcome::AlreadyApplied(count) => {
                present = present.saturating_add(count);
                println!("already fixed {document}: {count} replacement(s) present");
            }
            DocumentOutcome::Refused(reason) => {
                refused = refused.saturating_add(1);
                println!("refused {document}: {reason}");
            }
        }
    }
    println!(
        "amiss fix: {applied} applied, {present} already present, {refused} refused, \
         {bare} findings carry no fix"
    );
    if refused > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn member<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let Value::Object(members) = value else {
        return None;
    };
    members
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, member)| member)
}

fn text(value: &Value) -> Option<&str> {
    let Value::String(text) = value else {
        return None;
    };
    Some(text)
}

fn byte_offset(value: &Value) -> Option<usize> {
    let Value::Integer(offset) = value else {
        return None;
    };
    usize::try_from(*offset).ok()
}

/// Every finding's fix, grouped by the document it edits, beside the count
/// of findings that carry none.
fn collect(envelope: &Value) -> (BTreeMap<String, Vec<Fix>>, usize) {
    let mut fixes: BTreeMap<String, Vec<Fix>> = BTreeMap::new();
    let mut bare = 0_usize;
    let rows = member(envelope, "payload")
        .and_then(|payload| member(payload, "findings"))
        .and_then(|findings| {
            if let Value::Array(rows) = findings {
                Some(rows)
            } else {
                None
            }
        });
    for row in rows.into_iter().flatten() {
        let Some(fix) = member(row, "fix") else {
            continue;
        };
        let parsed = member(fix, "path").and_then(text).zip(
            member(fix, "span")
                .and_then(|span| {
                    member(span, "start_byte")
                        .and_then(byte_offset)
                        .zip(member(span, "end_byte").and_then(byte_offset))
                })
                .zip(member(fix, "replacement").and_then(text)),
        );
        match parsed {
            Some((document, ((start, end), replacement))) => {
                fixes.entry(document.to_owned()).or_default().push(Fix {
                    start,
                    end,
                    replacement: replacement.to_owned(),
                });
            }
            None => bare = bare.saturating_add(1),
        }
    }
    (fixes, bare)
}

/// The staged blob bytes for every document a fix names, read through the
/// same index the evaluation read.
fn staged_blobs(
    repo: &Repository,
    object_format: ObjectFormat,
    fixes: &BTreeMap<String, Vec<Fix>>,
) -> Result<BTreeMap<String, Vec<u8>>, ()> {
    let mut resources = GitResources::new(GitLimits::default());
    let bytes = repo
        .read_index_bytes(&mut resources)
        .map_err(|_defect| ())?;
    let index = parse_index_file(object_format, &bytes).map_err(|_defect| ())?;
    let mut staged = BTreeMap::new();
    for entry in &index.entries {
        let Ok(path) = std::str::from_utf8(&entry.path) else {
            continue;
        };
        if !fixes.contains_key(path) {
            continue;
        }
        let object = repo
            .read_object(&mut resources, &entry.oid)
            .map_err(|_defect| ())?;
        staged.insert(path.to_owned(), object.body);
    }
    Ok(staged)
}

fn repair_document(
    worktree: &Path,
    document: &str,
    rows: &[Fix],
    staged: Option<&Vec<u8>>,
) -> DocumentOutcome {
    let Some(staged) = staged else {
        return DocumentOutcome::Refused("not in the staged index");
    };
    let Some(repaired) = splice(staged, rows) else {
        return DocumentOutcome::Refused("overlapping or out-of-range spans");
    };
    let path = worktree.join(document);
    let regular = fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_file());
    if !regular {
        return DocumentOutcome::Refused("not a regular worktree file");
    }
    let Ok(current) = fs::read(&path) else {
        return DocumentOutcome::Refused("unreadable in the worktree");
    };
    if current == repaired {
        return DocumentOutcome::AlreadyApplied(rows.len());
    }
    if current != *staged {
        return DocumentOutcome::Refused("worktree differs from the staged bytes the fixes name");
    }
    if fs::write(&path, &repaired).is_err() {
        return DocumentOutcome::Refused("could not be written");
    }
    DocumentOutcome::Applied(rows.len())
}

/// The spans replaced back to front so earlier offsets stay true; two spans
/// that touch are one edit nobody proved, so they refuse together.
fn splice(source: &[u8], rows: &[Fix]) -> Option<Vec<u8>> {
    let mut ordered: Vec<&Fix> = rows.iter().collect();
    ordered.sort_by_key(|fix| (fix.start, fix.end));
    for pair in ordered.windows(2) {
        let [first, second] = pair else {
            return None;
        };
        if first.end > second.start {
            return None;
        }
    }
    let mut repaired = source.to_vec();
    for fix in ordered.iter().rev() {
        if fix.start > fix.end || fix.end > repaired.len() {
            return None;
        }
        repaired.splice(fix.start..fix.end, fix.replacement.bytes());
    }
    Some(repaired)
}

#[path = "../tests/internal/repair.rs"]
mod tests;
