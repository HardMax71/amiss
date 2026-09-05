use amiss_fixtures::commit_chain;
use amiss_git::GitLimits;
use amiss_scan::resolve::{ForgeContext, RAW_EVIDENCE_DOMAIN};
use amiss_scan::{Error, Resolution, ScanLimits};
use amiss_wire::controls::{ResourceName, TargetKind};
use amiss_wire::digest::hb;
use amiss_wire::model::{Adapter, ForgeDialect, ObjectFormat, Oid};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{
    BlobContent, ExternalReference, Missing, Target, UnsupportedSemantics, VersionScope,
};

use crate::support::{Bed, bed, bed_at, forge_context};

const HISTORICAL_BODY: &str = "# Historical heading\n\nhistorical body\n";

#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test fixture helper"
)]
fn history_bed(git_limits: GitLimits) -> (Bed, String) {
    let chain = commit_chain(&[
        ("historical", &[("docs/guide.md", HISTORICAL_BODY)]),
        (
            "candidate",
            &[("docs/guide.md", "# Candidate heading\n\ncandidate body\n")],
        ),
    ])
    .unwrap();
    let historical = chain
        .commits
        .first()
        .expect("the history fixture has a commit")
        .id
        .clone();
    (
        bed_at(chain, 1, ScanLimits::CONTRACT, git_limits),
        historical,
    )
}

#[test]
fn same_repository_intents_retain_query_and_fragment() {
    let cases = [
        (
            forge_context(ForgeDialect::Github),
            IntentKind::SameRepositoryGithub,
            "https://github.com/acme/widgets/blob/feature/x/docs/guide.md?plain=1#intro",
            "plain=1",
        ),
        (
            forge_context(ForgeDialect::Gitlab),
            IntentKind::SameRepositoryGitlab,
            "https://gitlab.com/acme/widgets/-/blob/feature/x/docs/guide.md?plain=1#intro",
            "plain=1",
        ),
        (
            forge_context(ForgeDialect::Gitea),
            IntentKind::SameRepositoryGitea,
            "https://codeberg.org/acme/widgets/src/branch/feature/x/docs/guide.md?plain=1#intro",
            "plain=1",
        ),
        (
            forge_context(ForgeDialect::BitbucketCloud),
            IntentKind::SameRepositoryBitbucketCloud,
            "https://bitbucket.org/acme/widgets/src/feature/docs/guide.md?plain=1#intro",
            "plain=1",
        ),
        (
            forge_context(ForgeDialect::BitbucketDataCenter),
            IntentKind::SameRepositoryBitbucketDataCenter,
            "https://bitbucket.example/projects/ACME/repos/widgets/browse/docs/guide.md?at=refs%2Fheads%2Ffeature%2Fx#intro",
            "at=refs%2Fheads%2Ffeature%2Fx",
        ),
    ];
    for (context, kind, destination, query) in cases {
        let (intent, _resolution) = bed()
            .run_as(
                Adapter::Markdown,
                Some(&context),
                "docs/guide.md",
                false,
                destination,
            )
            .unwrap();
        assert_eq!(intent.kind, kind, "{destination}");
        assert_eq!(intent.query.as_deref(), Some(query), "{destination}");
        assert_eq!(intent.fragment.as_deref(), Some("intro"), "{destination}");
    }
}

