mod tests;

use std::fs;
use std::process::ExitCode;

use amiss_scan::claim::{GovernedForm, classify};
use amiss_wire::model::Adapter;

use crate::invocation::AuthorInvocation;

/// Nothing reaches stdout unproven: the printed bytes must survive the
/// extractor and the claim grammar first.
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
    let Ok(wanted) = usize::try_from(author.line) else {
        eprintln!("amiss claim: the line number does not fit this platform");
        return ExitCode::FAILURE;
    };
    // The evaluation's own line scanner, so authored numbers and bytes match
    // what a check will later compare.
    let Some(line) = amiss_md::lines::scan(&bytes).nth(wanted.saturating_sub(1)) else {
        let held = amiss_md::lines::scan(&bytes).count();
        eprintln!(
            "amiss claim: {} holds {held} lines and L{} is past them",
            author.path, author.line
        );
        return ExitCode::FAILURE;
    };
    let selected = bytes.get(line.start..line.end).unwrap_or_default();
    let content = selected
        .strip_suffix(b"\r\n")
        .or_else(|| selected.strip_suffix(b"\n"))
        .or_else(|| selected.strip_suffix(b"\r"))
        .unwrap_or(selected);
    let Ok(expected) = std::str::from_utf8(content) else {
        eprintln!(
            "amiss claim: line L{} of {} is not UTF-8, so it cannot be quoted",
            author.line, author.path
        );
        return ExitCode::FAILURE;
    };
    // Double quotes first, single quotes for lines that hold them.
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
        GovernedForm::Projection { .. } | GovernedForm::Unknown => false,
    }
}
