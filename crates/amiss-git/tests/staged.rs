use std::fs;
use std::path::Path;

use amiss_fixtures::stage_symlink;
use amiss_git::{Error, GitLimits, GitResources, Repository, parse_index_file};
use amiss_wire::controls::GitMode;
use amiss_wire::model::ObjectFormat;
use tempfile::TempDir;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn git_allow_failure(dir: &Path, args: &[&str]) {
    amiss_fixtures::git_output(dir, args).expect("run git");
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn read(dir: &Path) -> Result<amiss_git::LogicalIndex, Error> {
    let repo = Repository::open(dir, ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let bytes = repo.read_index_bytes(&mut resources)?;
    parse_index_file(ObjectFormat::Sha1, &bytes)
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn base(dir: &Path) {
    git(dir, &["init", "-q"]);
    fs::write(dir.join("b.txt"), "b\n").unwrap();
    fs::create_dir_all(dir.join("docs")).unwrap();
    fs::write(dir.join("docs/a.md"), "# a\n").unwrap();
    fs::write(dir.join("run.sh"), "#!/bin/sh\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["update-index", "--chmod=+x", "--", "run.sh"]);
    stage_symlink(dir, "b.txt", "alias").unwrap();
    git(
        dir,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,0123456789012345678901234567890123456789,module",
        ],
    );
}

#[test]
fn the_supported_index_parses_with_exact_pairings() {
    let dir = TempDir::new().unwrap();
    base(dir.path());
    let index = read(dir.path()).unwrap();
    let listing: Vec<(&str, GitMode, bool)> = index
        .entries
        .iter()
        .map(|entry| {
            (
                str::from_utf8(&entry.path).unwrap(),
                entry.mode,
                entry.skip_worktree,
            )
        })
        .collect();
    assert_eq!(
        listing,
        vec![
            ("alias", GitMode::Symlink, false),
            ("b.txt", GitMode::RegularFile, false),
            ("docs/a.md", GitMode::RegularFile, false),
            ("module", GitMode::Gitlink, false),
            ("run.sh", GitMode::ExecutableFile, false),
        ],
        "sorted stage-zero rows with their exact modes"
    );
}

#[test]
fn skip_worktree_survives_and_version_four_parses() {
    let dir = TempDir::new().unwrap();
    base(dir.path());
    git(dir.path(), &["update-index", "--skip-worktree", "b.txt"]);
    let index = read(dir.path()).unwrap();
    let skipped: Vec<&str> = index
        .entries
        .iter()
        .filter(|entry| entry.skip_worktree)
        .map(|entry| str::from_utf8(&entry.path).unwrap())
        .collect();
    assert_eq!(skipped, vec!["b.txt"]);

    git(dir.path(), &["update-index", "--index-version", "4"]);
    let again = read(dir.path()).unwrap();
    assert_eq!(index, again, "version four carries the same logical index");
}

#[test]
fn intent_to_add_and_unmerged_and_split_reject() {
    let dir = TempDir::new().unwrap();
    base(dir.path());
    fs::write(dir.path().join("new.md"), "n\n").unwrap();
    git(dir.path(), &["add", "-N", "new.md"]);
    assert_eq!(read(dir.path()), Err(Error::IntentToAdd));
    git(dir.path(), &["rm", "--cached", "-q", "new.md"]);

    git(dir.path(), &["commit", "-qm", "one"]);
    git(dir.path(), &["checkout", "-qb", "left"]);
    fs::write(dir.path().join("b.txt"), "left\n").unwrap();
    git(dir.path(), &["commit", "-aqm", "left"]);
    git(dir.path(), &["checkout", "-q", "-"]);
    fs::write(dir.path().join("b.txt"), "right\n").unwrap();
    git(dir.path(), &["commit", "-aqm", "right"]);
    git_allow_failure(dir.path(), &["merge", "-q", "left"]);
    assert_eq!(read(dir.path()), Err(Error::IndexUnmerged));
    git_allow_failure(dir.path(), &["merge", "--abort"]);

    git(dir.path(), &["update-index", "--split-index"]);
    assert_eq!(read(dir.path()), Err(Error::IndexInvalid));
}

#[test]
fn corruption_and_race_detection() {
    let dir = TempDir::new().unwrap();
    base(dir.path());
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let initial = repo.read_index_bytes(&mut resources).unwrap();

    let mut corrupt = initial.clone();
    if let Some(byte) = corrupt.get_mut(20) {
        *byte = byte.wrapping_add(1);
    }
    assert_eq!(
        parse_index_file(ObjectFormat::Sha1, &corrupt),
        Err(Error::IndexInvalid),
        "the trailing checksum binds the content"
    );

    assert_eq!(
        repo.verify_index_unchanged(&mut resources, &initial),
        Ok(()),
        "byte identity is accepted"
    );

    let file = fs::File::options()
        .append(true)
        .open(dir.path().join("b.txt"))
        .unwrap();
    file.set_modified(std::time::SystemTime::UNIX_EPOCH)
        .unwrap();
    drop(file);
    git(dir.path(), &["update-index", "b.txt"]);
    let refreshed = repo.read_index_bytes(&mut resources).unwrap();
    assert_ne!(refreshed, initial, "the raw bytes moved with the stat data");
    assert_eq!(
        repo.verify_index_unchanged(&mut resources, &initial),
        Ok(()),
        "a stat-only rewrite keeps the logical projection equal"
    );

    fs::write(dir.path().join("b.txt"), "changed\n").unwrap();
    git(dir.path(), &["add", "b.txt"]);
    assert_eq!(
        repo.verify_index_unchanged(&mut resources, &initial),
        Err(Error::SnapshotChanged),
        "a staged content change is solely a snapshot change"
    );
}

fn entry_bytes(path: &[u8], flags: u16, extended: Option<u16>) -> Vec<u8> {
    let mut entry = vec![0_u8; 24];
    entry.extend_from_slice(&0o100_644_u32.to_be_bytes());
    entry.extend_from_slice(&[0; 12]);
    entry.extend_from_slice(&[0xaa; 20]);
    entry.extend_from_slice(&flags.to_be_bytes());
    if let Some(word) = extended {
        entry.extend_from_slice(&word.to_be_bytes());
    }
    entry.extend_from_slice(path);
    entry.push(0);
    while !entry.len().is_multiple_of(8) {
        entry.push(0);
    }
    entry
}

fn raw_entry(path: &[u8], flags: u16) -> Vec<u8> {
    entry_bytes(path, flags, None)
}

fn named_entry(path: &[u8]) -> Vec<u8> {
    raw_entry(path, u16::try_from(path.len()).unwrap_or(0x0fff))
}

fn extended_entry(path: &[u8], extended: u16) -> Vec<u8> {
    let flags = 0x4000 | u16::try_from(path.len()).unwrap_or(0);
    entry_bytes(path, flags, Some(extended))
}

fn raw_index_v(version: u32, entries: &[Vec<u8>], extensions: &[u8]) -> Vec<u8> {
    use sha1_checked::Digest as _;
    let mut content = b"DIRC".to_vec();
    content.extend_from_slice(&version.to_be_bytes());
    content.extend_from_slice(&u32::try_from(entries.len()).unwrap_or(0).to_be_bytes());
    for entry in entries {
        content.extend_from_slice(entry);
    }
    content.extend_from_slice(extensions);
    let mut hasher = sha1_checked::Sha1::builder().build();
    hasher.update(&content);
    content.extend_from_slice(hasher.try_finalize().hash().as_slice());
    content
}

fn raw_index(entries: &[Vec<u8>], extensions: &[u8]) -> Vec<u8> {
    raw_index_v(2, entries, extensions)
}

#[test]
fn an_extended_entry_is_skip_worktree_only_by_its_bit() {
    let clear = raw_index_v(3, &[extended_entry(b"a", 0)], b"");
    let parsed = parse_index_file(ObjectFormat::Sha1, &clear).expect("a clear extended word");
    assert_eq!(
        parsed.entries.first().map(|entry| entry.skip_worktree),
        Some(false)
    );

    let set = raw_index_v(3, &[extended_entry(b"a", 0x4000)], b"");
    let parsed = parse_index_file(ObjectFormat::Sha1, &set).expect("the skip-worktree bit");
    assert_eq!(
        parsed.entries.first().map(|entry| entry.skip_worktree),
        Some(true)
    );
}

#[test]
fn a_crafted_index_holds_order_prefixes_and_name_lengths() {
    let sound = parse_index_file(ObjectFormat::Sha1, &raw_index(&[named_entry(b"a")], b""))
        .expect("one plain row parses");
    assert_eq!(sound.entries.len(), 1);

    let shadowed = raw_index(&[named_entry(b"a"), named_entry(b"a/b")], b"");
    assert_eq!(
        parse_index_file(ObjectFormat::Sha1, &shadowed).unwrap_err(),
        Error::IndexInvalid,
        "a file row shadows every path below it"
    );

    let misordered = raw_index(&[named_entry(b"b"), named_entry(b"a")], b"");
    assert_eq!(
        parse_index_file(ObjectFormat::Sha1, &misordered).unwrap_err(),
        Error::IndexInvalid,
        "rows must ascend"
    );

    let lying = raw_index(&[raw_entry(b"ab", 1)], b"");
    assert_eq!(
        parse_index_file(ObjectFormat::Sha1, &lying).unwrap_err(),
        Error::IndexInvalid,
        "the flag length must match the path"
    );

    let long_marker = parse_index_file(
        ObjectFormat::Sha1,
        &raw_index(&[raw_entry(b"a", 0x0fff)], b""),
    )
    .expect("the long-name marker skips length verification");
    assert_eq!(long_marker.entries.len(), 1);
}

#[test]
fn index_extensions_are_skipped_or_refused_by_their_case() {
    let entry = named_entry(b"a");

    let mut mandatory = b"link".to_vec();
    mandatory.extend_from_slice(&0_u32.to_be_bytes());
    assert_eq!(
        parse_index_file(
            ObjectFormat::Sha1,
            &raw_index(std::slice::from_ref(&entry), &mandatory)
        )
        .unwrap_err(),
        Error::IndexInvalid,
        "a lowercase-initial extension is mandatory and unknown"
    );

    let mut optional = b"TREE".to_vec();
    optional.extend_from_slice(&4_u32.to_be_bytes());
    optional.extend_from_slice(b"tree");
    let skipped = parse_index_file(
        ObjectFormat::Sha1,
        &raw_index(std::slice::from_ref(&entry), &optional),
    )
    .expect("an exactly-fitting optional extension is skipped");
    assert_eq!(skipped.entries.len(), 1);

    let mut overrun = b"TREE".to_vec();
    overrun.extend_from_slice(&8_u32.to_be_bytes());
    overrun.extend_from_slice(b"tree");
    assert_eq!(
        parse_index_file(ObjectFormat::Sha1, &raw_index(&[entry], &overrun)).unwrap_err(),
        Error::IndexInvalid,
        "an extension may not overrun the file"
    );
}
