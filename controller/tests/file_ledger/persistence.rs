use std::fs;
use std::mem::size_of;
use std::path::Path;
use std::sync::Arc;

use amiss_controller::{
    ArtifactReference, ControllerClock, DeliveryClaim, DeliveryLedger, FileLedger, FileLedgerError,
    LeaseCompletion, StageOutcome,
};
use amiss_wire::digest::{hb, sha256};
use tempfile::TempDir;

use super::support::{
    MAX_RECORDS, TestClock, assert_frame_contract, check_binding, config, delivery, executed,
    ledger_file, open, publication, staged,
};

#[test]
fn state_files_keep_the_existing_frame_contracts() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    ledger.claim(&delivery, &check_binding()).unwrap();

    assert_frame_contract(
        &directory.path().join(".amiss-root.state"),
        b"AMISS-DELIVERY-ROOT",
        "amiss/controller-file-root-frame-v1",
        4_096,
        "amiss/controller-file-root-v2",
    );
    assert_frame_contract(
        &directory.path().join(".amiss-capacity.state"),
        b"AMISS-DELIVERY-CAPACITY",
        "amiss/controller-file-capacity-frame-v1",
        4_096,
        "amiss/controller-file-capacity-v1",
    );
    assert_frame_contract(
        &ledger_file(directory.path(), ".state").unwrap(),
        b"AMISS-DELIVERY-RECORD",
        "amiss/controller-file-record-v1",
        131_072,
        "amiss/controller-file-record-v3",
    );
}

#[test]
fn staged_bytes_survive_reopen_and_completion_is_repeat_safe() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    let mut publication = publication(&delivery, &lease);
    let mut mismatched = publication.clone();
    mismatched.artifact = Some(ArtifactReference {
        id: "f".repeat(64),
        locator: format!("https://amiss.example/artifacts/{}/report", "f".repeat(64)),
        expires_at_unix_millis: 2_000,
        report_digest: sha256(b"another report"),
        semantic_digest: None,
        assessment_digest: None,
        external_tally: None,
        external_incomplete: false,
    });
    assert!(matches!(
        ledger.stage(&delivery, &lease, &mismatched),
        Err(FileLedgerError::Corrupt)
    ));
    publication.artifact = Some(ArtifactReference {
        id: "a".repeat(64),
        locator: format!("https://amiss.example/artifacts/{}/report", "a".repeat(64)),
        expires_at_unix_millis: 2_000,
        report_digest: sha256(publication.report.as_deref().unwrap()),
        semantic_digest: Some(sha256(b"semantic")),
        assessment_digest: None,
        external_tally: None,
        external_incomplete: false,
    });
    let frozen = staged(ledger.stage(&delivery, &lease, &publication).unwrap()).unwrap();

    assert_eq!(
        ledger.stage(&delivery, &lease, &publication).unwrap(),
        StageOutcome::Staged(frozen.clone())
    );
    drop(ledger);

    let mut reopened = open(directory.path(), &clock);
    assert_eq!(
        reopened.claim(&delivery, &check_binding()).unwrap(),
        DeliveryClaim::Publish(frozen.clone())
    );
    let report_path = ledger_file(directory.path(), ".report").unwrap();
    assert_eq!(
        reopened.complete(&delivery, &frozen).unwrap(),
        LeaseCompletion::Completed
    );
    assert!(!report_path.exists());
    fs::write(&report_path, b"orphaned report").unwrap();
    assert_eq!(
        reopened.complete(&delivery, &frozen).unwrap(),
        LeaseCompletion::Completed
    );
    assert!(!report_path.exists());
    drop(reopened);

    let mut after_restart = open(directory.path(), &clock);
    assert_eq!(
        after_restart.claim(&delivery, &check_binding()).unwrap(),
        DeliveryClaim::Duplicate {
            evaluation_id: lease.evaluation_id
        }
    );
}

