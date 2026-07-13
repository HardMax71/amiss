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

/// Stages a symlink entry, mode 120000, naming `target`. A symlink in a tree
/// is a blob holding the target path, so recording one needs no worktree
/// symlink, which an unprivileged Windows process cannot create anyway. The
/// resulting entry is byte for byte the one `git add` of a real symlink would
/// write, so the scanner sees the same tree on every platform.
///
/// Call this after the worktree has been staged: `git add .` stages deletions
/// too, and would drop an entry whose path is not in the worktree.
///
/// # Errors
///
/// Any git invocation failure, as plain I/O errors.
pub fn stage_symlink(root: &Path, target: &str, name: &str) -> std::io::Result<()> {
    let scratch = root.join("amiss-symlink-target");
    std::fs::write(&scratch, target)?;
    let oid = git(root, &["hash-object", "-w", "--", "amiss-symlink-target"])?;
    std::fs::remove_file(&scratch)?;
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("120000,{},{name}", oid.trim()),
        ],
    )?;
    Ok(())
}

/// A directory reparse point at `link` naming `target`: a symlink on unix, a
/// junction on Windows. A junction needs no privilege, where a Windows symlink
/// needs one, so the no-follow boundary stays provable on an ordinary CI
/// runner rather than only on an elevated one.
///
/// # Errors
///
/// The underlying link failure, as a plain I/O error.
#[cfg(unix)]
pub fn directory_link(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
pub fn directory_link(target: &Path, link: &Path) -> std::io::Result<()> {
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("mklink /J failed"))
    }
}

/// One quiet, hermetic git invocation with pinned identity and dates. The
/// global config names a path that does not exist, which every platform reads
/// as an empty file, where `/dev/null` would not resolve on Windows. Skipping
/// the system config matters twice over there: it is what carries Git for
/// Windows' `core.autocrlf=true`, so blobs and worktree bytes stay LF on
/// every platform and the fixtures hash the same everywhere.
///
/// # Errors
///
/// Spawn failures and nonzero exits, as plain I/O errors.
pub fn git(dir: &Path, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("absent-global-config"))
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
