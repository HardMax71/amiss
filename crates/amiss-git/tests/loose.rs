#![cfg(unix)]

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use amiss_git::{Error, GitLimits, GitResources, ObjectKind, Repository, parse_commit, parse_tree};
use amiss_wire::controls::{GitMode, ResourceName};
use amiss_wire::model::{ObjectFormat, Oid};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use sha2::Digest as _;
use tempfile::TempDir;

fn sha1_hex(preimage: &[u8]) -> String {
    let mut hasher = sha1_checked::Sha1::builder().build();
    hasher.update(preimage);
    let mut out = String::new();
    for byte in hasher.try_finalize().hash().iter().copied() {
        out.push(char::from_digit(u32::from(byte.wrapping_shr(4)), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(byte & 0xF), 16).unwrap_or('0'));
    }
    out
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn compress(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn make_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".git/objects")).unwrap();
    dir
}

fn loose_path(root: &Path, hex: &str) -> PathBuf {
    let (fan, rest) = hex.split_at(2);
    root.join(".git/objects").join(fan).join(rest)
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn place(root: &Path, hex: &str, compressed: &[u8]) {
    let path = loose_path(root, hex);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, compressed).unwrap();
}

fn preimage(kind: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!("{kind} {}\0", body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn write_loose(root: &Path, kind: &str, body: &[u8]) -> Oid {
    let bytes = preimage(kind, body);
    let hex = sha1_hex(&bytes);
    place(root, &hex, &compress(&bytes));
    Oid::new(ObjectFormat::Sha1, hex).unwrap()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn open(root: &Path) -> Repository {
    Repository::open(root, ObjectFormat::Sha1).unwrap()
}

#[test]
fn reads_verified_loose_objects() {
    let dir = make_repo();
    let oid = write_loose(dir.path(), "blob", b"hello docs\n");
    let repo = open(dir.path());
    let mut res = GitResources::new(GitLimits::CONTRACT);
    let object = repo.read_object(&mut res, &oid).unwrap();
    assert_eq!(object.kind, ObjectKind::Blob);
    assert_eq!(object.body, b"hello docs\n");
    assert_eq!(
        repo.read_expected(&mut res, &oid, ObjectKind::Tree)
            .unwrap_err(),
        Error::ObjectWrongKind
    );
}

#[test]
fn missing_and_unavailable_are_distinct() {
    let dir = make_repo();
    let repo = open(dir.path());
    let mut res = GitResources::new(GitLimits::CONTRACT);
    let absent = Oid::new(ObjectFormat::Sha1, "c".repeat(40)).unwrap();
    assert_eq!(
        repo.read_object(&mut res, &absent).unwrap_err(),
        Error::ObjectMissing
    );

    assert_eq!(
        Repository::open(&dir.path().join("nowhere"), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable
    );

    let bare = TempDir::new().unwrap();
    assert_eq!(
        Repository::open(bare.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "missing .git"
    );

    let filegit = TempDir::new().unwrap();
    fs::write(filegit.path().join(".git"), "gitdir: elsewhere\n").unwrap();
    assert_eq!(
        Repository::open(filegit.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        ".git file is a linked worktree"
    );

    let symlinked = TempDir::new().unwrap();
    std::os::unix::fs::symlink(dir.path().join(".git"), symlinked.path().join(".git")).unwrap();
    assert_eq!(
        Repository::open(symlinked.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "symlink .git is never followed"
    );
}

#[test]
fn pack_presence_is_not_treated_as_missing() {
    let dir = make_repo();
    let pack_dir = dir.path().join(".git/objects/pack");
    fs::create_dir_all(&pack_dir).unwrap();
    fs::write(pack_dir.join("pack-junk.idx"), b"junk").unwrap();
    let repo = open(dir.path());
    let mut res = GitResources::new(GitLimits::CONTRACT);
    let absent = Oid::new(ObjectFormat::Sha1, "c".repeat(40)).unwrap();
    assert_eq!(
        repo.read_object(&mut res, &absent).unwrap_err(),
        Error::PackLookupUnimplemented
    );
}

#[test]
fn rejects_corrupt_loose_objects() {
    let dir = make_repo();
    let repo = open(dir.path());
    let mut res = GitResources::new(GitLimits::CONTRACT);
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();

    let victim = write_loose(dir.path(), "blob", b"victim");
    let mut corrupt = fs::read(loose_path(dir.path(), victim.as_str())).unwrap();
    if let Some(byte) = corrupt.last_mut() {
        *byte = byte.wrapping_add(1);
    }
    cases.push((victim.as_str().to_owned(), corrupt));

    for bytes in [
        preimage("blobb", b"x"),
        preimage("Blob", b"x"),
        b"blob 5\0abcd".to_vec(),
        b"blob 3\0abcd".to_vec(),
        b"blob 01\0a".to_vec(),
    ] {
        cases.push((sha1_hex(&bytes), compress(&bytes)));
    }

    let good = preimage("blob", b"a");
    let mut trailing = compress(&good);
    trailing.push(0x2a);
    cases.push((sha1_hex(&good), trailing));

    let migrant = preimage("blob", b"migrant");
    cases.push(("d".repeat(40), compress(&migrant)));

    for (hex, bytes) in cases {
        place(dir.path(), &hex, &bytes);
        let oid = Oid::new(ObjectFormat::Sha1, hex).unwrap();
        assert_eq!(
            repo.read_object(&mut res, &oid).unwrap_err(),
            Error::ObjectUnreadable,
            "case {oid:?}"
        );
    }

    let linked = write_loose(dir.path(), "blob", b"link target");
    let alias = "e".repeat(40);
    let alias_path = loose_path(dir.path(), &alias);
    fs::create_dir_all(alias_path.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(loose_path(dir.path(), linked.as_str()), alias_path).unwrap();
    let alias_oid = Oid::new(ObjectFormat::Sha1, alias).unwrap();
    assert_eq!(
        repo.read_object(&mut res, &alias_oid).unwrap_err(),
        Error::ObjectUnreadable,
        "symlinked loose object is not ordinary"
    );
}

#[test]
fn charges_and_enforces_resource_caps() {
    let dir = make_repo();
    let oid = write_loose(dir.path(), "blob", &vec![b'x'; 512]);
    let repo = open(dir.path());

    let mut res = GitResources::new(GitLimits {
        compressed_stream_bytes: 8,
        ..GitLimits::CONTRACT
    });
    let Err(Error::ResourceLimit { resource, .. }) = repo.read_object(&mut res, &oid) else {
        panic!("expected the stream cap");
    };
    assert_eq!(resource, ResourceName::GitCompressedObjectBytes);

    let mut res = GitResources::new(GitLimits {
        inflated_object_bytes: 16,
        ..GitLimits::CONTRACT
    });
    let Err(Error::ResourceLimit {
        resource,
        configured_limit,
        observed_lower_bound,
    }) = repo.read_object(&mut res, &oid)
    else {
        panic!("expected the inflated cap");
    };
    assert_eq!(resource, ResourceName::GitObjectBytes);
    assert_eq!(configured_limit, 16);
    assert!(observed_lower_bound > 16);

    let mut res = GitResources::new(GitLimits {
        aggregate_compressed_bytes: 4,
        ..GitLimits::CONTRACT
    });
    let Err(Error::ResourceLimit { resource, .. }) = repo.read_object(&mut res, &oid) else {
        panic!("expected the aggregate cap");
    };
    assert_eq!(
        resource,
        ResourceName::AggregateGitCompressedObjectBytesPerEvaluation
    );

    let mut res = GitResources::new(GitLimits::CONTRACT);
    repo.read_object(&mut res, &oid).unwrap();
    repo.read_object(&mut res, &oid).unwrap();
}

#[test]
fn parses_tree_and_commit_grammar() {
    let format = ObjectFormat::Sha1;
    let raw = vec![0xab_u8; 20];
    let blob_oid = "a".repeat(40);

    let mut tree = Vec::new();
    tree.extend_from_slice(b"100644 a.txt\0");
    tree.extend_from_slice(&raw);
    tree.extend_from_slice(b"40000 a\0");
    tree.extend_from_slice(&raw);
    let entries = parse_tree(format, &tree).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].mode, GitMode::RegularFile);
    assert_eq!(entries[0].name, b"a.txt");
    assert_eq!(entries[1].mode, GitMode::Tree);

    let mut unordered = Vec::new();
    unordered.extend_from_slice(b"40000 a\0");
    unordered.extend_from_slice(&raw);
    unordered.extend_from_slice(b"100644 a.txt\0");
    unordered.extend_from_slice(&raw);
    assert_eq!(
        parse_tree(format, &unordered).unwrap_err(),
        Error::ObjectUnreadable
    );

    for bad in [
        b"100645 a\0".as_slice(),
        b"040000 a\0".as_slice(),
        b"100644 \0".as_slice(),
        b"100644 ..\0".as_slice(),
        b"100644 a/b\0".as_slice(),
    ] {
        let mut body = bad.to_vec();
        body.extend_from_slice(&raw);
        assert_eq!(
            parse_tree(format, &body).unwrap_err(),
            Error::ObjectUnreadable
        );
    }
    assert_eq!(
        parse_tree(format, b"100644 a\0short").unwrap_err(),
        Error::ObjectUnreadable
    );

    let commit = format!(
        "tree {blob_oid}\nparent {blob_oid}\nparent {blob_oid}\nauthor A <a@x> 1 +0000\ncommitter A <a@x> 1 +0000\ngpgsig -----BEGIN-----\n more\n -----END-----\n\nmessage body\n"
    );
    let parsed = parse_commit(format, commit.as_bytes()).unwrap();
    assert_eq!(parsed.tree.as_str(), blob_oid);
    assert_eq!(parsed.parents.len(), 2);

    for bad in [
        format!("parent {blob_oid}\ntree {blob_oid}\nauthor a\ncommitter a\n\n"),
        format!("tree {blob_oid}\nauthor A\nparent {blob_oid}\ncommitter A\n\n"),
        format!("tree {blob_oid}\nauthor A\n\n"),
        format!("tree {blob_oid}\nauthor A\ncommitter B\ntree extra\n\n"),
        format!("tree {blob_oid}\nauthor A\ncommitter B\n cont-without-ext\n\n"),
        format!("tree {blob_oid}\r\nauthor A\ncommitter B\n\n"),
        format!("tree {blob_oid}\nauthor A\ncommitter B\n"),
    ] {
        assert_eq!(
            parse_commit(format, bad.as_bytes()).unwrap_err(),
            Error::ObjectUnreadable,
            "case {bad:?}"
        );
    }
}
