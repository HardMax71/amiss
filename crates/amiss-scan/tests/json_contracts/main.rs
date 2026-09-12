use amiss_fixtures::{Staged, staged_repository};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::{
    Includes, ScanLimits, ScanResources, discover,
    policy::{PROTECTED_CONTROL_EVIDENCE_DOMAIN, ProtectedState, protected_state},
    resolve::{
        RAW_EVIDENCE_DOMAIN, Resolver, TARGET_LINE_PROJECTION_DOMAIN, TARGET_PROJECTION_DOMAIN,
        TargetCache,
    },
};
use amiss_wire::{
    model::{Adapter, ObjectFormat, Oid, RepoPath},
    resolution::{BlobContent, Resolution, Target},
};
use sha2::Digest as _;

mod semantic;
mod site;

#[test]
fn blob_and_protected_control_digests_keep_their_canonical_preimages() {
    let body = b"one\r\ntwo\n";
    let fixture = staged_repository(&[
        ("README.md", Staged::File(b"# Readme\n")),
        ("plain.rs", Staged::File(body)),
        ("run.sh", Staged::Executable(body)),
        ("odd \" é.rs", Staged::Absent(body)),
    ])
    .unwrap();
    let repo = Repository::open(fixture.root(), ObjectFormat::Sha1).unwrap();
    let tree = Oid::new(ObjectFormat::Sha1, fixture.commits[0].tree.clone()).unwrap();
    let mut git = GitResources::new(GitLimits::CONTRACT);
    let mut scan = ScanResources::new(ScanLimits::CONTRACT);
    let snapshot = discover(&repo, &mut git, &mut scan, &Includes::default(), &tree).unwrap();
    let mut cache = TargetCache::default();
    let document = RepoPath::new("README.md".to_owned()).unwrap();
    for (path, mode, destination) in [
        ("plain.rs", "100644", "plain.rs"),
        ("run.sh", "100755", "run.sh"),
        ("odd \" é.rs", "100644", "odd%20%22%20%C3%A9.rs"),
    ] {
        let raw = amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(RAW_EVIDENCE_DOMAIN)
                .chain_update([0_u8])
                .chain_update(body)
                .finalize()
                .0,
        );
        let protected_preimage = format!(
            r#"{{"git_mode":"{mode}","path":{},"raw_digest":"{raw}"}}"#,
            serde_json::to_string(path).unwrap()
        );
        assert_eq!(
            protected_state(&repo, &mut git, &mut scan, &snapshot.entries, path).unwrap(),
            ProtectedState::Present(amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix(PROTECTED_CONTROL_EVIDENCE_DOMAIN)
                    .chain_update([0_u8])
                    .chain_update(protected_preimage.as_bytes())
                    .finalize()
                    .0
            ))
        );
        for (suffix, selected, domain) in [
            ("", body.as_slice(), TARGET_PROJECTION_DOMAIN),
            ("#L2", b"two\n".as_slice(), TARGET_LINE_PROJECTION_DOMAIN),
        ] {
            let selected_raw = amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix(RAW_EVIDENCE_DOMAIN)
                    .chain_update([0_u8])
                    .chain_update(selected)
                    .finalize()
                    .0,
            );
            let preimage = format!(r#"{{"git_mode":"{mode}","raw_digest":"{selected_raw}"}}"#);
            let (_, resolution) = Resolver::new(&repo, &mut git, &mut scan, &mut cache, &snapshot)
                .resolve(
                    None,
                    Adapter::Markdown,
                    &document,
                    false,
                    &format!("{destination}{suffix}"),
                )
                .unwrap();
            let Resolution::Resolved {
                target: Target::Blob(blob),
            } = resolution
            else {
                panic!("unexpected resolution: {resolution:?}");
            };
            assert_eq!(
                blob.content,
                BlobContent::Available {
                    raw_digest: raw,
                    projection_digest: amiss_wire::model::Digest::from(
                        sha2::Sha256::new_with_prefix(domain)
                            .chain_update([0_u8])
                            .chain_update(preimage.as_bytes())
                            .finalize()
                            .0
                    ),
                }
            );
        }
    }
}
