use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;

use crate::{Entries, commit_object, index_with_modes, stage_directory, tree_from};

/// The four forms the fixtures speak in the hundreds, answered without a git
/// process and byte for byte as git would write them. Anything else, and any
/// repository state this does not model, is `None`: run git.
pub(crate) fn answer(dir: &Path, args: &[&str]) -> Option<std::io::Result<String>> {
    let git_dir = dir.join(".git");
    let sha1 = git_dir.is_dir() && !declares_object_format(&git_dir);
    match args {
        ["init", "-q"] => (!git_dir.exists()).then(|| init(&git_dir)),
        ["add", "."] if sha1 => add(dir),
        ["commit", "-qm" | "-m", message] | ["commit", "-q", "-m", message] if sha1 => {
            commit(dir, message)
        }
        ["rev-parse", "HEAD"] if sha1 => match head_commit(&git_dir) {
            Ok(Some(oid)) => Some(Ok(format!("{oid}\n"))),
            Ok(None) => None,
            Err(defect) => Some(Err(defect)),
        },
        _ => None,
    }
}

/// A repository that names its object format was initialized by git with a
/// format the fixture writers do not produce.
fn declares_object_format(git_dir: &Path) -> bool {
    std::fs::read_to_string(git_dir.join("config"))
        .is_ok_and(|config| config.to_ascii_lowercase().contains("objectformat"))
}

fn init(git_dir: &Path) -> std::io::Result<String> {
    for directory in ["objects/info", "objects/pack", "refs/heads", "refs/tags"] {
        std::fs::create_dir_all(git_dir.join(directory))?;
    }
    std::fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n")?;
    let config = if cfg!(windows) {
        "[core]\n\trepositoryformatversion = 0\n\tfilemode = false\n\tbare = false\n\tlogallrefupdates = true\n\tsymlinks = false\n\tignorecase = true\n"
    } else {
        "[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n\tlogallrefupdates = true\n"
    };
    std::fs::write(git_dir.join("config"), config)?;
    Ok(String::new())
}

fn add(root: &Path) -> Option<std::io::Result<String>> {
    let git_dir = root.join(".git");
    match read_index(&git_dir) {
        Ok(Some(_current)) => {}
        Ok(None) => return None,
        Err(defect) => return Some(Err(defect)),
    }
    match plain_worktree(root, root) {
        Ok(true) => {}
        Ok(false) => return None,
        Err(defect) => return Some(Err(defect)),
    }
    let mut staged = BTreeMap::new();
    if let Err(defect) = stage_directory(root, root, &mut staged) {
        return Some(Err(defect));
    }
    let rows = staged
        .iter()
        .map(|(path, (mode, oid))| {
            u32::from_str_radix(mode, 8)
                .map(|mode| (path.as_bytes(), mode, oid.as_str()))
                .map_err(std::io::Error::other)
        })
        .collect::<std::io::Result<Vec<_>>>();
    Some(
        rows.and_then(|rows| index_with_modes(root, &rows))
            .map(|()| String::new()),
    )
}

fn commit(root: &Path, message: &str) -> Option<std::io::Result<String>> {
    let git_dir = root.join(".git");
    let branch = match head_ref(&git_dir) {
        Ok(Some(branch)) => branch,
        Ok(None) => return None,
        Err(defect) => return Some(Err(defect)),
    };
    let staged = match read_index(&git_dir) {
        Ok(Some(staged)) => staged,
        Ok(None) => return None,
        Err(defect) => return Some(Err(defect)),
    };
    let parent = match read_ref(&git_dir, &branch) {
        Ok(parent) => parent,
        Err(defect) => return Some(Err(defect)),
    };
    let parent_tree = match parent
        .as_deref()
        .map(|oid| loose_commit_tree(&git_dir, oid))
    {
        None => None,
        Some(Some(tree)) => Some(tree),
        Some(None) => return None,
    };
    Some(commit_staged(
        root,
        &branch,
        &staged,
        parent.as_deref(),
        parent_tree.as_deref(),
        message,
    ))
}

fn commit_staged(
    root: &Path,
    branch: &str,
    staged: &Entries,
    parent: Option<&str>,
    parent_tree: Option<&str>,
    message: &str,
) -> std::io::Result<String> {
    let tree = tree_from(root, staged)?;
    if parent_tree == Some(tree.as_str()) {
        return Err(std::io::Error::other(
            "nothing to commit, working tree clean",
        ));
    }
    let parents: Vec<&str> = parent.into_iter().collect();
    let id = commit_object(root, &tree, &parents, message)?;
    let reference = root.join(".git").join(branch);
    if let Some(directory) = reference.parent() {
        std::fs::create_dir_all(directory)?;
    }
    std::fs::write(reference, format!("{id}\n"))?;
    Ok(String::new())
}

