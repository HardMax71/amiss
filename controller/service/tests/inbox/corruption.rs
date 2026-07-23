use std::fs;
use std::path::Path;

use amiss_controller_service::{Inbox, InboxError};
use cap_fs_ext::DirExt as _;
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use tempfile::TempDir;

use super::support::{incoming, limits, open, row_file};

#[test]
fn truncated_and_tampered_rows_fail_closed() {
    let truncated_directory = TempDir::new().unwrap();
    let mut truncated = open(truncated_directory.path());
    truncated.enqueue(incoming("delivery-1", b"body")).unwrap();
    fs::write(row_file(truncated_directory.path()), b"truncated").unwrap();
    assert!(matches!(truncated.entries(), Err(InboxError::Corrupt)));

    let tampered_directory = TempDir::new().unwrap();
    let mut tampered = open(tampered_directory.path());
    tampered.enqueue(incoming("delivery-1", b"body")).unwrap();
    let path = row_file(tampered_directory.path());
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.last_mut().unwrap();
    *last ^= 1;
    fs::write(path, bytes).unwrap();
    assert!(matches!(tampered.entries(), Err(InboxError::Corrupt)));
}

#[test]
fn non_regular_roots_and_rows_fail_closed() {
    let file_directory = TempDir::new().unwrap();
    let file = file_directory.path().join("inbox");
    fs::write(&file, b"not a directory").unwrap();
    assert!(matches!(
        Inbox::open(&file, limits()),
        Err(InboxError::Corrupt)
    ));

    let row_directory = TempDir::new().unwrap();
    let inbox = open(row_directory.path());
    drop(inbox);
    fs::create_dir(row_directory.path().join(format!("{}.row", "a".repeat(64)))).unwrap();
    assert!(matches!(
        Inbox::open(row_directory.path(), limits()),
        Err(InboxError::Corrupt)
    ));
}

#[test]
fn symlink_roots_and_rows_fail_closed_without_platform_branches() {
    let parent = TempDir::new().unwrap();
    let target_root = parent.path().join("target-root");
    fs::create_dir(&target_root).unwrap();
    let parent_dir = Dir::open_ambient_dir(parent.path(), ambient_authority()).unwrap();
    parent_dir
        .symlink_dir("target-root", "linked-root")
        .unwrap();
    assert!(matches!(
        Inbox::open(parent.path().join("linked-root"), limits()),
        Err(InboxError::Corrupt)
    ));

    let row_root = parent.path().join("rows");
    fs::create_dir(&row_root).unwrap();
    let mut inbox = open(&row_root);
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    let row = row_file(&row_root);
    let name = row.file_name().unwrap().to_owned();
    fs::remove_file(row).unwrap();
    let target = parent.path().join("target-row");
    fs::write(&target, b"row target").unwrap();
    let row_root_dir = Dir::open_ambient_dir(&row_root, ambient_authority()).unwrap();
    row_root_dir
        .symlink_file(Path::new("..").join("target-row"), name)
        .unwrap();
    assert!(matches!(inbox.entries(), Err(InboxError::Corrupt)));
}

#[test]
fn unknown_entries_and_a_second_process_owner_fail_closed() {
    let directory = TempDir::new().unwrap();
    let inbox = open(directory.path());
    assert!(matches!(
        Inbox::open(directory.path(), limits()),
        Err(InboxError::AlreadyOpen)
    ));
    drop(inbox);

    fs::write(directory.path().join("unexpected"), b"file").unwrap();
    assert!(matches!(
        Inbox::open(directory.path(), limits()),
        Err(InboxError::Corrupt)
    ));
}

#[test]
fn interrupted_atomic_writes_are_removed_only_in_the_known_shape() {
    let directory = TempDir::new().unwrap();
    let empty = directory.path().join(".atomicwrite-empty");
    let with_file = directory.path().join(".atomicwrite-file");
    fs::create_dir(&empty).unwrap();
    fs::create_dir(&with_file).unwrap();
    fs::write(with_file.join("tmpfile.tmp"), b"partial").unwrap();

    let inbox = open(directory.path());
    assert!(!empty.exists());
    assert!(!with_file.exists());
    drop(inbox);

    let malformed_directory = TempDir::new().unwrap();
    let malformed = malformed_directory.path().join(".atomicwrite-malformed");
    fs::create_dir(&malformed).unwrap();
    fs::write(malformed.join("unexpected"), b"data").unwrap();
    assert!(matches!(
        Inbox::open(malformed_directory.path(), limits()),
        Err(InboxError::Corrupt)
    ));
    assert!(malformed.exists());
}
