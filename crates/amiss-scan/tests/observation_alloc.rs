use std::alloc::System;

use amiss_scan::observe::{ObservationIdentity, observation_digest, observation_input};
use amiss_scan::resolve::Intent;
use amiss_wire::controls::{SourceConstruct, TargetKind};
use amiss_wire::digest::{hb, hj};
use amiss_wire::model::{Adapter, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::IntentKind;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn borrowed_observation_hash_allocates_no_heap_and_matches_canonical_projection() {
    let mut bytes = b"docs/".to_vec();
    bytes.extend(std::iter::repeat_n(0xff, 4096 - bytes.len()));
    let document = RepoPath::from_bytes(bytes).unwrap();
    let intent = Intent {
        kind: IntentKind::RepositoryPath,
        commit_oid: Oid::new(ObjectFormat::Sha1, "a".repeat(40)),
        repository_path: Some(document.clone()),
        target_kind: Some(TargetKind::Blob),
        external_scheme: None,
        query: Some("escaped \"\\\nβ".repeat(8192)),
        fragment: Some(String::new()),
    };
    let identity = ObservationIdentity {
        adapter: Adapter::Markdown,
        contract_digest: hb("test", b"adapter"),
        document: &document,
        construct: SourceConstruct::InlineLink,
        node_path: &[0, 42, 65_536],
        projection_digest: hb("test", b"projection"),
        intent: &intent,
        raw_destination_digest: hb("test", b"destination"),
    };
    let expected = hj(
        amiss_scan::observe::OBSERVATION_ID_DOMAIN,
        &observation_input(&identity).unwrap(),
    );
    let region = Region::new(GLOBAL);
    let actual = observation_digest(&identity).unwrap();
    let stats = region.change();
    assert_eq!(actual, expected);
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    let invalid_path = [usize::MAX];
    let invalid = ObservationIdentity {
        node_path: &invalid_path,
        ..identity
    };
    if u64::try_from(usize::MAX).is_ok_and(|index| index > amiss_wire::codec::MAX_SAFE_INTEGER) {
        assert!(observation_digest(&invalid).is_err());
        assert!(observation_input(&invalid).is_err());
    }
}
