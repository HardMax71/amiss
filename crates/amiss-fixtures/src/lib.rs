pub mod requests;

pub use cap;

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use amiss_wire::controls::ConstraintPlatform;
use amiss_wire::model::{ObjectFormat, Oid};
use sha1_checked::Digest as _;

/// Repository-local variables Git exports to hooks. They must not select the
/// repository, index, object store, or configuration for a fixture command.
/// Keep this list in sync with `git rev-parse --local-env-vars`; the integration
/// test detects additions made by the Git version running in CI.
const GIT_REPOSITORY_LOCAL_ENVIRONMENT: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

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
    // cmd reads a forward slash as a switch, so the paths hand it backslashes
    let flip = |path: &Path| {
        path.to_str()
            .map(|text| text.replace('/', "\\"))
            .ok_or_else(|| std::io::Error::other("junction paths are utf-8 here"))
    };
    let link = flip(link)?;
    let target = flip(target)?;
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J", link.as_str(), target.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("mklink /J failed"))
    }
}

/// A small executable header for one supported platform.
#[must_use]
pub fn executable_bytes(platform: ConstraintPlatform) -> Vec<u8> {
    let mut bytes = match platform {
        ConstraintPlatform::LinuxX8664 | ConstraintPlatform::LinuxAarch64 => {
            let machine = if platform == ConstraintPlatform::LinuxX8664 {
                [0x3e, 0x00]
            } else {
                [0xb7, 0x00]
            };
            let mut header = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0];
            header.extend_from_slice(&[0; 8]);
            header.extend_from_slice(&[0x02, 0x00]);
            header.extend_from_slice(&machine);
            header
        }
        ConstraintPlatform::MacosX8664 | ConstraintPlatform::MacosAarch64 => {
            let cpu = if platform == ConstraintPlatform::MacosX8664 {
                [0x07, 0x00, 0x00, 0x01]
            } else {
                [0x0c, 0x00, 0x00, 0x01]
            };
            let mut header = vec![0xcf, 0xfa, 0xed, 0xfe];
            header.extend_from_slice(&cpu);
            header
        }
        ConstraintPlatform::WindowsX8664 | ConstraintPlatform::WindowsAarch64 => {
            let machine = if platform == ConstraintPlatform::WindowsX8664 {
                [0x64, 0x86]
            } else {
                [0x64, 0xaa]
            };
            let mut header = vec![b'M', b'Z'];
            header.resize(0x3c, 0);
            header.extend_from_slice(&0x40_u32.to_le_bytes());
            header.extend_from_slice(b"PE\0\0");
            header.extend_from_slice(&machine);
            header
        }
    };
    bytes.extend_from_slice(&[0x90; 512]);
    bytes
}

/// Writes one loose object of `kind` framing `body` into the store at
/// `root/.git` and returns its full hex object ID. Bypassing git is the
/// point: a hostile fixture staged through a git port gets vetoed or mangled
/// on some platforms, and these bytes must be identical on all of them.
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
    // an existing object is this object, and git leaves them read-only
    if !file.exists() {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&framed)?;
        std::fs::write(file, encoder.finish()?)?;
    }
    Ok(oid)
}

/// Writes a tree object of `(mode literal, raw name, hex oid)` entries,
/// sorted here the way the grammar demands (a directory compares as its
/// name with `/` appended), so callers list them in any order.
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

/// Writes a commit object over `tree` with `parents`, under the same pinned
/// identity and date as the `git` helper.
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

/// Overwrites `root/.git/index` with a version-two index of stage-zero
/// regular files, each a raw path and hex blob ID. Stat fields stay zero,
/// which doubles as proof the scanner never trusts one.
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

/// A two-commit repository under a temporary root, addressed the way the
/// command line wants it. The root lives exactly as long as the value does.
pub struct CommitPair {
    dir: tempfile::TempDir,
    pub repo: String,
    pub base: String,
    pub candidate: String,
    pub base_tree: String,
    pub candidate_tree: String,
}

impl CommitPair {
    /// The repository root, for tests that stage more on top.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.dir.path()
    }
}

/// Writes the `.git` skeleton a fixture repository needs: an object store, a
/// heads directory, and a HEAD pointing at the branch commits will land on.
///
/// # Errors
///
/// Any filesystem failure, as plain I/O errors.
pub fn init_repository(root: &Path) -> std::io::Result<()> {
    let git_dir = root.join(".git");
    std::fs::create_dir_all(git_dir.join("objects"))?;
    std::fs::create_dir_all(git_dir.join("refs").join("heads"))?;
    std::fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n")
}

