use std::fs;
use std::process::ExitCode;

use amiss_scan::claim::{GovernedForm, classify};
use amiss_wire::model::Adapter;

use crate::invocation::AuthorInvocation;

/// Prints one ready value-claim definition for the named line, proven by
/// running the printed bytes back through the markdown extractor and the
/// claim grammar before anything reaches stdout.
#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the definition is the command's output and refusals are diagnostics"
)]
pub(crate) fn run(author: &AuthorInvocation) -> ExitCode {
    let bytes = match fs::read(author.repo.join(&author.path)) {
        Ok(bytes) => bytes,
        Err(_defect) => {
            eprintln!(
                "amiss claim: {} is unreadable under the repo root",
                author.path
            );
            return ExitCode::FAILURE;
        }
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        eprintln!(
            "amiss claim: {} is not UTF-8, so no line can be quoted",
            author.path
        );
        return ExitCode::FAILURE;
    };
    let Ok(wanted) = usize::try_from(author.line) else {
        eprintln!("amiss claim: the line number does not fit this platform");
        return ExitCode::FAILURE;
    };
    let Some(line) = text.split_inclusive('\n').nth(wanted.saturating_sub(1)) else {
        let held = text.split_inclusive('\n').count();
        eprintln!(
            "amiss claim: {} holds {held} lines and L{} is past them",
            author.path, author.line
        );
        return ExitCode::FAILURE;
    };
    let expected = line.trim_end_matches('\n').trim_end_matches('\r');
    // Double quotes first, the single-quoted title second for lines that
    // hold double quotes themselves; each spelling must survive the round
    // trip before it reaches stdout.
    let spellings = [
        format!(
            "[amiss:{}]: <amiss:value?path={}&line=L{}> \"{expected}\"",
            author.name, author.path, author.line
        ),
        format!(
            "[amiss:{}]: <amiss:value?path={}&line=L{}> '{expected}'",
            author.name, author.path, author.line
        ),
    ];
    for definition in &spellings {
        if round_trips(definition, author, expected) {
            println!("{definition}");
            return ExitCode::SUCCESS;
        }
    }
    eprintln!(
        "amiss claim: the line cannot be spelled into the claim grammar; neither title \
         quoting survives extraction, so pick another line"
    );
    ExitCode::FAILURE
}

/// The printed bytes must extract into exactly the claim the flags named.
fn round_trips(definition: &str, author: &AuthorInvocation, expected: &str) -> bool {
    let document = format!("{definition}\n");
    let Ok(analysis) = amiss_md::analyze(Adapter::Markdown, document.as_bytes(), u64::MAX) else {
        return false;
    };
    let Some(extraction) = analysis.extraction else {
        return false;
    };
    let [definition] = extraction.governed.as_slice() else {
        return false;
    };
    match classify(definition) {
        GovernedForm::Value(claim) => {
            claim.name == author.name
                && claim.line == author.line
                && claim.expected == expected
                && claim.path.as_str() == Some(author.path.as_str())
        }
        GovernedForm::Unknown => false,
    }
}

#[path = "../tests/internal/author.rs"]
mod tests;
