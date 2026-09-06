use std::{fs, sync::Arc};

use amiss_controller::{FileRelationScheduleStore, RelationAdmission};
use amiss_controller_fixtures::relation::relation_audit;
use amiss_wire::{controls::ProjectionKind, digest::sha256};

#[test]
fn every_projection_source_preserves_durable_schedule_bytes() {
    let cases = [
        (
            ProjectionKind::CodeTextV1,
            r#"{"kind":"blob-lines","path":"reference/é \".md","first_line":1,"last_line":9007199254740991}"#,
            "sha256:397565436e22ca8cdf7cc98c0c804b0155b366ee0fb9bf5d0122622351f1d245",
        ),
        (
            ProjectionKind::CodeTextV1,
            r#"{"kind":"named-region","path":"reference/é \".md","start_marker":"API \"start\"","end_marker":"API end"}"#,
            "sha256:ce93e5a27e7cc70f31d159f804bb0f7bd1f276bc369a885cd8a38b7439aa0ad2",
        ),
        (
            ProjectionKind::DecimalCountV1,
            r#"{"kind":"tree-paths","root":"reference","suffix":".md","maximum_depth":3}"#,
            "sha256:7210ae5e69b234e091a97285b9c4e45fecdc7e02a655c53d6f14cd925461cd1b",
        ),
        (
            ProjectionKind::SortedRowsV1,
            r#"{"kind":"tree-paths","root":"reference","maximum_depth":1}"#,
            "sha256:14decd8d341c5a931b0601f1d6294560376fb3838b91d223dee8b60cdaec4c77",
        ),
        (
            ProjectionKind::CodeTextV1,
            r#"{"kind":"record-value","set":"rust/public-api","key":"é \"item\""}"#,
            "sha256:8dada2429ece99523690a28a3c5aa93d3493deb40200a677642f143f14fdd70b",
        ),
        (
            ProjectionKind::SortedRowsV1,
            r#"{"kind":"record-set","set":"rust/public-api"}"#,
            "sha256:698df6fca07f95c83010a95770b31bb9d761e33b9234b338a65caf28dfe5daae",
        ),
    ];
    let fixture = relation_audit(true).unwrap().transition;
    for (projection, source, expected_digest) in cases {
        let mut transition = fixture.clone();
        let plan = Arc::make_mut(&mut transition.relation.plan);
        plan.projection = projection;
        for subject in &mut plan.subjects {
            subject.source = serde_json::from_str(source).unwrap();
        }
        let directory = tempfile::tempdir().unwrap();
        let store = FileRelationScheduleStore::open(directory.path(), 4).unwrap();
        let RelationAdmission::Scheduled(first) = store.schedule(transition.clone()).unwrap()
        else {
            panic!("a fresh transition schedules");
        };
        let journal = directory.path().join(".amiss-relation-schedules.journal");
        let bytes = fs::read(&journal).unwrap();
        assert_eq!(
            sha256(&bytes).to_string(),
            expected_digest,
            "journal drift for {projection:?}, source: {source}"
        );
        drop(store);

        let reopened = FileRelationScheduleStore::open(directory.path(), 4).unwrap();
        let RelationAdmission::Duplicate(repeated) = reopened.schedule(transition).unwrap() else {
            panic!("reopening preserves the exact binding");
        };
        assert_eq!(repeated, first);
        assert_eq!(fs::read(&journal).unwrap(), bytes);
    }
}
