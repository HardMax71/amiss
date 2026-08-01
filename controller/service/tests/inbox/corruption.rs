use std::fs;
use std::path::Path;

use amiss_controller_service::{Inbox, InboxError};
use cap_fs_ext::DirExt as _;
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use tempfile::TempDir;

use super::support::{incoming, limits, open, reseal_row, row_file};

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

/// The store admits deliveries until its byte budget is spent, then names
/// Full; the record-side bound is proven by the limits contract itself,
/// since the reservation is the worst case an admissible delivery encodes.
#[test]
fn the_store_fills_and_refuses() {
    let directory = tempfile::tempdir().unwrap();
    let mut inbox = open(directory.path());
    let body = vec![b'x'; 4_000];
    let mut admitted = 0_u32;
    for index in 0..64 {
        let source = format!("delivery-{index}");
        match inbox.enqueue(incoming(&source, &body)) {
            Ok(_) => admitted += 1,
            Err(InboxError::Full) => break,
            Err(other) => panic!("unexpected refusal: {other:?}"),
        }
    }
    assert!(
        (1..64).contains(&admitted),
        "the store admits some and then fills: {admitted}"
    );
}

#[test]
fn reopening_with_other_limits_is_a_configuration_error() {
    let directory = tempfile::tempdir().unwrap();
    drop(open(directory.path()));
    let mut changed = limits();
    changed.max_records += 1;
    assert!(matches!(
        Inbox::open(directory.path(), changed),
        Err(InboxError::Configuration)
    ));
}

#[cfg(unix)]
#[test]
fn an_unreadable_metadata_file_is_an_error_not_an_absence() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    drop(open(directory.path()));
    let metadata = directory.path().join(".amiss-inbox.state");
    fs::set_permissions(&metadata, fs::Permissions::from_mode(0o000)).unwrap();
    assert!(
        matches!(
            Inbox::open(directory.path(), limits()),
            Err(InboxError::Io(_))
        ),
        "an unreadable metadata file must not read as a fresh root"
    );
}

/// A fresh root adopts nothing it did not write: a stray directory, a file
/// wearing the atomic prefix, and a directory wearing the lock name each
/// refuse rather than vanish.
#[test]
fn a_fresh_root_adopts_nothing_it_did_not_write() {
    type Plant = fn(&Path);
    let plants: [Plant; 3] = [
        |root| fs::create_dir(root.join("junk")).unwrap(),
        |root| fs::write(root.join(".atomicwrite-file"), b"partial").unwrap(),
        |root| fs::create_dir(root.join(".amiss-inbox.lock")).unwrap(),
    ];
    for plant in plants {
        let directory = tempfile::tempdir().unwrap();
        plant(directory.path());
        assert!(
            matches!(
                Inbox::open(directory.path(), limits()),
                Err(InboxError::Corrupt)
            ),
            "a foreign entry in a fresh root is corrupt"
        );
    }
}

/// A directory squatting on the exact row target is a corrupt store, not a
/// write destination.
#[test]
fn a_directory_on_the_row_target_is_corrupt() {
    let sized = tempfile::tempdir().unwrap();
    let mut inbox = open(sized.path());
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    let name = row_file(sized.path()).file_name().unwrap().to_owned();

    let squatted = tempfile::tempdir().unwrap();
    let mut fresh = open(squatted.path());
    fs::create_dir(squatted.path().join(&name)).unwrap();
    assert!(
        matches!(
            fresh.enqueue(incoming("delivery-1", b"body")),
            Err(InboxError::Corrupt)
        ),
        "the row target is occupied by a directory"
    );
}

type PayloadEdit = fn(&str) -> String;