/// Commits everything under `root` except `.git`, on top of whatever HEAD
/// names, staging the listed paths executable where they are ordinary files.
/// Symbolic links are staged as links rather than followed.
///
/// # Errors
///
/// Any filesystem failure, or an entry that is neither a file, a directory,
/// nor a symbolic link.
pub fn commit_worktree(
    root: &Path,
    executables: &[&str],
    message: &str,
) -> std::io::Result<Commit> {
    let mut staged = BTreeMap::new();
    stage_directory(root, root, &mut staged)?;
    for path in executables {
        if let Some((mode, _oid)) = staged.get_mut(*path)
            && mode == "100644"
        {
            "100755".clone_into(mode);
        }
    }
    let tree = tree_from(root, &staged)?;
    let parent = head_commit(root)?;
    let parents: Vec<&str> = parent.iter().map(String::as_str).collect();
    let id = commit_object(root, &tree, &parents, message)?;
    std::fs::write(
        root.join(".git").join("refs").join("heads").join("main"),
        format!("{id}\n"),
    )?;
    let rows: Vec<(&[u8], &str)> = staged
        .iter()
        .map(|(path, (_mode, oid))| (path.as_bytes(), oid.as_str()))
        .collect();
    index_file(root, &rows)?;
    Ok(Commit { id, tree })
}

fn head_commit(root: &Path) -> std::io::Result<Option<String>> {
    let head = root.join(".git").join("refs").join("heads").join("main");
    match std::fs::read_to_string(&head) {
        Ok(text) => Ok(Some(text.trim().to_owned())),
        Err(defect) if defect.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(defect) => Err(defect),
    }
}

fn stage_directory(
    root: &Path,
    directory: &Path,
    staged: &mut BTreeMap<String, (String, String)>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            stage_directory(root, &path, staged)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(std::io::Error::other)?
            .components()
            .map(|part| {
                part.as_os_str()
                    .to_str()
                    .ok_or_else(|| std::io::Error::other("fixture paths are utf-8"))
            })
            .collect::<std::io::Result<Vec<_>>>()?
            .join("/");
        let (mode, body) = if kind.is_symlink() {
            let target = std::fs::read_link(&path)?;
            ("120000", path_arg(&target).into_bytes())
        } else if kind.is_file() {
            ("100644", std::fs::read(&path)?)
        } else {
            return Err(std::io::Error::other("fixture trees hold files and links"));
        };
        staged.insert(
            relative,
            (mode.to_owned(), loose_object(root, "blob", &body)?),
        );
    }
    Ok(())
}

