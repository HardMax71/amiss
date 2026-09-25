use std::fs;

use amiss_git::object::verify_oid;
use amiss_wire::controls::{ContentAvailability, GitMode};
use amiss_wire::human::atom;
use amiss_wire::model::RepoPath;
use amiss_wire::report::model::{Evaluation, ReportPayload};

use crate::invocation::{CandidateSelector, Invocation, Verb};

/// The bytes Git stages for a CRLF working copy under `core.autocrlf` or
/// `eol=crlf`, which is what a clean `git status` compares.
fn crlf_cleaned(copy: &[u8]) -> Vec<u8> {
    let mut cleaned = Vec::with_capacity(copy.len());
    let mut bytes = copy.iter().peekable();
    while let Some(byte) = bytes.next() {
        if *byte != b'\r' || bytes.peek() != Some(&&b'\n') {
            cleaned.push(*byte);
        }
    }
    cleaned
}

/// How many differing documents the note names before it counts the rest.
const NAMED: usize = 5;

/// The staged documents whose working copies hold other bytes, which a staged
/// check never reads, named on the diagnostics channel so a pass over stale
/// staging does not read as a pass over the edit. Only a document the check
/// read is compared, by size first and hashed only when the sizes agree.
/// Under a sparse checkout an absent copy is the checkout's choice, so only a
/// present one is compared.
#[expect(clippy::print_stderr, reason = "the contract diagnostics channel")]
pub(crate) fn unstaged<R, E>(
    invocation: &Invocation,
    payload: &ReportPayload<RepoPath, R, GitMode, E>,
) {
    let Evaluation::Resolved(evaluation) = &payload.evaluation else {
        return;
    };
    if invocation.verb != Verb::Check || invocation.candidate != CandidateSelector::Index {
        return;
    }
    let (worktree, sparse) = (
        invocation.repo.as_path(),
        evaluation.skip_worktree_paths > 0,
    );
    let differing: Vec<String> = payload
        .documents
        .iter()
        .filter_map(|row| {
            let side = row.candidate.as_ref()?;
            let path = row.path.as_str()?;
            if side.content_availability != ContentAvailability::Available {
                return None;
            }
            let copy = worktree.join(path);
            let Ok(metadata) = fs::symlink_metadata(&copy) else {
                return (!sparse).then(|| atom(path));
            };
            let same = metadata.is_file()
                && fs::read(&copy).is_ok_and(|bytes| {
                    let cleaned;
                    let staged_form = if u64::try_from(bytes.len()) == Ok(side.byte_count) {
                        &bytes
                    } else {
                        cleaned = crlf_cleaned(&bytes);
                        &cleaned
                    };
                    let header = format!("blob {}\0", staged_form.len());
                    verify_oid(
                        invocation.object_format,
                        &side.entry_oid,
                        header.as_bytes(),
                        staged_form,
                    )
                    .is_ok()
                });
            (!same).then(|| atom(path))
        })
        .collect();
    if differing.is_empty() {
        return;
    }
    let rest = differing.len().saturating_sub(NAMED);
    eprintln!(
        "amiss: note: {} staged documents differ from their working copies, which --index does not read; git add them to check them: {}{}",
        differing.len(),
        differing.get(..NAMED).unwrap_or(&differing).join(", "),
        if rest > 0 {
            format!(" and {rest} more")
        } else {
            String::new()
        },
    );
}
