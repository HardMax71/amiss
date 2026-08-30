#![expect(
    clippy::unwrap_used,
    reason = "fixtures construct known-valid paths, objects, and repositories"
)]

use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::{
    Error, RepositoryProjectionLimits, RepositoryProjectionRequest, project_repository,
};
use amiss_wire::controls::{
    BlobLineSelection, NamedRegionSelection, ProjectionKind, ProjectionSource, RecordSetSelection,
    TreePathSelection,
};
use amiss_wire::digest::sha256;
use amiss_wire::model::{ArtifactId, ObjectFormat, Oid, RepoPathText};

fn path(raw: &str) -> RepoPathText {
    RepoPathText::new(raw.to_owned()).unwrap()
}

fn project(
    pair: &amiss_fixtures::CommitPair,
    projection: ProjectionKind,
    source: &ProjectionSource,
    limits: RepositoryProjectionLimits,
) -> Result<amiss_scan::RepositoryProjectionOutcome, Error> {
    let repository = Repository::open(pair.root(), ObjectFormat::Sha1).unwrap();
    let tree = Oid::new(ObjectFormat::Sha1, pair.candidate_tree.clone()).unwrap();
    let mut git = GitResources::new(GitLimits::CONTRACT);
    project_repository(RepositoryProjectionRequest {
        repository: &repository,
        git: &mut git,
        tree: &tree,
        projection,
        source,
        limits,
    })
}

fn limits(records: u64, bytes: u64) -> RepositoryProjectionLimits {
    RepositoryProjectionLimits { records, bytes }
}

#[test]
fn line_and_named_region_sources_share_the_code_text_canonicalization() {
    let pair = amiss_fixtures::commit_pair(
        &[("README.md", "base\n")],
        &[
            ("lines.txt", "zero\r\none\r\ntwo\r\n"),
            ("region.txt", "BEGIN\r\none\r\ntwo\r\nEND\r\n"),
        ],
    )
    .unwrap();
    let lines = ProjectionSource::BlobLines(BlobLineSelection {
        path: path("lines.txt"),
        first_line: 2,
        last_line: 3,
    });
    let region = ProjectionSource::NamedRegion(NamedRegionSelection {
        path: path("region.txt"),
        start_marker: "BEGIN".to_owned(),
        end_marker: "END".to_owned(),
    });

    for source in [&lines, &region] {
        let outcome = project(&pair, ProjectionKind::CodeTextV1, source, limits(1, 1_024)).unwrap();
        assert_eq!(
            outcome.value,
            Some(amiss_wire::relation::RelationProjectedValue {
                value_digest: sha256(b"one\ntwo"),
                value_bytes: 7,
            })
        );
        assert_eq!(outcome.records, 1);
        assert!(outcome.bytes >= 7);
    }
}

#[test]
fn complete_tree_paths_project_sorted_rows_or_their_decimal_count() {
    let pair = amiss_fixtures::commit_pair(
        &[("README.md", "base\n")],
        &[
            ("docs/z.md", "z\n"),
            ("docs/a.md", "a\n"),
            ("docs/nested/b.md", "b\n"),
            ("docs/nested/deep/c.md", "c\n"),
            ("docs/notes.txt", "notes\n"),
        ],
    )
    .unwrap();
    let source = ProjectionSource::TreePaths(TreePathSelection {
        root: path("docs"),
        suffix: Some(".md".to_owned()),
        maximum_depth: 2,
    });

    let rows = project(
        &pair,
        ProjectionKind::SortedRowsV1,
        &source,
        limits(3, 1_024),
    )
    .unwrap();
    assert_eq!(
        rows.value,
        Some(amiss_wire::relation::RelationProjectedValue {
            value_digest: sha256(b"a.md\nnested/b.md\nz.md"),
            value_bytes: 21,
        })
    );
    assert_eq!((rows.records, rows.bytes), (3, 21));

    let count = project(
        &pair,
        ProjectionKind::DecimalCountV1,
        &source,
        limits(3, 1_024),
    )
    .unwrap();
    assert_eq!(
        count.value,
        Some(amiss_wire::relation::RelationProjectedValue {
            value_digest: sha256(b"3"),
            value_bytes: 1,
        })
    );
    assert_eq!((count.records, count.bytes), (3, 19));
}