#[test]
fn staged_v3_without_a_gate_commit_is_reexecuted_before_publication() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let original = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    let original_publication = publication(&delivery, &original);
    ledger
        .stage(&delivery, &original, &original_publication)
        .unwrap();
    let report_path = ledger_file(directory.path(), ".report").unwrap();
    assert!(report_path.exists());
    drop(ledger);

    remove_gate_commit(directory.path());
    let mut reopened = open(directory.path(), &clock);
    let recovered = executed(reopened.claim(&delivery, &check_binding()).unwrap()).unwrap();
    assert_eq!(recovered.evaluation_id, original.evaluation_id);
    assert!(recovered.fence.get() > original.fence.get());
    assert!(!report_path.exists());

    let replacement = publication(&delivery, &recovered);
    assert!(matches!(
        reopened.stage(&delivery, &recovered, &replacement).unwrap(),
        StageOutcome::Staged(staged) if staged.publication.as_ref() == &replacement
    ));
}

#[test]
fn corrupt_state_or_report_fails_closed() {
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let state_directory = TempDir::new().unwrap();
    let mut state_ledger = open(state_directory.path(), &clock);
    state_ledger.claim(&delivery, &check_binding()).unwrap();
    fs::write(
        ledger_file(state_directory.path(), ".state").unwrap(),
        b"truncated",
    )
    .unwrap();
    assert!(matches!(
        state_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));

    let report_directory = TempDir::new().unwrap();
    let mut report_ledger = open(report_directory.path(), &clock);
    let lease = executed(report_ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    let publication = publication(&delivery, &lease);
    report_ledger
        .stage(&delivery, &lease, &publication)
        .unwrap();
    fs::write(
        ledger_file(report_directory.path(), ".report").unwrap(),
        b"tampered",
    )
    .unwrap();
    assert!(matches!(
        report_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));
}

#[cfg(unix)]
#[test]
fn an_unreadable_file_is_an_error_not_an_absence() {
    use std::os::unix::fs::PermissionsExt;

    let unreadable = |path: &Path| {
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
    };
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");

    let record_directory = TempDir::new().unwrap();
    let mut record_ledger = open(record_directory.path(), &clock);
    record_ledger.claim(&delivery, &check_binding()).unwrap();
    unreadable(&ledger_file(record_directory.path(), ".state").unwrap());
    assert!(matches!(
        record_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Io(_))
    ));

    let metadata_directory = TempDir::new().unwrap();
    drop(open(metadata_directory.path(), &clock));
    unreadable(&metadata_directory.path().join(".amiss-root.state"));
    let clock_source: Arc<dyn ControllerClock> = clock.clone();
    assert!(matches!(
        FileLedger::open_with_clock(metadata_directory.path(), config(MAX_RECORDS), clock_source),
        Err(FileLedgerError::Io(_))
    ));

    let capacity_directory = TempDir::new().unwrap();
    let mut capacity_ledger = open(capacity_directory.path(), &clock);
    unreadable(&capacity_directory.path().join(".amiss-capacity.state"));
    assert!(matches!(
        capacity_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Io(_))
    ));

    let report_directory = TempDir::new().unwrap();
    let mut report_ledger = open(report_directory.path(), &clock);
    let lease = executed(report_ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    let staged_publication = publication(&delivery, &lease);
    report_ledger
        .stage(&delivery, &lease, &staged_publication)
        .unwrap();
    unreadable(&ledger_file(report_directory.path(), ".report").unwrap());
    assert!(
        matches!(
            report_ledger.claim(&delivery, &check_binding()),
            Err(FileLedgerError::Io(_))
        ),
        "an unreadable report is an error, not a missing one"
    );
}

/// A staged row without its report is corrupt at the root scan, before any
/// claim asks for the bytes.
#[test]
fn a_staged_row_without_its_report_is_corrupt_at_reopen() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    ledger
        .stage(&delivery, &lease, &publication(&delivery, &lease))
        .unwrap();
    drop(ledger);
    fs::remove_file(ledger_file(directory.path(), ".report").unwrap()).unwrap();

    let clock_source: Arc<dyn ControllerClock> = clock.clone();
    assert!(matches!(
        FileLedger::open_with_clock(directory.path(), config(MAX_RECORDS), clock_source),
        Err(FileLedgerError::Corrupt)
    ));
}

