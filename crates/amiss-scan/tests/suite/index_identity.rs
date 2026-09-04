use amiss_scan::report::{INDEX_PROJECTION_SCHEMA, SNAPSHOT_SCHEMA, synthetic_candidate};
use amiss_wire::controls::GitMode;
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};

#[test]
fn typed_index_identities_hash_every_entry_field_without_changing_the_wire() {
    let cases = [
        (GitMode::RegularFile, "100644", "blob"),
        (GitMode::ExecutableFile, "100755", "blob"),
        (GitMode::Symlink, "120000", "symlink"),
        (GitMode::Gitlink, "160000", "gitlink"),
        (GitMode::Tree, "040000", "blob"),
    ];
    for (format, width, name) in [
        (ObjectFormat::Sha1, 40, "sha1"),
        (ObjectFormat::Sha256, 64, "sha256"),
    ] {
        let base = Oid::new(format, "1".repeat(width)).unwrap();
        let object = Oid::new(format, "a".repeat(width)).unwrap();
        for (mode, mode_name, kind) in cases {
            for skip in [false, true] {
                let entries = [
                    (
                        RepoPath::new("docs/quoted-\"β\n.md".to_owned()).unwrap(),
                        mode,
                        object.clone(),
                        skip,
                    ),
                    (
                        RepoPath::from_bytes(b"docs/\xff.md".to_vec()).unwrap(),
                        mode,
                        object.clone(),
                        !skip,
                    ),
                ];
                let projection = serde_json::json!({
                    "schema": "amiss/scanner-index-projection",
                    "entries": [
                        {"path": "docs/quoted-\"β\n.md", "entry_kind": kind,
                         "git_mode": mode_name, "object_format": name,
                         "object_oid": "a".repeat(width), "skip_worktree": skip},
                        {"path": {"bytes_hex": "646f63732fff2e6d64"}, "entry_kind": kind,
                         "git_mode": mode_name, "object_format": name,
                         "object_oid": "a".repeat(width), "skip_worktree": !skip}
                    ]
                });
                let projection_digest = hb(
                    INDEX_PROJECTION_SCHEMA,
                    &serde_json_canonicalizer::to_vec(&projection).unwrap(),
                );
                let snapshot = serde_json::json!({
                    "schema": "amiss/scanner-snapshot", "kind": "index",
                    "identity_scope": "complete-logical-index", "base_object_format": name,
                    "base_commit_oid": "1".repeat(width),
                    "index_projection_digest": projection_digest
                });
                let candidate = synthetic_candidate(format, &base, &entries, 1).unwrap();
                assert_eq!(
                    candidate.snapshot.index_projection_digest,
                    projection_digest
                );
                assert_eq!(
                    candidate.snapshot.snapshot_digest,
                    hb(
                        SNAPSHOT_SCHEMA,
                        &serde_json_canonicalizer::to_vec(&snapshot).unwrap()
                    )
                );
                assert_eq!(candidate.snapshot.entry_count, 2);
                assert_eq!(candidate.skip_worktree_paths, 1);
                let reversed = [entries[1].clone(), entries[0].clone()];
                assert_ne!(
                    candidate.snapshot.index_projection_digest,
                    synthetic_candidate(format, &base, &reversed, 1)
                        .unwrap()
                        .snapshot
                        .index_projection_digest
                );
            }
        }
    }
}

#[test]
fn an_empty_index_still_has_a_complete_projection() {
    let base = Oid::new(ObjectFormat::Sha1, "1".repeat(40)).unwrap();
    let candidate = synthetic_candidate(ObjectFormat::Sha1, &base, &[], 0).unwrap();
    assert_eq!(candidate.snapshot.entry_count, 0);
    assert_eq!(candidate.skip_worktree_paths, 0);
    assert_eq!(
        candidate.snapshot.index_projection_digest,
        hb(
            INDEX_PROJECTION_SCHEMA,
            br#"{"entries":[],"schema":"amiss/scanner-index-projection"}"#
        )
    );
}
