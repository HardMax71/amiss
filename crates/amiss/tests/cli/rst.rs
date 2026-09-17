use amiss_wire::repo_path_text;
use amiss_wire::report::IntentKind;
use amiss_wire::report::model::{
    MissingResolution, RepoPath, Resolution, UnsupportedSemanticsReason,
    UnsupportedSemanticsResolution, occurrences,
};
use amiss_wire::resolution::{BlobTarget, Target};

use crate::support::{amiss, payload};

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn sphinx_fixture() -> (i32, Vec<u8>, String) {
    let fx = amiss_fixtures::commit_pair(
        &[
            ("docs/index.rst", "Index\n=====\n\nSee :doc:`guide`.\n"),
            (
                "docs/guide.rst",
                ".. _setup:\n\n.. _`Wide  Name`:\n\nGuide\n=====\n\nSee :ref:`setup` and :ref:`gone` and :ref:`twice`.\nAlso :ref:`wide name` and :ref:`python:comparisons`.\n",
            ),
            ("docs/a.rst", ".. _twice:\n\nA\n=\n"),
            ("docs/b.rst", ".. _twice:\n\nB\n=\n"),
        ],
        &[("docs/index.rst", "Index\n=====\n\nStill :doc:`guide`.\n")],
    )
    .unwrap();
    amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "enforce",
        "--format",
        "json",
    ])
}

/// A relative `:doc:` resolves through the ordinary path lane with the
/// source suffix appended, and the dead `:ref:` beside it blocks the run.
#[test]
fn a_sphinx_doc_role_resolves_through_the_path_lane() {
    let (code, stdout, stderr) = sphinx_fixture();
    assert_eq!((code, stderr.as_str()), (1, ""), "the dead label blocks");
    let body = payload(&stdout);
    let report = crate::support::report(&stdout);
    let doc: Vec<_> = report
        .payload
        .observations
        .iter()
        .filter_map(|row| occurrences(row).candidate)
        .filter(|side| {
            side.observation_id_input.extracted_intent.kind == IntentKind::RepositoryPath
        })
        .collect();
    assert!(
        doc.iter().any(|side| {
            side.observation_id_input.extracted_intent.repository_path
                == Some(RepoPath::Text(repo_path_text!("docs/guide.rst")))
                && matches!(side.resolution, Resolution::Resolved { .. })
        }),
        "the :doc: role resolves through the ordinary path lane: {doc:?}"
    );

    let findings = body.get("findings").unwrap().as_array().unwrap();
    assert!(
        findings.iter().any(|row| {
            row.get("kind").unwrap() == "explicit-target-missing"
                && row.get("effective_disposition").unwrap() == "fail"
        }),
        "a label nobody declares blocks under enforce"
    );
}

/// A `:ref:` resolves through the snapshot's label table: one declaration
/// resolves to its declaring document, none is a missing target, and two are
/// undecided rather than guessed between.
#[test]
fn sphinx_labels_resolve_through_the_label_table() {
    let (_code, stdout, _stderr) = sphinx_fixture();
    let report = crate::support::report(&stdout);
    let labels: Vec<_> = report
        .payload
        .observations
        .iter()
        .filter_map(|row| occurrences(row).candidate)
        .filter(|side| side.observation_id_input.extracted_intent.kind == IntentKind::Label)
        .collect();
    assert_eq!(labels.len(), 5, "five :ref: observations: {labels:?}");
    let count = |expected: fn(&Resolution) -> bool| {
        labels
            .iter()
            .filter(|side| expected(&side.resolution))
            .count()
    };
    assert_eq!(
        count(|resolution| matches!(resolution, Resolution::Resolved { .. })),
        2,
        "two held including the quoted phrase: {labels:?}"
    );
    assert_eq!(
        count(|resolution| matches!(
            resolution,
            Resolution::Missing(MissingResolution::LabelNotDeclared {})
        )),
        1,
        "one dead: {labels:?}"
    );
    assert_eq!(
        count(|resolution| matches!(
            resolution,
            Resolution::UnsupportedSemantics(UnsupportedSemanticsResolution {
                reason: UnsupportedSemanticsReason::DuplicateLabel,
                ..
            })
        )),
        1,
        "one duplicated: {labels:?}"
    );
    assert_eq!(
        count(|resolution| matches!(
            resolution,
            Resolution::UnsupportedSemantics(UnsupportedSemanticsResolution {
                reason: UnsupportedSemanticsReason::ExternalInventory,
                ..
            })
        )),
        1,
        "one another project's: {labels:?}"
    );
    let held = labels
        .iter()
        .find_map(|side| match &side.resolution {
            Resolution::Resolved { target } => Some(target),
            Resolution::DeclaredUntracked { .. }
            | Resolution::External { .. }
            | Resolution::Invalid { .. }
            | Resolution::Missing(_)
            | Resolution::TypeMismatch { .. }
            | Resolution::UnsupportedSemantics(_)
            | Resolution::UnsupportedTarget { .. }
            | Resolution::UnsupportedVersion { .. } => None,
        })
        .unwrap();
    let (Target::Tree { path } | Target::Blob(BlobTarget { path, .. })) = held;
    assert_eq!(
        *path,
        RepoPath::Text(repo_path_text!("docs/guide.rst")),
        "the label resolves to its declaring document"
    );
    for side in &labels {
        assert!(
            side.observation_id_input
                .extracted_intent
                .fragment_digest
                .is_some(),
            "the label rides the intent as its fragment: {side:?}"
        );
    }
}