#[test]
fn a_full_local_commit_resolves_only_in_the_declared_object_format() {
    let mut bed = bed();
    let raw = bed
        .dir
        .commits
        .first()
        .expect("the resolver fixture has a commit")
        .id
        .clone();
    let cases = [
        (
            forge_context(ForgeDialect::Github),
            format!("https://github.com/acme/widgets/blob/{raw}/docs/guide.md"),
        ),
        (
            forge_context(ForgeDialect::Gitlab),
            format!("https://gitlab.com/acme/widgets/-/blob/{raw}/docs/guide.md"),
        ),
        (
            forge_context(ForgeDialect::Gitea),
            format!("https://codeberg.org/acme/widgets/src/commit/{raw}/docs/guide.md"),
        ),
        (
            forge_context(ForgeDialect::BitbucketCloud),
            format!("https://bitbucket.org/acme/widgets/src/{raw}/docs/guide.md"),
        ),
        (
            forge_context(ForgeDialect::BitbucketDataCenter),
            format!(
                "https://bitbucket.example/projects/ACME/repos/widgets/browse/docs/guide.md?at={raw}"
            ),
        ),
    ];
    for (context, destination) in cases {
        let (intent, resolution) = bed
            .run_as(
                Adapter::Markdown,
                Some(&context),
                "docs/guide.md",
                false,
                &destination,
            )
            .unwrap_or_else(|_defect| panic!());
        assert!(
            matches!(resolution, Resolution::Resolved { .. }),
            "{destination}"
        );
        assert_eq!(
            intent.commit_oid.as_ref().map(Oid::as_str),
            Some(raw.as_str())
        );
    }

    let mut sha256 = forge_context(ForgeDialect::Github);
    sha256.object_format = ObjectFormat::Sha256;
    let (_intent, resolution) = bed
        .run_as(
            Adapter::Markdown,
            Some(&sha256),
            "docs/guide.md",
            false,
            &format!("https://github.com/acme/widgets/blob/{raw}/docs/guide.md"),
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(
        resolution,
        Resolution::UnsupportedVersion {
            scope: VersionScope::UnknownPath
        }
    );

    let full_sha256 = "a".repeat(64);
    let (_intent, resolution) = bed
        .run_as(
            Adapter::Markdown,
            Some(&sha256),
            "docs/guide.md",
            false,
            &format!("https://github.com/acme/widgets/blob/{full_sha256}/docs/guide.md"),
        )
        .unwrap_or_else(|_defect| panic!());
    let Resolution::UnsupportedVersion {
        scope: VersionScope::KnownCommit { commit_oid, path },
    } = resolution
    else {
        panic!("unexpected resolution: {resolution:?}");
    };
    assert_eq!(commit_oid.as_str(), full_sha256);
    assert_eq!(path.as_str(), Some("docs/guide.md"));
}

#[test]
fn an_exact_historical_url_reads_only_its_own_tree_and_content() {
    let (mut bed, historical) = history_bed(GitLimits::CONTRACT);
    let context = forge_context(ForgeDialect::Github);
    let destination = format!("https://github.com/acme/widgets/blob/{historical}/docs/guide.md");
    let (intent, resolution) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            &destination,
        )
        .unwrap_or_else(|defect| panic!("{defect:?}"));
    assert_eq!(
        intent.commit_oid.as_ref().map(Oid::as_str),
        Some(historical.as_str())
    );
    let Resolution::Resolved {
        target: Target::Blob(blob),
    } = resolution
    else {
        panic!("unexpected resolution: {resolution:?}");
    };
    let BlobContent::Available { raw_digest, .. } = blob.content else {
        panic!("historical content was unavailable");
    };
    assert_eq!(
        raw_digest,
        hb(RAW_EVIDENCE_DOMAIN, HISTORICAL_BODY.as_bytes())
    );

    let (_intent, old_anchor) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            &format!("{destination}#historical-heading"),
        )
        .unwrap_or_else(|defect| panic!("{defect:?}"));
    assert!(matches!(old_anchor, Resolution::Resolved { .. }));

    let (_intent, candidate_anchor) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            &format!("{destination}#candidate-heading"),
        )
        .unwrap_or_else(|defect| panic!("{defect:?}"));
    assert!(matches!(
        candidate_anchor,
        Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
    ));
}