/// A worktree git would stage exactly as the walk does: no ignore or
/// attribute files anywhere, and no repository nested inside it.
fn plain_worktree(root: &Path, directory: &Path) -> std::io::Result<bool> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" {
            if directory != root {
                return Ok(false);
            }
            continue;
        }
        if name == ".gitignore" || name == ".gitattributes" {
            return Ok(false);
        }
        if entry.file_type()?.is_dir() && !plain_worktree(root, &entry.path())? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn head_ref(git_dir: &Path) -> std::io::Result<Option<String>> {
    let head = std::fs::read_to_string(git_dir.join("HEAD"))?;
    Ok(head
        .strip_prefix("ref: ")
        .map(|reference| reference.trim().to_owned()))
}

fn head_commit(git_dir: &Path) -> std::io::Result<Option<String>> {
    match head_ref(git_dir)? {
        Some(branch) => read_ref(git_dir, &branch),
        None => Ok(Some(
            std::fs::read_to_string(git_dir.join("HEAD"))?
                .trim()
                .to_owned(),
        )),
    }
}

fn read_ref(git_dir: &Path, reference: &str) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(git_dir.join(reference)) {
        Ok(text) => Ok(Some(text.trim().to_owned())),
        Err(defect) if defect.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(defect) => Err(defect),
    }
}

/// The tree a loose commit names, or `None` when the commit is packed or
/// otherwise not something this reader should interpret.
fn loose_commit_tree(git_dir: &Path, oid: &str) -> Option<String> {
    let (fan, rest) = oid.split_at(oid.len().min(2));
    let compressed = std::fs::read(git_dir.join("objects").join(fan).join(rest)).ok()?;
    let mut object = Vec::new();
    flate2::read::ZlibDecoder::new(compressed.as_slice())
        .read_to_end(&mut object)
        .ok()?;
    let text = std::str::from_utf8(&object).ok()?;
    let body = text.strip_prefix("commit ")?;
    let (_length, body) = body.split_once('\0')?;
    body.lines()
        .find_map(|line| line.strip_prefix("tree "))
        .map(str::to_owned)
}

/// The staged entries of an index this reader models: version two or three,
/// every entry at stage zero without extended flags, no split index. Anything
/// else is `None`, and an absent index is empty.
fn read_index(git_dir: &Path) -> std::io::Result<Option<Entries>> {
    match std::fs::read(git_dir.join("index")) {
        Ok(bytes) => Ok(parse_index(&bytes)),
        Err(defect) if defect.kind() == std::io::ErrorKind::NotFound => Ok(Some(BTreeMap::new())),
        Err(defect) => Err(defect),
    }
}

fn parse_index(bytes: &[u8]) -> Option<Entries> {
    if bytes.get(..4)? != b"DIRC" || !(2..=3).contains(&be32(bytes, 4)?) {
        return None;
    }
    let count = usize::try_from(be32(bytes, 8)?).ok()?;
    let body = bytes.get(..bytes.len().checked_sub(20)?)?;
    let mut staged = BTreeMap::new();
    let mut at: usize = 12;
    for _ in 0..count {
        let mode = be32(body, at.checked_add(24)?)?;
        let oid = hex::encode(body.get(at.checked_add(40)?..at.checked_add(60)?)?);
        if be16(body, at.checked_add(60)?)? & 0x7000 != 0 {
            return None;
        }
        let name_at = at.checked_add(62)?;
        let name_end = body
            .get(name_at..)?
            .iter()
            .position(|byte| *byte == 0)?
            .checked_add(name_at)?;
        let name = std::str::from_utf8(body.get(name_at..name_end)?).ok()?;
        staged.insert(name.to_owned(), (format!("{mode:o}"), oid));
        let length = name_end.checked_add(1)?.checked_sub(at)?;
        at = at.checked_add(length.div_ceil(8).checked_mul(8)?)?;
    }
    while at < body.len() {
        if body.get(at..at.checked_add(4)?)? == b"link" {
            return None;
        }
        let size = usize::try_from(be32(body, at.checked_add(4)?)?).ok()?;
        at = at.checked_add(8)?.checked_add(size)?;
    }
    (at == body.len()).then_some(staged)
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes
        .get(at..at.checked_add(4)?)?
        .try_into()
        .ok()
        .map(u32::from_be_bytes)
}

fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    bytes
        .get(at..at.checked_add(2)?)?
        .try_into()
        .ok()
        .map(u16::from_be_bytes)
}