/// What a fixture stages at one path.
pub enum Staged<'a> {
    /// Written to the worktree and staged as an ordinary file.
    File(&'a [u8]),
    /// Written to the worktree and staged executable.
    Executable(&'a [u8]),
    /// Staged as an ordinary file without touching the worktree, for a name a
    /// filesystem would hand back in another spelling.
    Absent(&'a [u8]),
    /// Staged as a symbolic link to this target, without touching the worktree.
    Symlink(&'a str),
    /// Staged as a gitlink to this commit.
    Submodule(&'a str),
}

/// Builds a one-commit repository whose tree holds exactly these entries.
///
/// # Errors
///
/// Any filesystem failure, or a submodule name that is not a full object name.
pub fn staged_repository(entries: &[(&str, Staged<'_>)]) -> std::io::Result<CommitChain> {
    let dir = tempfile::TempDir::new()?;
    let root = dir.path();
    let git_dir = root.join(".git");
    std::fs::create_dir_all(git_dir.join("objects"))?;
    std::fs::create_dir_all(git_dir.join("refs").join("heads"))?;
    std::fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n")?;

    let mut staged = BTreeMap::new();
    for (path, entry) in entries {
        let (mode, oid) = match entry {
            Staged::File(body) => {
                write_bytes(root, path, body)?;
                ("100644", loose_object(root, "blob", body)?)
            }
            Staged::Executable(body) => {
                write_bytes(root, path, body)?;
                ("100755", loose_object(root, "blob", body)?)
            }
            Staged::Absent(body) => ("100644", loose_object(root, "blob", body)?),
            Staged::Symlink(target) => ("120000", loose_object(root, "blob", target.as_bytes())?),
            Staged::Submodule(commit) => {
                let _checked = oid_bytes(commit)?;
                ("160000", (*commit).to_owned())
            }
        };
        staged.insert((*path).to_owned(), (mode.to_owned(), oid));
    }
    let tree = tree_from(root, &staged)?;
    let id = commit_object(root, &tree, &[], "fixture")?;
    std::fs::write(
        git_dir.join("refs").join("heads").join("main"),
        format!("{id}\n"),
    )?;
    let rows: Vec<(&[u8], &str)> = staged
        .iter()
        .filter(|(_path, (mode, _oid))| mode != "160000")
        .map(|(path, (_mode, oid))| (path.as_bytes(), oid.as_str()))
        .collect();
    index_file(root, &rows)?;
    let repo = path_arg(root);
    Ok(CommitChain {
        dir,
        repo,
        commits: vec![Commit { id, tree }],
    })
}

/// One commit of a fixture history: its own name and the name of its tree.
pub struct Commit {
    pub id: String,
    pub tree: String,
}

/// A fixture history under a temporary root, one commit per step, each step
/// written over the tree the previous one left.
pub struct CommitChain {
    dir: tempfile::TempDir,
    pub repo: String,
    pub commits: Vec<Commit>,
}

impl CommitChain {
    /// The repository root, for tests that stage more on top.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.dir.path()
    }
}

/// Builds one commit per step, each named by its message and carrying the
/// files written so far, so a later step may leave the tree unchanged.
///
/// # Errors
///
/// Any filesystem failure, as plain I/O errors.
pub fn commit_chain(steps: &[(&str, &[(&str, &str)])]) -> std::io::Result<CommitChain> {
    let dir = tempfile::TempDir::new()?;
    let root = dir.path();
    let git_dir = root.join(".git");
    std::fs::create_dir_all(git_dir.join("objects"))?;
    std::fs::create_dir_all(git_dir.join("refs").join("heads"))?;
    std::fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n")?;

    let mut staged = BTreeMap::new();
    let mut commits: Vec<Commit> = Vec::with_capacity(steps.len());
    for (message, files) in steps {
        let parents: Vec<&str> = commits
            .last()
            .map(|last| last.id.as_str())
            .into_iter()
            .collect();
        let (id, tree) = commit_state(root, files, &mut staged, &parents, message)?;
        commits.push(Commit { id, tree });
    }
    if let Some(head) = commits.last() {
        std::fs::write(
            git_dir.join("refs").join("heads").join("main"),
            format!("{}\n", head.id),
        )?;
    }
    let rows: Vec<(&[u8], &str)> = staged
        .iter()
        .map(|(path, (_mode, oid))| (path.as_bytes(), oid.as_str()))
        .collect();
    index_file(root, &rows)?;
    let repo = path_arg(root);
    Ok(CommitChain { dir, repo, commits })
}

/// Reads one fixture object name as the SHA-1 object it must be.
#[must_use]
pub fn sha1_oid(raw: &str) -> Option<Oid> {
    Oid::new(ObjectFormat::Sha1, raw.to_owned())
}

/// Builds the base commit from `base` files, then the candidate commit from
/// `candidate` files written over them. Parent directories appear as needed,
/// and either commit may leave the tree unchanged.
///
/// # Errors
///
/// Any filesystem failure, as plain I/O errors.
pub fn commit_pair(
    base: &[(&str, &str)],
    candidate: &[(&str, &str)],
) -> std::io::Result<CommitPair> {
    let chain = commit_chain(&[("base", base), ("candidate", candidate)])?;
    let mut built = chain.commits.into_iter();
    let (Some(base), Some(candidate)) = (built.next(), built.next()) else {
        return Err(std::io::Error::other("the fixture chain lost a commit"));
    };
    Ok(CommitPair {
        dir: chain.dir,
        repo: chain.repo,
        base: base.id,
        candidate: candidate.id,
        base_tree: base.tree,
        candidate_tree: candidate.tree,
    })
}

fn commit_state(
    root: &Path,
    files: &[(&str, &str)],
    staged: &mut BTreeMap<String, (String, String)>,
    parents: &[&str],
    message: &str,
) -> std::io::Result<(String, String)> {
    for (path, body) in files {
        write_file(root, path, body)?;
        staged.insert(
            (*path).to_owned(),
            (
                "100644".to_owned(),
                loose_object(root, "blob", body.as_bytes())?,
            ),
        );
    }
    let tree = tree_from(root, staged)?;
    let commit = commit_object(root, &tree, parents, message)?;
    Ok((commit, tree))
}

fn tree_from(root: &Path, files: &BTreeMap<String, (String, String)>) -> std::io::Result<String> {
    let mut blobs = Vec::new();
    let mut directories: BTreeMap<String, BTreeMap<String, (String, String)>> = BTreeMap::new();
    for (path, entry) in files {
        match path.split_once('/') {
            None => blobs.push((path.clone(), entry.clone())),
            Some((head, rest)) => {
                directories
                    .entry(head.to_owned())
                    .or_default()
                    .insert(rest.to_owned(), entry.clone());
            }
        }
    }
    let mut subtrees = Vec::new();
    for (name, nested) in &directories {
        subtrees.push((name.clone(), tree_from(root, nested)?));
    }
    let mut entries: Vec<(&str, &[u8], &str)> = blobs
        .iter()
        .map(|(name, (mode, oid))| (mode.as_str(), name.as_bytes(), oid.as_str()))
        .collect();
    entries.extend(
        subtrees
            .iter()
            .map(|(name, oid)| ("40000", name.as_bytes(), oid.as_str())),
    );
    tree_object(root, &entries)
}

fn write_file(root: &Path, path: &str, body: &str) -> std::io::Result<()> {
    write_bytes(root, path, body.as_bytes())
}

fn write_bytes(root: &Path, path: &str, body: &[u8]) -> std::io::Result<()> {
    let file = root.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(file, body)
}

/// A filesystem path as the UTF-8 string a command line carries.
///
/// # Panics
///
/// When the path is not UTF-8. Fixture trees choose their own names, so a
/// path this cannot render is the calling test's own defect, surfaced here
/// rather than three asserts later as a mangled argument.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "fixture paths are chosen by the tests themselves"
)]
pub fn path_arg(path: &Path) -> String {
    path.to_str().expect("fixture paths are utf-8").to_owned()
}

/// One quiet, hermetic git invocation with pinned identity and dates. Any
/// repository-local environment inherited from a hook is removed first, so a
/// linked worktree's `GIT_DIR`, common directory, index, and object store
/// cannot capture commands intended for a fixture repository. The global
/// config names a path that does not exist, which every platform reads as an
/// empty file, where `/dev/null` would not resolve on Windows. Skipping the
/// system config matters twice over there: it is what carries Git for Windows'
/// `core.autocrlf=true`, so blobs and worktree bytes stay LF on every platform
/// and the fixtures hash the same everywhere.
///
/// # Errors
///
/// Spawn failures and nonzero exits, as plain I/O errors.
pub fn git(dir: &Path, args: &[&str]) -> std::io::Result<String> {
    let output = git_output(dir, args)?;
    if !output.status.success() {
        let mut detail = std::io::stderr().lock();
        let _best_effort = detail.write_all(&output.stderr);
        return Err(std::io::Error::other(format!("git {args:?} failed")));
    }
    String::from_utf8(output.stdout).map_err(std::io::Error::other)
}

/// One hermetic git invocation whose output is returned even when git exits
/// unsuccessfully. This is for fixtures that deliberately create a rejected
/// operation, such as a merge conflict.
///
/// # Errors
///
/// Git could not be spawned or its output could not be collected.
pub fn git_output(dir: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    git_command(dir).args(args).output()
}

fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    configure_git_command(&mut command, dir);
    command
}

/// Makes a git command hermetic for one fixture repository. Repository-local
/// variables inherited from a hook are explicitly removed, and configuration,
/// identity, dates, working directory, and standard input are pinned.
///
/// Explicitly removed variables override values already attached to `command`
/// as well as values inherited from its parent process.
pub fn configure_git_command(command: &mut Command, dir: &Path) {
    for name in GIT_REPOSITORY_LOCAL_ENVIRONMENT {
        command.env_remove(name);
    }
    command
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("absent-global-config"))
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .stdin(Stdio::null());
}

/// Live-allocation ceiling for every test binary in the workspace.
pub const MEMORY_CEILING: usize = 2 * 1024 * 1024 * 1024;

/// Caps the binary's global allocator, so a runaway allocation fails the one
/// test that made it on any machine, instead of relying on the environment to
/// contain what the work budget no longer does.
#[macro_export]
macro_rules! bounded_memory {
    () => {
        $crate::bounded_memory!(::std::alloc::System, ::std::alloc::System);
    };
    ($inner_type:ty, $inner:expr) => {
        #[global_allocator]
        static BOUNDED: $crate::cap::Cap<$inner_type> =
            $crate::cap::Cap::new($inner, $crate::MEMORY_CEILING);
    };
}