#[test]
fn historical_absence_requires_a_complete_local_walk() {
    let (mut bed, historical) = history_bed(GitLimits::CONTRACT);
    let context = forge_context(ForgeDialect::Github);
    let (intent, missing_path) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            &format!("https://github.com/acme/widgets/blob/{historical}/docs/absent.md"),
        )
        .unwrap_or_else(|defect| panic!("{defect:?}"));
    assert_eq!(
        intent.commit_oid.as_ref().map(Oid::as_str),
        Some(historical.as_str())
    );
    assert!(matches!(
        missing_path,
        Resolution::Missing(Missing::PathNotFound { .. })
    ));

    let unavailable = "f".repeat(40);
    let (intent, missing_commit) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            &format!("https://github.com/acme/widgets/blob/{unavailable}/docs/absent.md"),
        )
        .unwrap_or_else(|defect| panic!("{defect:?}"));
    assert_eq!(
        intent.commit_oid.as_ref().map(Oid::as_str),
        Some(unavailable.as_str())
    );
    assert!(matches!(
        missing_commit,
        Resolution::UnsupportedVersion {
            scope: VersionScope::KnownCommit { .. }
        }
    ));
}

#[test]
fn historical_queries_stay_outside_unscanned_build_semantics() {
    let (mut bed, historical) = history_bed(GitLimits::CONTRACT);
    let (_intent, resolution) = bed
        .run_as(
            Adapter::Markdown,
            Some(&forge_context(ForgeDialect::Github)),
            "docs/guide.md",
            false,
            &format!("https://github.com/acme/widgets/blob/{historical}/docs/guide.md?plain=1"),
        )
        .unwrap_or_else(|defect| panic!("{defect:?}"));
    assert!(matches!(
        resolution,
        Resolution::UnsupportedSemantics(UnsupportedSemantics::Query(_))
    ));
}

#[test]
fn repeated_historical_walks_share_the_tree_entry_budget() {
    let limits = GitLimits {
        tree_entries_per_snapshot: 3,
        ..GitLimits::CONTRACT
    };
    let (mut bed, historical) = history_bed(limits);
    let destination = format!("https://github.com/acme/widgets/blob/{historical}/docs/guide.md");
    bed.run_as(
        Adapter::Markdown,
        Some(&forge_context(ForgeDialect::Github)),
        "docs/guide.md",
        false,
        &destination,
    )
    .unwrap_or_else(|defect| panic!("{defect:?}"));
    assert_eq!(
        bed.run_as(
            Adapter::Markdown,
            Some(&forge_context(ForgeDialect::Github)),
            "docs/guide.md",
            false,
            &destination,
        ),
        Err(Error::ResourceLimit {
            resource: ResourceName::GitTreeEntriesPerSnapshot,
            configured_limit: 3,
            observed_lower_bound: 4,
        })
    );
}

#[test]
fn a_ref_spelled_like_a_full_oid_is_ambiguous() {
    let raw = "0123456789012345678901234567890123456789";
    let mut context = forge_context(ForgeDialect::Github);
    context.candidate_ref = format!("refs/heads/{raw}");
    let (intent, resolution) = bed()
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            &format!("https://github.com/acme/widgets/blob/{raw}/docs/guide.md"),
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(intent.kind, IntentKind::Unsupported);
    assert_eq!(
        resolution,
        Resolution::UnsupportedVersion {
            scope: VersionScope::UnknownPath
        }
    );
}

/// The gitea family against a real tree: the typed branch form resolves
/// with an either target, the commit form is pinned to the exact candidate
/// OID, a tag spelled like the candidate branch stays version-scoped out,
/// and the untyped legacy form is foreign.
#[test]
fn gitea_recognition_resolves_against_the_tree() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::Gitea);
    let (intent, row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/branch/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(intent.kind, IntentKind::SameRepositoryGitea);
    assert_eq!(intent.target_kind, Some(TargetKind::Either));
    let Resolution::Resolved {
        target: Target::Blob(blob),
    } = row
    else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));

    let local_commit = bed
        .dir
        .commits
        .first()
        .expect("the resolver fixture has a commit")
        .id
        .clone();
    let (_intent, pinned) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            &format!("https://codeberg.org/acme/widgets/src/commit/{local_commit}/docs/guide.md"),
        )
        .unwrap_or_else(|_defect| panic!());
    assert!(
        matches!(pinned, Resolution::Resolved { .. }),
        "an available exact commit resolves from its own tree"
    );

    let (_intent, tag) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/tag/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(
        tag,
        Resolution::UnsupportedVersion {
            scope: VersionScope::UnknownPath
        },
        "a tag spelled like the candidate branch is still no trusted ref"
    );

    let (_intent, untyped) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(
        untyped,
        Resolution::External {
            reason: ExternalReference::ForeignRepository
        }
    );
}

