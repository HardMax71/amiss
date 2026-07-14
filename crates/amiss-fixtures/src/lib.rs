use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use sha1_checked::Digest as _;

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

/// Writes one loose object of `kind` framing `body` into the store at
/// `root/.git` and returns its full hex object ID. The bytes go straight to
/// disk, which is what lets a fixture carry a name or a path no operating
/// system or git port would accept from a command line: git for Windows
/// refuses to stage a path holding a colon or a control byte, and a fixture
/// routed through it quietly degenerates into a tree that no longer tests
/// anything. The scanner reads the store, so the store is where hostile
/// bytes are planted, identically on every platform.
///
/// # Errors
///
/// Any filesystem failure, as plain I/O errors.
pub fn loose_object(root: &Path, kind: &str, body: &[u8]) -> std::io::Result<String> {
    let mut framed = Vec::with_capacity(body.len().saturating_add(32));
    framed.extend_from_slice(kind.as_bytes());
    framed.push(b' ');
    framed.extend_from_slice(body.len().to_string().as_bytes());
    framed.push(0);
    framed.extend_from_slice(body);
    let oid = hex(&sha1(&framed));
    let (fan, rest) = oid.split_at(2);
    let bucket = root.join(".git").join("objects").join(fan);
    std::fs::create_dir_all(&bucket)?;
    let file = bucket.join(rest);
    // The store is content-addressed and git writes loose objects read-only,
    // so an object that already exists is this object, and stays untouched.
    if !file.exists() {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&framed)?;
        std::fs::write(file, encoder.finish()?)?;
    }
    Ok(oid)
}

/// Writes a tree object holding exactly `entries`, each a git mode literal
/// such as `100644` or `40000`, raw name bytes, and the hex object ID the
/// name resolves to. Entries are sorted here the way the tree grammar
/// demands, a directory comparing as its name with `/` appended, so callers
/// list them in any order.
///
/// # Errors
///
/// Any filesystem failure or malformed object ID, as plain I/O errors.
pub fn tree_object(root: &Path, entries: &[(&str, &[u8], &str)]) -> std::io::Result<String> {
    let mut rows = entries.to_vec();
    rows.sort_by_key(|(mode, name, _oid)| {
        let mut key = name.to_vec();
        if *mode == "40000" {
            key.push(b'/');
        }
        key
    });
    let mut body = Vec::new();
    for (mode, name, oid) in rows {
        body.extend_from_slice(mode.as_bytes());
        body.push(b' ');
        body.extend_from_slice(name);
        body.push(0);
        body.extend_from_slice(&oid_bytes(oid)?);
    }
    loose_object(root, "tree", &body)
}

/// Writes a commit object over `tree` with `parents`, under the identity and
/// date the `git` helper pins, so directly written history is as
/// deterministic as the spawned kind.
///
/// # Errors
///
/// Any filesystem failure, as plain I/O errors.
pub fn commit_object(
    root: &Path,
    tree: &str,
    parents: &[&str],
    message: &str,
) -> std::io::Result<String> {
    let mut body = format!("tree {tree}\n");
    for parent in parents {
        let _infallible = std::fmt::Write::write_fmt(&mut body, format_args!("parent {parent}\n"));
    }
    body.push_str("author t <t@example.invalid> 1767225600 +0000\n");
    body.push_str("committer t <t@example.invalid> 1767225600 +0000\n\n");
    body.push_str(message);
    body.push('\n');
    loose_object(root, "commit", body.as_bytes())
}

/// Overwrites `root/.git/index` with a version-two index holding exactly
/// `entries` as stage-zero regular files, each a raw path and the hex blob
/// ID it names. Paths sort by their bytes, a path longer than the format's
/// twelve length bits stores the `0xFFF` sentinel, and every stat field is
/// zero, which doubles as proof the scanner never trusts one.
///
/// # Errors
///
/// Any filesystem failure or malformed object ID, as plain I/O errors.
pub fn index_file(root: &Path, entries: &[(&[u8], &str)]) -> std::io::Result<()> {
    let mut rows = entries.to_vec();
    rows.sort_by_key(|(path, _oid)| path.to_vec());
    let mut content = Vec::new();
    content.extend_from_slice(b"DIRC");
    content.extend_from_slice(&2_u32.to_be_bytes());
    let count = u32::try_from(rows.len()).map_err(std::io::Error::other)?;
    content.extend_from_slice(&count.to_be_bytes());
    for (path, oid) in rows {
        let start = content.len();
        content.extend_from_slice(&[0_u8; 24]);
        content.extend_from_slice(&0o100_644_u32.to_be_bytes());
        content.extend_from_slice(&[0_u8; 12]);
        content.extend_from_slice(&oid_bytes(oid)?);
        let name_bits = u16::try_from(path.len().min(0xFFF)).unwrap_or(0xFFF);
        content.extend_from_slice(&name_bits.to_be_bytes());
        content.extend_from_slice(path);
        let unpadded = content.len().saturating_sub(start);
        let pad = 8_usize.saturating_sub(unpadded.checked_rem(8).unwrap_or(0));
        content.resize(content.len().saturating_add(pad), 0);
    }
    let checksum = sha1(&content);
    content.extend_from_slice(&checksum);
    std::fs::write(root.join(".git").join("index"), content)
}

fn sha1(data: &[u8]) -> Vec<u8> {
    let mut hasher = sha1_checked::Sha1::builder()
        .detect_collision(false)
        .build();
    hasher.update(data);
    hasher.try_finalize().hash().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _infallible = std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
    }
    out
}

fn oid_bytes(oid: &str) -> std::io::Result<Vec<u8>> {
    if oid.len() != 40 {
        return Err(std::io::Error::other("object IDs here are full sha1 hex"));
    }
    oid.as_bytes()
        .chunks(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|text| u8::from_str_radix(text, 16).ok())
                .ok_or_else(|| std::io::Error::other("object IDs here are full sha1 hex"))
        })
        .collect()
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