#[test]
fn exact_count_includes_selected_paths_that_cannot_form_rows() {
    let mut pair = amiss_fixtures::commit_pair(&[("README.md", "base\n")], &[]).unwrap();
    let blob = amiss_fixtures::loose_object(pair.root(), "blob", b"value").unwrap();
    let docs = amiss_fixtures::tree_object(
        pair.root(),
        &[("100644", b"a.md", &blob), ("100644", b"bad\xff.md", &blob)],
    )
    .unwrap();
    pair.candidate_tree = amiss_fixtures::tree_object(
        pair.root(),
        &[("40000", b"docs", &docs), ("100644", b"README.md", &blob)],
    )
    .unwrap();
    let source = ProjectionSource::TreePaths(TreePathSelection {
        root: path("docs"),
        suffix: Some(".md".to_owned()),
        maximum_depth: 1,
    });

    let count = project(
        &pair,
        ProjectionKind::DecimalCountV1,
        &source,
        limits(2, 1_024),
    )
    .unwrap();
    assert_eq!(
        count.value,
        Some(amiss_wire::relation::RelationProjectedValue {
            value_digest: sha256(b"2"),
            value_bytes: 1,
        })
    );
    assert_eq!(count.records, 2);
    assert_eq!(
        project(
            &pair,
            ProjectionKind::SortedRowsV1,
            &source,
            limits(2, 1_024),
        )
        .unwrap()
        .value,
        None
    );
}

#[test]
fn refused_paths_only_hide_values_selected_from_their_own_root() {
    let mut pair = amiss_fixtures::commit_pair(&[("README.md", "base\n")], &[]).unwrap();
    let blob = amiss_fixtures::loose_object(pair.root(), "blob", b"value").unwrap();
    let docs = amiss_fixtures::tree_object(pair.root(), &[("100644", b"a.md", &blob)]).unwrap();
    let oversized = vec![b'x'; 4_097];
    pair.candidate_tree = amiss_fixtures::tree_object(
        pair.root(),
        &[("40000", b"docs", &docs), ("100644", &oversized, &blob)],
    )
    .unwrap();
    let source = ProjectionSource::TreePaths(TreePathSelection {
        root: path("docs"),
        suffix: Some(".md".to_owned()),
        maximum_depth: 1,
    });

    for projection in [ProjectionKind::SortedRowsV1, ProjectionKind::DecimalCountV1] {
        assert!(
            project(&pair, projection, &source, limits(1, 1_024))
                .unwrap()
                .value
                .is_some()
        );
    }

    let incomplete_docs = amiss_fixtures::tree_object(
        pair.root(),
        &[("100644", b"a.md", &blob), ("100644", &oversized, &blob)],
    )
    .unwrap();
    pair.candidate_tree =
        amiss_fixtures::tree_object(pair.root(), &[("40000", b"docs", &incomplete_docs)]).unwrap();
    for projection in [ProjectionKind::SortedRowsV1, ProjectionKind::DecimalCountV1] {
        assert_eq!(
            project(&pair, projection, &source, limits(1, 1_024))
                .unwrap()
                .value,
            None
        );
    }
}

#[test]
fn unavailable_repository_sources_and_external_record_sets_stay_null() {
    let pair = amiss_fixtures::commit_pair(
        &[("README.md", "base\n")],
        &[("region.txt", "BEGIN\nvalue\n")],
    )
    .unwrap();
    let sources = [
        (
            ProjectionKind::CodeTextV1,
            ProjectionSource::BlobLines(BlobLineSelection {
                path: path("missing.txt"),
                first_line: 1,
                last_line: 1,
            }),
        ),
        (
            ProjectionKind::CodeTextV1,
            ProjectionSource::NamedRegion(NamedRegionSelection {
                path: path("region.txt"),
                start_marker: "BEGIN".to_owned(),
                end_marker: "END".to_owned(),
            }),
        ),
        (
            ProjectionKind::SortedRowsV1,
            ProjectionSource::TreePaths(TreePathSelection {
                root: path("missing"),
                suffix: None,
                maximum_depth: 1,
            }),
        ),
        (
            ProjectionKind::SortedRowsV1,
            ProjectionSource::RecordSet(RecordSetSelection {
                set: ArtifactId::new("rust/public-api".to_owned()).unwrap(),
            }),
        ),
    ];

    for (projection, source) in sources {
        assert_eq!(
            project(&pair, projection, &source, limits(10, 1_024))
                .unwrap()
                .value,
            None
        );
    }
}

#[test]
fn record_and_byte_ceilings_stop_projection_before_a_value_is_claimed() {
    let pair = amiss_fixtures::commit_pair(
        &[("README.md", "base\n")],
        &[("docs/a.md", "a\n"), ("docs/b.md", "b\n")],
    )
    .unwrap();
    let tree = ProjectionSource::TreePaths(TreePathSelection {
        root: path("docs"),
        suffix: Some(".md".to_owned()),
        maximum_depth: 1,
    });
    assert!(matches!(
        project(&pair, ProjectionKind::SortedRowsV1, &tree, limits(1, 1_024)),
        Err(Error::ResourceLimit { .. })
    ));

    let blob = ProjectionSource::BlobLines(BlobLineSelection {
        path: path("docs/a.md"),
        first_line: 1,
        last_line: 1,
    });
    assert!(matches!(
        project(&pair, ProjectionKind::CodeTextV1, &blob, limits(1, 1)),
        Err(Error::ResourceLimit { .. })
    ));
}