#[cfg(unix)]
#[test]
fn a_state_file_replaced_by_a_symlink_is_corrupt_at_claim() {
    use std::os::unix::fs::symlink;

    let directory = TempDir::new().unwrap();
    let aside = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    ledger.claim(&delivery, &check_binding()).unwrap();
    let state = ledger_file(directory.path(), ".state").unwrap();
    let target = aside.path().join("record");
    fs::rename(&state, &target).unwrap();
    symlink(&target, &state).unwrap();

    assert!(matches!(
        ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));
}

#[test]
fn a_missing_staged_report_is_corrupt() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    ledger
        .stage(&delivery, &lease, &publication(&delivery, &lease))
        .unwrap();
    fs::remove_file(ledger_file(directory.path(), ".report").unwrap()).unwrap();

    assert!(matches!(
        ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));
}

#[test]
fn impossible_but_checksummed_states_fail_closed() {
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let expiry_directory = TempDir::new().unwrap();
    let mut expiry_ledger = open(expiry_directory.path(), &clock);
    expiry_ledger.claim(&delivery, &check_binding()).unwrap();
    rewrite_state(expiry_directory.path(), |text| {
        let last_seen = field(text, "last_seen_unix_millis");
        replace_field(text, "expires_at_unix_millis", &last_seen.to_string())
    });
    assert!(matches!(
        expiry_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));

    let fence_directory = TempDir::new().unwrap();
    let mut fence_ledger = open(fence_directory.path(), &clock);
    fence_ledger.claim(&delivery, &check_binding()).unwrap();
    rewrite_state(fence_directory.path(), |text| {
        let generation = field(text, "generation");
        replace_field(text, "fence", &generation.saturating_add(1).to_string())
    });
    assert!(matches!(
        fence_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));
}

#[test]
fn malformed_record_and_publication_check_bindings_fail_closed() {
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");

    let record_directory = TempDir::new().unwrap();
    let mut record_ledger = open(record_directory.path(), &clock);
    record_ledger.claim(&delivery, &check_binding()).unwrap();
    rewrite_state(record_directory.path(), |text| {
        replace_string(
            text,
            r#""required_status_name":""#,
            r#""required_status_name":" "#,
        )
    });
    assert!(matches!(
        record_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));

    let publication_directory = TempDir::new().unwrap();
    let mut publication_ledger = open(publication_directory.path(), &clock);
    let lease = executed(
        publication_ledger
            .claim(&delivery, &check_binding())
            .unwrap(),
    )
    .unwrap();
    publication_ledger
        .stage(&delivery, &lease, &publication(&delivery, &lease))
        .unwrap();
    rewrite_state(publication_directory.path(), |text| {
        let last = text.rfind(r#""required_status_name":""#).unwrap();
        let (head, tail) = text.split_at(last);
        format!(
            "{head}{}",
            replace_string(
                tail,
                r#""required_status_name":""#,
                r#""required_status_name":" "#
            )
        )
    });
    assert!(matches!(
        publication_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));
}

/// Rewrites the record payload as text. A serde roundtrip would reorder the
/// members and fail the frame's canonical check before any field is read, so
/// every state edit splices bytes in place instead.
fn rewrite_state(root: &Path, change: impl FnOnce(&str) -> String) {
    rewrite_payload(root, |payload| {
        let text = std::str::from_utf8(payload).unwrap();
        let edited = change(text);
        assert_ne!(edited, text, "the edit must land");
        edited.into_bytes()
    });
}

fn field(text: &str, key: &str) -> i64 {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle).unwrap().saturating_add(needle.len());
    let rest = text.get(start..).unwrap();
    let end = rest
        .find(|character: char| !character.is_ascii_digit() && character != '-')
        .unwrap();
    rest.get(..end).unwrap().parse().unwrap()
}

fn replace_field(text: &str, key: &str, value: &str) -> String {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle).unwrap();
    let value_start = start.saturating_add(needle.len());
    let rest = text.get(value_start..).unwrap();
    let end = rest
        .find(|character: char| !character.is_ascii_digit() && character != '-')
        .unwrap();
    format!(
        "{}{needle}{value}{}",
        text.get(..start).unwrap(),
        rest.get(end..).unwrap()
    )
}