#[test]
fn bitbucket_cloud_recognizes_only_the_documented_source_contract() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::BitbucketCloud);
    let (intent, candidate) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://bitbucket.org/acme/widgets/src/feature/src/lib.rs?fileviewer=file-view-default#lib.rs-1",
        )
        .unwrap();
    assert_eq!(intent.kind, IntentKind::SameRepositoryBitbucketCloud);
    assert_eq!(intent.target_kind, Some(TargetKind::Either));
    assert_eq!(
        intent.query.as_deref(),
        Some("fileviewer=file-view-default")
    );
    assert_eq!(intent.fragment.as_deref(), Some("lib.rs-1"));
    assert!(matches!(candidate, Resolution::Resolved { .. }));

    let (_intent, custom_viewer) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://bitbucket.org/acme/widgets/src/feature/src/lib.rs?fileviewer=custom",
        )
        .unwrap();
    assert!(matches!(
        custom_viewer,
        Resolution::UnsupportedSemantics(UnsupportedSemantics::Query(_))
    ));

    let slashed = ForgeContext {
        candidate_ref: "refs/heads/feature/x".to_owned(),
        ..context
    };
    let (_intent, no_guessed_split) = bed
        .run_as(
            Adapter::Markdown,
            Some(&slashed),
            "docs/guide.md",
            false,
            "https://bitbucket.org/acme/widgets/src/feature/x/docs/guide.md",
        )
        .unwrap();
    let Resolution::UnsupportedVersion {
        scope: VersionScope::KnownPath { path },
    } = no_guessed_split
    else {
        panic!("unexpected resolution: {no_guessed_split:?}");
    };
    assert_eq!(path.as_str(), Some("x/docs/guide.md"));
}

#[test]
fn bitbucket_data_center_recognizes_query_bound_browse_routes() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::BitbucketDataCenter);
    for destination in [
        "https://bitbucket.example/projects/ACME/repos/widgets/browse/src/lib.rs?at=refs%2Fheads%2Ffeature%2Fx#1",
        "https://bitbucket.example/bitbucket/users/acme/repos/widgets/browse/src/lib.rs?at=refs/heads/feature/x#1",
        "https://bitbucket.example/bitbucket/projects/~acme/repos/widgets/browse/src/lib.rs?at=refs%2Fheads%2Ffeature%2Fx#1",
    ] {
        let (intent, resolution) = bed
            .run_as(
                Adapter::Markdown,
                Some(&context),
                "docs/guide.md",
                false,
                destination,
            )
            .unwrap();
        assert_eq!(
            intent.kind,
            IntentKind::SameRepositoryBitbucketDataCenter,
            "{destination}"
        );
        assert!(
            matches!(resolution, Resolution::Resolved { .. }),
            "{destination}: {resolution:?}"
        );
    }
}

#[test]
fn bitbucket_data_center_keeps_unknown_revision_queries_scoped_out() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::BitbucketDataCenter);
    for query in [
        "at=feature/x",
        "at=0123456",
        "at=refs%2Fheads%2Ffeature%2Fx&raw",
        "until=6a66ef14b9b8b174a54ccf8ea4b0dd18f42f9f22&untilPath=other.md",
    ] {
        let (intent, resolution) = bed
            .run_as(
                Adapter::Markdown,
                Some(&context),
                "docs/guide.md",
                false,
                &format!(
                    "https://bitbucket.example/projects/ACME/repos/widgets/browse/docs/guide.md?{query}"
                ),
            )
            .unwrap();
        assert_eq!(intent.kind, IntentKind::Unsupported, "{query}");
        let Resolution::UnsupportedVersion {
            scope: VersionScope::KnownPath { path },
        } = resolution
        else {
            panic!("unexpected resolution for {query}: {resolution:?}");
        };
        assert_eq!(path.as_str(), Some("docs/guide.md"), "{query}");
    }
}