/// Each row carries one semantic defect behind a valid seal, so the refusal
/// is the record validator's own, not the frame digest's.
#[test]
fn resealed_semantic_defects_fail_closed() {
    let edits: [(&str, PayloadEdit); 5] = [
        ("foreign schema", |payload| {
            payload.replace("inbox-record-v1", "inbox-record-v2")
        }),
        ("fence ahead of attempts", |payload| {
            payload.replace("\"attempts\":0,\"fence\":0", "\"attempts\":0,\"fence\":1")
        }),
        ("generation behind attempts", |payload| {
            payload.replace(
                "\"generation\":0,\"attempts\":0,\"fence\":0",
                "\"generation\":0,\"attempts\":1,\"fence\":1",
            )
        }),
        ("negative availability", |payload| {
            payload.replace(
                "\"available_at_unix_millis\":0",
                "\"available_at_unix_millis\":-1",
            )
        }),
        ("re-pointed content digest", |payload| {
            let prefix = "\"content_digest\":\"";
            let at = payload.find(prefix).unwrap().saturating_add(prefix.len());
            let flipped = if payload.get(at..=at) == Some("0") {
                "1"
            } else {
                "0"
            };
            let mut edited = payload.to_owned();
            edited.replace_range(at..=at, flipped);
            edited
        }),
    ];
    for (reason, edit) in edits {
        let directory = TempDir::new().unwrap();
        let mut inbox = open(directory.path());
        inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
        reseal_row(directory.path(), |payload| {
            let edited = edit(payload);
            assert_ne!(edited, payload, "{reason}: the edit must land");
            edited
        });
        assert!(
            matches!(inbox.entries(), Err(InboxError::Corrupt)),
            "{reason}"
        );
    }
}

/// The stored key is the file's name, so a renamed row claims an identity
/// its payload does not hash to.
#[test]
fn a_row_under_a_foreign_key_fails_closed() {
    let directory = TempDir::new().unwrap();
    let mut inbox = open(directory.path());
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    let path = row_file(directory.path());
    fs::rename(
        &path,
        path.with_file_name(format!("{}.row", "0".repeat(64))),
    )
    .unwrap();
    assert!(matches!(inbox.entries(), Err(InboxError::Corrupt)));
}

const CLAIMED_COUNTERS: &str = "\"generation\":1,\"attempts\":1,\"fence\":1";
const CLAIMED_STATE: &str = "{\"state\":\"claimed\",\"owner\":\"0123456789abcdef0123456789abcdef\",\"expires_at_unix_millis\":5000}";

fn claimed_row<'edit>(counters: &'edit str, state: &'edit str) -> impl Fn(&str) -> String + 'edit {
    move |payload: &str| {
        let edited = payload
            .replace("\"generation\":0,\"attempts\":0,\"fence\":0", counters)
            .replace(
                "{\"state\":\"pending\",\"available_at_unix_millis\":0}",
                state,
            );
        assert_ne!(edited, payload, "both claimed edits must land");
        edited
    }
}

/// A claimed row is held to its whole lease grammar: a well-formed one is
/// readable, and each broken leg fails closed on its own.
#[test]
fn a_claimed_row_answers_for_its_lease_grammar() {
    let sound = TempDir::new().unwrap();
    let mut inbox = open(sound.path());
    inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
    reseal_row(sound.path(), claimed_row(CLAIMED_COUNTERS, CLAIMED_STATE));
    assert!(inbox.entries().is_ok(), "the well-formed claimed row reads");

    let broken: [(&str, &str, &str); 4] = [
        (
            "short owner",
            CLAIMED_COUNTERS,
            "{\"state\":\"claimed\",\"owner\":\"0123456789abcdef0123456789abcde\",\"expires_at_unix_millis\":5000}",
        ),
        (
            "owner outside the hex alphabet",
            CLAIMED_COUNTERS,
            "{\"state\":\"claimed\",\"owner\":\"0123456789abcdefg123456789abcdef\",\"expires_at_unix_millis\":5000}",
        ),
        (
            "expiry at the epoch",
            CLAIMED_COUNTERS,
            "{\"state\":\"claimed\",\"owner\":\"0123456789abcdef0123456789abcdef\",\"expires_at_unix_millis\":0}",
        ),
        (
            "claimed without an attempt",
            "\"generation\":1,\"attempts\":0,\"fence\":0",
            CLAIMED_STATE,
        ),
    ];
    for (reason, counters, state) in broken {
        let directory = TempDir::new().unwrap();
        let mut inbox = open(directory.path());
        inbox.enqueue(incoming("delivery-1", b"body")).unwrap();
        reseal_row(directory.path(), claimed_row(counters, state));
        assert!(
            matches!(inbox.entries(), Err(InboxError::Corrupt)),
            "{reason}"
        );
    }
}