/// Swaps the first character of a value for another hex digit, so the field
/// stays well formed and only its identity changes.
fn retint(text: &str, needle: &str) -> String {
    let at = text.find(needle).unwrap().saturating_add(needle.len());
    let next = at.saturating_add(1);
    let replacement = if text.get(at..next) == Some("a") {
        "b"
    } else {
        "a"
    };
    format!(
        "{}{replacement}{}",
        text.get(..at).unwrap(),
        text.get(next..).unwrap()
    )
}

fn replace_string(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "{from} is present");
    text.replacen(from, to, 1)
}

fn remove_gate_commit(root: &Path) {
    const FIELD: &[u8] = b",\"gate_commit\":\"";

    rewrite_payload(root, |payload| {
        let start = payload
            .windows(FIELD.len())
            .position(|window| window == FIELD)
            .unwrap();
        let value_start = start.checked_add(FIELD.len()).unwrap();
        let value_bytes = payload.get(value_start..).unwrap();
        let value_end = value_bytes.iter().position(|byte| *byte == b'"').unwrap();
        let end = value_start
            .checked_add(value_end)
            .and_then(|offset| offset.checked_add(1))
            .unwrap();
        let removed = end.checked_sub(start).unwrap();
        let mut legacy = Vec::with_capacity(payload.len().checked_sub(removed).unwrap());
        legacy.extend_from_slice(payload.get(..start).unwrap());
        legacy.extend_from_slice(payload.get(end..).unwrap());
        legacy
    });
}

fn rewrite_payload(root: &Path, change: impl FnOnce(&[u8]) -> Vec<u8>) {
    const MAGIC: &[u8] = b"AMISS-DELIVERY-RECORD";
    const VERSION: u8 = 1;
    const DIGEST_BYTES: usize = 32;
    const DOMAIN: &str = "amiss/controller-file-record-v1";

    let path = ledger_file(root, ".state").unwrap();
    let frame = fs::read(&path).unwrap();
    let header_bytes = MAGIC
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(size_of::<u64>()))
        .and_then(|length| length.checked_add(DIGEST_BYTES))
        .unwrap();
    let payload = frame.get(header_bytes..).unwrap();
    let payload = change(payload);
    let payload_length = u64::try_from(payload.len()).unwrap();
    let mut frame = Vec::with_capacity(header_bytes.checked_add(payload.len()).unwrap());
    frame.extend_from_slice(MAGIC);
    frame.push(VERSION);
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(hb(DOMAIN, &payload).as_bytes());
    frame.extend_from_slice(&payload);
    fs::write(path, frame).unwrap();
}

/// How far to drive a row before its bytes are edited.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reached {
    Running,
    Staged,
    Done,
}

fn refuses_edited_state(reached: Reached, edit: fn(&str) -> String, reason: &str) {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    if reached != Reached::Running {
        let publication = publication(&delivery, &lease);
        let frozen = staged(ledger.stage(&delivery, &lease, &publication).unwrap()).unwrap();
        if reached == Reached::Done {
            ledger.complete(&delivery, &frozen).unwrap();
        }
    }
    rewrite_state(directory.path(), edit);
    assert!(
        matches!(
            ledger.claim(&delivery, &check_binding()),
            Err(FileLedgerError::Corrupt)
        ),
        "{reason}"
    );
}

