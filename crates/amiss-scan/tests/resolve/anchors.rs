use amiss_fixtures::{CommitChain, Staged, staged_repository};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::resolve::TargetCache;
use amiss_scan::{Resolution, ScanLimits, ScanResources, discover, resolve};
use amiss_wire::model::{Adapter, ObjectFormat, Oid, RepoPath};
use amiss_wire::resolution::{Missing, Target, UnsupportedSemantics};

use crate::support::{ANCHORS, bed, bed_with};

/// The identity of a heading belongs to the renderer, so an anchor resolves
/// when any pinned renderer would publish it. Nothing a repository declares can
/// narrow that set.
#[test]
fn a_heading_anchor_resolves_under_the_union_of_the_renderer_rules() {
    let mut bed = bed();
    for fragment in [
        "setup--config",
        "setup-config",
        "setup--config-1",
        "setup-config_1",
        "r%C3%A9sum%C3%A9-draft",
        "resume-draft",
        "declared",
        "custom",
        "explicit-custom",
        "tool",
        "anchor-point",
        "pair-id",
    ] {
        let destination = format!("anchors.md#{fragment}");
        let row = bed
            .run_as(
                Adapter::Markdown,
                None,
                "docs/guide.md",
                false,
                &destination,
            )
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Resolved(Target::Blob(blob)) = &row else {
            panic!("{fragment} is published by a known renderer: {row:?}");
        };
        assert_eq!(blob.path.as_str(), Some("docs/anchors.md"));
    }

    for fragment in ["Setup--Config", "setup", "résumé", "customid"] {
        let destination = format!("anchors.md#{fragment}");
        let row = bed
            .run_as(
                Adapter::Markdown,
                None,
                "docs/guide.md",
                false,
                &destination,
            )
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Missing(Missing::HeadingAnchorNotFound { path, .. }) = &row else {
            panic!("{fragment} is published by no renderer: {row:?}");
        };
        assert_eq!(path.as_str(), Some("docs/anchors.md"));
    }
}

/// A target the evaluation cannot read, parse, or afford keeps the unsupported
/// answer. Reporting it missing would be reporting on a parse that never ran.
#[test]
fn an_unevaluable_anchor_target_stays_unsupported_semantics() {
    let mut bed = bed();
    for destination in ["pointer.md#any", "invalid.md#any", "../llms.txt#any"] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(
                &row,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
            ),
            "{destination}: {row:?}"
        );
    }

    let mut starved = bed_with(ScanLimits {
        aggregate_heading_anchor_evaluation_bytes_per_snapshot: 0,
        ..ScanLimits::CONTRACT
    });
    let row = starved
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "anchors.md#setup--config",
        )
        .unwrap_or_else(|_defect| panic!("resolve under an exhausted anchor budget"))
        .1;
    assert!(
        matches!(
            &row,
            Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
        ),
        "an exhausted budget judges nothing: {row:?}"
    );
}

/// The identities are built once per target, so repeated anchors into one
/// document are charged once.
#[test]
fn distinct_anchors_into_one_target_are_charged_once() {
    let mut bed = bed();
    for fragment in ["setup--config", "resume-draft", "declared"] {
        let destination = format!("anchors.md#{fragment}");
        bed.run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            &destination,
        )
        .unwrap_or_else(|_defect| panic!("resolve {destination}"));
    }
    assert_eq!(
        bed.scan_resources.heading_anchor_bytes(),
        u64::try_from(ANCHORS.len()).unwrap_or(u64::MAX),
        "one charge for the one target"
    );
}

/// A reStructuredText document that includes another file publishes
/// identities this engine never read, so an anchor it does not hold is
/// undecided rather than absent, the boundary the `AsciiDoc` include already
/// declared.
#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn transclusion_fixture() -> CommitChain {
    staged_repository(&[
        (
            "README.md",
            Staged::File(b"[a](docs/host.rst#present)\n[b](docs/host.rst#spliced)\n"),
        ),
        (
            "docs/host.rst",
            Staged::File(b"Present\n=======\n\n.. include:: part.rst\n"),
        ),
        ("docs/part.rst", Staged::File(b"Spliced\n=======\n")),
    ])
    .unwrap()
}

#[test]
fn an_rst_include_leaves_absent_anchors_undecided() {
    let dir = transclusion_fixture();
    let tree = Oid::new(
        ObjectFormat::Sha1,
        dir.commits.first().unwrap().tree.clone(),
    )
    .unwrap();
    let repo = Repository::open(dir.root(), ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let includes = amiss_scan::Includes::default();
    let discovery = discover(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &includes,
        &tree,
    )
    .unwrap();
    let mut cache = TargetCache::default();

    let (_intent, held) = resolve(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &mut cache,
        &discovery,
        None,
        Adapter::Markdown,
        &RepoPath::new("README.md".to_owned()).unwrap(),
        false,
        "docs/host.rst#present",
    )
    .unwrap();
    assert!(matches!(held, Resolution::Resolved(_)), "{held:?}");

    let (_intent, spliced) = resolve(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &mut cache,
        &discovery,
        None,
        Adapter::Markdown,
        &RepoPath::new("README.md".to_owned()).unwrap(),
        false,
        "docs/host.rst#spliced",
    )
    .unwrap();
    assert!(
        matches!(spliced, Resolution::UnsupportedSemantics(_)),
        "an anchor behind an include is undecided, got {spliced:?}"
    );
}

/// The neighbor steps forward only alone: one typography match names itself,
/// two stay bare as a real ambiguity, and an unrelated miss stays bare.
#[test]
fn a_typography_neighbor_steps_forward_alone() {
    let mut bed = bed();
    let near_of = |bed: &mut crate::support::Bed, destination: &str| {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Missing(Missing::HeadingAnchorNotFound { near, .. }) = row else {
            panic!("{destination} is not a missing anchor: {row:?}");
        };
        near
    };
    assert_eq!(
        near_of(&mut bed, "anchors.md#Setup--Config"),
        Some("setup--config".to_owned()),
        "one case-fold neighbor names itself"
    );
    assert_eq!(
        near_of(&mut bed, "anchors.md#DECLARED"),
        Some("declared".to_owned())
    );
    assert_eq!(
        near_of(&mut bed, "anchors.md#customid"),
        None,
        "an unrelated miss stays bare"
    );
    assert_eq!(
        near_of(&mut bed, "case.md#DUP"),
        None,
        "two case variants are a real ambiguity"
    );
    assert_eq!(
        near_of(&mut bed, "anchors.md#anchor_point"),
        Some("anchor-point".to_owned()),
        "the separator renderers disagree on folds away"
    );
    assert_eq!(
        near_of(&mut bed, "anchors.md#Pair_ID"),
        Some("pair-id".to_owned()),
        "case and separator fold together"
    );
}
