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
    let observations = body.get("observations").unwrap().as_array().unwrap();
    let candidate_side = |kind: &str| {
        observations
            .iter()
            .filter_map(|row| row.get("candidate"))
            .filter(|side| {
                side.pointer("/intent/kind")
                    .is_some_and(|value| value == kind)
            })
            .collect::<Vec<_>>()
    };

    let doc = candidate_side("repository-path");
    assert!(
        doc.iter().any(|side| {
            side.pointer("/intent/repository_path")
                .is_some_and(|path| path == "docs/guide.rst")
                && side
                    .pointer("/resolution/kind")
                    .is_some_and(|kind| kind == "resolved")
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
    let body = payload(&stdout);
    let observations = body.get("observations").unwrap().as_array().unwrap();
    let candidate_side = |kind: &str| {
        observations
            .iter()
            .filter_map(|row| row.get("candidate"))
            .filter(|side| {
                side.pointer("/intent/kind")
                    .is_some_and(|value| value == kind)
            })
            .collect::<Vec<_>>()
    };
    let labels = candidate_side("label");
    assert_eq!(labels.len(), 5, "five :ref: observations: {labels:?}");
    let outcome = |side: &&serde_json::Value| {
        (
            side.pointer("/resolution/kind")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned(),
            side.pointer("/resolution/reason")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned(),
        )
    };
    let mut outcomes: Vec<(String, String)> = labels.iter().map(outcome).collect();
    outcomes.sort();
    assert_eq!(
        outcomes,
        vec![
            ("missing".to_owned(), "label-not-declared".to_owned()),
            ("resolved".to_owned(), String::new()),
            ("resolved".to_owned(), String::new()),
            (
                "unsupported-semantics".to_owned(),
                "duplicate-label".to_owned()
            ),
            (
                "unsupported-semantics".to_owned(),
                "external-inventory".to_owned()
            ),
        ],
        "two held including the quoted phrase, one dead, one duplicated, one another project's"
    );
    let held = labels
        .iter()
        .find(|side| {
            side.pointer("/resolution/kind")
                .is_some_and(|kind| kind == "resolved")
        })
        .unwrap();
    assert_eq!(
        held.pointer("/resolution/target/path").unwrap(),
        "docs/guide.rst",
        "the label resolves to its declaring document"
    );
    for side in &labels {
        assert!(
            side.pointer("/intent/fragment_digest")
                .is_some_and(|digest| !digest.is_null()),
            "the label rides the intent as its fragment: {side:?}"
        );
    }
}