#[test]
fn bitbucket_data_center_history_query_binds_the_path() {
    let mut bed = bed();
    let raw = bed
        .dir
        .commits
        .first()
        .expect("the resolver fixture has a commit")
        .id
        .clone();
    let context = forge_context(ForgeDialect::BitbucketDataCenter);
    let (intent, resolution) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            &format!(
                "https://bitbucket.example/projects/ACME/repos/widgets/browse/docs/guide.md?until={raw}&untilPath=docs%2Fguide.md"
            ),
        )
        .unwrap();
    assert_eq!(
        intent.commit_oid.as_ref().map(Oid::as_str),
        Some(raw.as_str())
    );
    assert!(matches!(resolution, Resolution::Resolved { .. }));
}

/// One wrong fact makes a same-repository spelling foreign: the owner, the
/// project, or the form, each alone, on every dialect.
#[test]
fn one_wrong_fact_makes_a_foreign_url() {
    let mut bed = bed();
    let cases = [
        (
            forge_context(ForgeDialect::Github),
            "https://github.com/other/widgets/blob/main/docs/guide.md",
        ),
        (
            forge_context(ForgeDialect::Github),
            "https://github.com/acme/other/blob/main/docs/guide.md",
        ),
        (
            forge_context(ForgeDialect::Github),
            "https://github.com/acme/widgets/raw/main/docs/guide.md",
        ),
        (
            forge_context(ForgeDialect::Gitea),
            "https://codeberg.org/other/widgets/src/branch/main/docs/guide.md",
        ),
        (
            forge_context(ForgeDialect::Gitea),
            "https://codeberg.org/acme/other/src/branch/main/docs/guide.md",
        ),
        (
            forge_context(ForgeDialect::Gitlab),
            "https://gitlab.com/group/widgets/-/blob/main/docs/guide.md",
        ),
        (
            forge_context(ForgeDialect::BitbucketCloud),
            "https://bitbucket.org/other/widgets/src/feature/docs/guide.md",
        ),
        (
            forge_context(ForgeDialect::BitbucketCloud),
            "https://bitbucket.org/acme/widgets/raw/feature/docs/guide.md",
        ),
        (
            forge_context(ForgeDialect::BitbucketDataCenter),
            "https://bitbucket.example/projects/other/repos/widgets/browse/docs/guide.md",
        ),
        (
            forge_context(ForgeDialect::BitbucketDataCenter),
            "https://bitbucket.example/projects/ACME/repos/widgets/raw/docs/guide.md",
        ),
        (
            forge_context(ForgeDialect::BitbucketDataCenter),
            "https://bitbucket.example/projects/OTHER/repos/else/browse/projects/ACME/repos/widgets/browse/docs/guide.md",
        ),
    ];
    for (context, destination) in cases {
        let (intent, _row) = bed
            .run_as(
                Adapter::Markdown,
                Some(&context),
                "docs/guide.md",
                false,
                destination,
            )
            .unwrap();
        assert_eq!(intent.kind, IntentKind::ExternalUrl, "{destination}");
    }
}

/// A commit selector is a full lowercase object ID. A full ID in the other
/// format remains this repository but cannot enter the declared run.
#[test]
fn a_commit_selector_is_an_exact_oid() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::Gitea);
    for selector in [
        "6A66EF14B9B8B174A54CCF8EA4B0DD18F42F9F22",
        "6a66ef14b9b8b174a54ccf8ea4b0dd18f42f9f2",
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
    ] {
        let (intent, _row) = bed
            .run_as(
                Adapter::Markdown,
                Some(&context),
                "docs/guide.md",
                false,
                &format!("https://codeberg.org/acme/widgets/src/commit/{selector}/docs/guide.md"),
            )
            .unwrap();
        assert_eq!(
            intent.kind,
            IntentKind::ExternalUrl,
            "{selector} is not a commit spelling the forge emits"
        );
    }

    for (object_format, selector) in [
        (ObjectFormat::Sha1, "a".repeat(64)),
        (ObjectFormat::Sha256, "a".repeat(40)),
    ] {
        let context = ForgeContext {
            object_format,
            ..forge_context(ForgeDialect::Gitea)
        };
        let (intent, row) = bed
            .run_as(
                Adapter::Markdown,
                Some(&context),
                "docs/guide.md",
                false,
                &format!("https://codeberg.org/acme/widgets/src/commit/{selector}/docs/guide.md"),
            )
            .unwrap();
        assert_eq!(intent.kind, IntentKind::Unsupported, "{selector}");
        assert_eq!(
            row,
            Resolution::UnsupportedVersion {
                scope: VersionScope::UnknownPath
            },
            "{selector} belongs to the other object format"
        );
    }
}

