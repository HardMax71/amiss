use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

/// The adversarial 4 MiB document behind the parser-eligibility law: the
/// densest legal stress on the reference grammars while staying valid under
/// every contract ceiling. Unpaired emphasis delimiters exercise the
/// attention resolver, deep lazy blockquotes exercise container matching,
/// and long code-span candidates exercise backtick pairing. The shape avoids
/// braces and angle brackets so the same bytes are valid MDX, produces no
/// extracted references, and keeps well under the node and nesting caps.
#[must_use]
pub fn worst_case_markdown(target_bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(target_bytes.saturating_add(1_024));
    let mut index = 0_usize;
    while out.len() < target_bytes {
        match index.checked_rem(3).unwrap_or(0) {
            0 => emphasis_section(&mut out),
            1 => blockquote_section(&mut out),
            _ => backtick_section(&mut out),
        }
        index = index.saturating_add(1);
    }
    out.truncate(target_bytes);
    while out.last().is_some_and(|byte| *byte != b'\n') {
        out.pop();
    }
    out
}

/// One paragraph of unpaired left-flanking emphasis runs: every `*` opens
/// and nothing closes, the classic delimiter-stack stress.
fn emphasis_section(out: &mut Vec<u8>) {
    for _ in 0..40 {
        for _ in 0..120 {
            out.extend_from_slice(b"**a __b ");
        }
        out.extend_from_slice(b"\n");
    }
    out.extend_from_slice(b"\n");
}

/// Two hundred nested blockquote markers, repeated with lazy continuation
/// lines, well under the 256 nesting cap.
fn blockquote_section(out: &mut Vec<u8>) {
    for _ in 0..8 {
        for _ in 0..200 {
            out.extend_from_slice(b"> ");
        }
        out.extend_from_slice(b"q\n");
    }
    out.extend_from_slice(b"\n");
}

/// Unmatched backtick runs of stepped lengths: every candidate code span
/// scans forward for a closer that never matches its length.
fn backtick_section(out: &mut Vec<u8>) {
    for step in 1..40_usize {
        for _ in 0..30 {
            for _ in 0..step {
                out.push(b'`');
            }
            out.push(b'x');
        }
        out.extend_from_slice(b"\n");
    }
    out.extend_from_slice(b"\n");
}

/// A representative documentation tree: `documents` markdown files with
/// intra-repository links (most resolving, a few dangling) plus target
/// files, sized like ordinary hand-written pages.
#[must_use]
pub fn representative_documents(documents: usize) -> Vec<(String, String)> {
    let mut files = Vec::with_capacity(documents.saturating_add(1));
    files.push((
        "README.md".to_owned(),
        "# Index\n\nSee [one](docs/doc-0.md).\n".to_owned(),
    ));
    for index in 0..documents {
        let next = index
            .saturating_add(1)
            .checked_rem(documents.max(1))
            .unwrap_or(0);
        let mut body = format!("# Document {index}\n\n");
        for paragraph in 0..12_usize {
            let links = if paragraph < 3 {
                format!("links [next](doc-{next}.md) and [home](../README.md) ")
            } else if paragraph == 4 && index.checked_rem(10).unwrap_or(0) == 0 {
                format!("cites a [dangling](missing-{index}.md) reference ")
            } else {
                String::new()
            };
            let _infallible = std::fmt::Write::write_fmt(
                &mut body,
                format_args!(
                    "Paragraph {paragraph} {links}with some plain prose to reach a realistic \
                     page size for the measurement, a `code span`, and *emphasis*.\n\n"
                ),
            );
        }
        files.push((format!("docs/doc-{index}.md"), body));
    }
    files
}

/// Builds a two-commit repository from the representative tree: the base
/// commit, then a candidate touching roughly one document in twenty.
///
/// # Errors
///
/// Any git invocation failure, as plain I/O errors.
pub fn representative_repository(root: &Path, documents: usize) -> std::io::Result<()> {
    git(root, &["init", "-q"])?;
    for (path, body) in representative_documents(documents) {
        let file = root.join(&path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, body)?;
    }
    git(root, &["add", "."])?;
    git(root, &["commit", "-qm", "base"])?;
    for index in (0..documents).step_by(20) {
        let path = root.join(format!("docs/doc-{index}.md"));
        let mut body = std::fs::read_to_string(&path)?;
        body.push_str("\nA candidate-side addition with a [new link](doc-1.md).\n");
        std::fs::write(path, body)?;
    }
    git(root, &["add", "."])?;
    git(root, &["commit", "-qm", "candidate"])?;
    Ok(())
}

/// One quiet, hermetic git invocation with pinned identity and dates.
///
/// # Errors
///
/// Spawn failures and nonzero exits, as plain I/O errors.
pub fn git(dir: &Path, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        let mut detail = std::io::stderr().lock();
        let _best_effort = detail.write_all(&output.stderr);
        return Err(std::io::Error::other(format!("git {args:?} failed")));
    }
    String::from_utf8(output.stdout).map_err(std::io::Error::other)
}