/// Every row carries one impossible field behind a valid seal, so the refusal
/// is the record validator's own rather than the frame's.
#[test]
fn one_impossible_field_fails_the_record_closed() {
    type Defect = fn(&str) -> String;
    let rows: [(&str, Reached, Defect); 15] = [
        ("a foreign schema", Reached::Running, |text| {
            replace_string(text, "file-record-v3", "file-record-v0")
        }),
        ("a generation before the first", Reached::Running, |text| {
            replace_field(text, "generation", "0")
        }),
        (
            "a last-seen instant before the epoch",
            Reached::Running,
            |text| replace_field(text, "last_seen_unix_millis", "-1"),
        ),
        ("a running fence nobody issued", Reached::Running, |text| {
            replace_field(text, "fence", "0")
        }),
        ("a staged fence nobody issued", Reached::Staged, |text| {
            replace_field(text, "fence", "0")
        }),
        (
            "a staged fence past its generation",
            Reached::Staged,
            |text| {
                let generation = field(text, "generation");
                replace_field(text, "fence", &generation.saturating_add(1).to_string())
            },
        ),
        ("a done fence nobody issued", Reached::Done, |text| {
            replace_field(text, "fence", "0")
        }),
        ("a done fence past its generation", Reached::Done, |text| {
            let generation = field(text, "generation");
            replace_field(text, "fence", &generation.saturating_add(1).to_string())
        }),
        ("a done digest off the wire", Reached::Done, |text| {
            replace_string(text, "sha256:", "sha256!")
        }),
        (
            "a gate commit off the object format",
            Reached::Staged,
            |text| replace_last(text, r#""gate_commit":""#, r#""gate_commit":"z"#),
        ),
        ("another evaluation", Reached::Staged, |text| {
            replace_last(text, r#""evaluation_id":""#, r#""evaluation_id":"other-"#)
        }),
        ("another provider run", Reached::Staged, |text| {
            replace_last(text, r#""run_id":""#, r#""run_id":"other-"#)
        }),
        ("another change", Reached::Staged, |text| {
            replace_last(text, r#""change":""#, r#""change":"9"#)
        }),
        ("a run naming another candidate", Reached::Staged, |text| {
            let at = text.find(r#""commits":"#).unwrap();
            let (head, tail) = text.split_at(at);
            format!("{head}{}", retint(tail, r#""candidate":""#))
        }),
        ("another status name", Reached::Staged, |text| {
            replace_last(
                text,
                r#""required_status_name":""#,
                r#""required_status_name":"other/"#,
            )
        }),
    ];
    for (reason, reached, edit) in rows {
        refuses_edited_state(reached, edit, reason);
    }
}

/// Replaces the last occurrence, which is the publication's own copy of a
/// member the record also carries.
fn replace_last(text: &str, from: &str, to: &str) -> String {
    let at = text.rfind(from).unwrap();
    let (head, tail) = text.split_at(at);
    format!("{head}{}", replace_string(tail, from, to))
}

/// A fence at its generation is the one the row holds, in every state; the
/// natural flow advances the generation past it, so it is written here.
#[test]
fn a_fence_at_its_generation_is_current_when_staged_or_done() {
    for complete_it in [false, true] {
        let directory = TempDir::new().unwrap();
        let clock = TestClock::at(1_000);
        let delivery = delivery("42");
        let mut ledger = open(directory.path(), &clock);
        let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
        let publication = publication(&delivery, &lease);
        let frozen = staged(ledger.stage(&delivery, &lease, &publication).unwrap()).unwrap();
        if complete_it {
            ledger.complete(&delivery, &frozen).unwrap();
        }
        rewrite_state(directory.path(), |text| {
            let generation = field(text, "generation");
            replace_field(text, "fence", &generation.to_string())
        });
        assert!(
            !matches!(
                ledger.claim(&delivery, &check_binding()),
                Err(FileLedgerError::Corrupt)
            ),
            "completed: {complete_it}"
        );
    }
}

/// A stored report reference is held to its own length and digest, and the
/// report on disk must answer both.
#[test]
fn a_report_reference_binds_length_and_digest() {
    type Defect = fn(&str) -> String;
    let edits: [(&str, Defect); 3] = [
        ("a length past the wire ceiling", |text| {
            let length = field(text, "length");
            assert!(length > 0, "the fixture report has bytes");
            replace_field(text, "length", "268435457")
        }),
        ("a digest off the wire", |text| {
            replace_last(text, r#""digest":"sha256:"#, r#""digest":"sha256!"#)
        }),
        ("a length the report does not have", |text| {
            let length = field(text, "length");
            replace_field(text, "length", &length.saturating_add(1).to_string())
        }),
    ];
    for (reason, edit) in edits {
        refuses_edited_state(Reached::Staged, edit, reason);
    }
}