/// A nested group owner matches segment by segment, and the separator may sit
/// past position two exactly when the owner has that many segments.
#[test]
fn nested_group_owners_match_segment_by_segment() {
    let mut bed = bed();
    let nested = ForgeContext {
        owner: "group/sub".to_owned(),
        ..forge_context(ForgeDialect::Gitlab)
    };
    let (intent, row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&nested),
            "docs/guide.md",
            false,
            "https://gitlab.com/group/sub/widgets/-/blob/feature/x/docs/guide.md",
        )
        .unwrap();
    assert_eq!(intent.kind, IntentKind::SameRepositoryGitlab);
    assert!(matches!(row, Resolution::Resolved { .. }));

    let (foreign, _row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&nested),
            "docs/guide.md",
            false,
            "https://gitlab.com/group/other/widgets/-/blob/feature/x/docs/guide.md",
        )
        .unwrap();
    assert_eq!(
        foreign.kind,
        IntentKind::ExternalUrl,
        "one wrong owner segment is foreign"
    );
}

/// A terminal slash is a directory hint exactly where the form tolerates it.
#[test]
fn a_terminal_slash_is_tolerated_only_as_a_directory_hint() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::Gitlab);
    let (tree, tree_row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acme/widgets/-/tree/feature/x/docs/",
        )
        .unwrap();
    assert_eq!(tree.kind, IntentKind::SameRepositoryGitlab);
    assert_eq!(tree.target_kind, Some(TargetKind::Tree));
    assert!(
        matches!(tree_row, Resolution::Resolved { .. }),
        "a tree with a directory-hint slash resolves: {tree_row:?}"
    );

    let (_blob, blob_row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acme/widgets/-/blob/feature/x/docs/guide.md/",
        )
        .unwrap();
    assert!(
        !matches!(blob_row, Resolution::Resolved { .. }),
        "a blob does not tolerate the hint: {blob_row:?}"
    );
}

/// The gitea directory hint needs at least one real segment before the
/// terminal slash, and a bare slash after the ref is a syntax defect.
#[test]
fn a_directory_hint_needs_a_segment_before_its_slash() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::Gitea);
    let (hinted, hinted_row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/branch/feature/x/docs/",
        )
        .unwrap();
    assert_eq!(hinted.kind, IntentKind::SameRepositoryGitea);
    assert_eq!(hinted.target_kind, Some(TargetKind::Tree));
    assert!(
        matches!(hinted_row, Resolution::Resolved { .. }),
        "{hinted_row:?}"
    );

    let (bare, bare_row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/branch/feature/x/",
        )
        .unwrap();
    assert_eq!(bare.target_kind, None, "no hint from a bare slash");
    assert!(
        !matches!(bare_row, Resolution::Resolved { .. }),
        "a bare slash after the ref is not a target: {bare_row:?}"
    );

    let (github, github_row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&forge_context(ForgeDialect::Github)),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/tree/feature/x/",
        )
        .unwrap();
    assert_eq!(
        github.kind,
        IntentKind::Unsupported,
        "a lone terminal slash after the ref is no spelling of a target"
    );
    assert!(
        !matches!(github_row, Resolution::Resolved { .. }),
        "a lone terminal slash after the ref stays a defect: {github_row:?}"
    );
}
