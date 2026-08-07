use amiss_rst::{Kind, ReferenceKind, Refusal, blocks, extract, normalized_label, title_underline};

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn kinds(source: &str) -> Vec<(ReferenceKind, String)> {
    extract(source.as_bytes())
        .expect("utf-8 source")
        .references
        .into_iter()
        .map(|reference| (reference.kind, reference.target))
        .collect()
}

#[test]
fn every_specified_reference_form_is_read_with_its_exact_target() {
    let source = concat!(
        "Title\n=====\n\n",
        "See `the guide <guide.rst>`_ and `home <../README>`_.\n\n",
        ".. _named: other.rst\n\n",
        ".. image:: img/logo.png\n\n",
        ".. include:: shared.rst\n\n",
        ".. csv-table::\n   :file: data/rows.csv\n",
    );
    assert_eq!(
        kinds(source),
        vec![
            (ReferenceKind::InlineHyperlink, "guide.rst".to_owned()),
            (ReferenceKind::InlineHyperlink, "../README".to_owned()),
            (ReferenceKind::NamedTarget, "other.rst".to_owned()),
            (ReferenceKind::Image, "img/logo.png".to_owned()),
            (ReferenceKind::Include, "shared.rst".to_owned()),
            (ReferenceKind::FileOption, "data/rows.csv".to_owned()),
        ],
    );
}

#[test]
fn an_unregistered_role_is_never_a_reference_because_the_role_set_is_open() {
    for source in [
        "See :class:`Model` for more.\n",
        "See :setting:`DEBUG` for more.\n",
        "See :py:func:`compute` for more.\n",
        "An ordinary `interpreted span` with no underscore.\n",
    ] {
        assert!(kinds(source).is_empty(), "{source:?} produced a reference");
    }
}

#[test]
fn the_two_sphinx_roles_are_modelled_by_name() {
    assert_eq!(
        kinds("See :doc:`guide` and :ref:`some-label` for more.\n"),
        vec![
            (ReferenceKind::DocRole, "guide".to_owned()),
            (ReferenceKind::RefRole, "some-label".to_owned()),
        ],
    );
    assert_eq!(
        kinds("Read :doc:`the guide <../intro/guide>` first.\n"),
        vec![(ReferenceKind::DocRole, "../intro/guide".to_owned())],
    );
    assert_eq!(
        kinds("Read :ref:`the setup <setup-label>` first.\n"),
        vec![(ReferenceKind::RefRole, "setup-label".to_owned())],
    );
    assert_eq!(
        kinds("See :ref:`pytest helpers` for more.\n"),
        vec![(ReferenceKind::RefRole, "pytest helpers".to_owned())],
        "a phrase label is a legal :ref: target"
    );
    for empty in [
        ":doc:``\n",
        ":doc:`has space`\n",
        ":doc:`x <>`\n",
        ":ref:``\n",
    ] {
        assert!(kinds(empty).is_empty(), "{empty:?} produced a reference");
    }
}

#[test]
fn a_quoted_label_declaration_sheds_its_backticks() {
    let read = extract(b".. _`pytest helpers`:\n\n.. _plain:\n").expect("utf-8 source");
    assert_eq!(
        read.anchors,
        vec!["pytest helpers".to_owned(), "plain".to_owned()],
    );
}

#[test]
fn a_target_declares_from_a_list_item_or_a_table_cell() {
    let source = b"*  .. _from-bullet:\n\n| ``'n'`` | .. _from-cell:      |\n";
    let read = extract(source).expect("utf-8 source");
    assert_eq!(
        read.anchors,
        vec!["from-bullet".to_owned(), "from-cell".to_owned()],
    );
}

#[test]
fn a_prefixed_role_is_the_longer_role_it_belongs_to() {
    for source in [
        "See :external+python:std:ref:`context manager <context-managers>`.\n",
        "See :std:ref:`something <target>`.\n",
        "See :my-ext.ref:`x`.\n",
    ] {
        assert!(kinds(source).is_empty(), "{source:?} produced a reference");
    }
    assert_eq!(
        kinds("(:ref:`parenthesised`)\n"),
        vec![(ReferenceKind::RefRole, "parenthesised".to_owned())],
        "a non-name byte before the opener is not a prefix"
    );
}

#[test]
fn a_section_level_comes_from_the_order_its_underline_first_appears() {
    let source = "First\n=====\n\nSecond\n------\n\nThird\n=====\n\nFourth\n~~~~~~\n";
    let levels: Vec<(usize, String)> = extract(source.as_bytes())
        .expect("utf-8 source")
        .titles
        .into_iter()
        .map(|title| (title.level, title.text))
        .collect();
    assert_eq!(
        levels,
        vec![
            (1, "First".to_owned()),
            (2, "Second".to_owned()),
            (1, "Third".to_owned()),
            (3, "Fourth".to_owned()),
        ],
    );
}

#[test]
fn an_internal_label_publishes_an_anchor_rather_than_a_reference() {
    let extraction = extract(b".. _install-notes:\n\nInstall\n=======\n").expect("utf-8 source");
    assert_eq!(extraction.anchors, vec!["install-notes"]);
    assert!(extraction.references.is_empty());
}

#[test]
fn what_the_parser_will_not_read_into_is_declared() {
    let extraction = extract(b"Example::\n\n   `hidden <guide.rst>`_\n\n.. a plain comment\n")
        .expect("utf-8 source");
    assert_eq!(extraction.opaque.len(), 2);
    assert!(extraction.references.is_empty());
    let literal = blocks("Example::\n\n   code\n")
        .into_iter()
        .any(|block| block.kind == Kind::Literal);
    assert!(literal, "a paragraph ending in :: opens a literal block");
}

#[test]
fn bytes_that_are_not_text_are_refused_rather_than_guessed_at() {
    assert_eq!(extract(&[0xff, 0xfe]), Err(Refusal::NotUtf8));
}

#[test]
fn the_simple_name_folds_case_and_collapses_whitespace() {
    assert_eq!(normalized_label("Wide  Name"), "wide name");
    assert_eq!(normalized_label(" plain "), "plain");
    assert_eq!(normalized_label("UPPER"), "upper");
}

/// Every target is one word: empty targets and targets carrying whitespace
/// are not references at all.
#[test]
fn a_target_is_one_word_or_nothing() {
    for (reason, source) in [
        ("a directive with no argument", ".. image::\n"),
        (
            "a directive argument with a space",
            ".. image:: img/a b.png\n",
        ),
        ("an include with no argument", ".. include::\n"),
        ("a named target with no value", ".. _named:\n"),
        ("a named target with a space", ".. _named: other file.rst\n"),
        ("a file option with no value", ".. csv-table::\n   :file:\n"),
        (
            "a file option with a space",
            ".. csv-table::\n   :file: data rows.csv\n",
        ),
        (
            "an inline target with a space",
            "See `the guide <a b.rst>`_.\n",
        ),
        ("an inline target that is empty", "See `the guide <>`_.\n"),
    ] {
        assert_eq!(kinds(source), Vec::new(), "{reason}");
    }
}

/// A label declares itself only when it names something without a table rule.
#[test]
fn a_label_names_something_that_is_not_a_rule() {
    assert_eq!(
        amiss_rst::target_definition(".. _`quoted label`:"),
        Some("quoted label".to_owned())
    );
    assert_eq!(
        amiss_rst::target_definition(".. _``:"),
        None,
        "backticks around nothing name nothing"
    );
    assert_eq!(
        amiss_rst::target_definition(".. _`a|b`:"),
        None,
        "a table rule is not part of a label"
    );
}

/// An underline is one punctuation run, at least as long as its title.
#[test]
fn an_underline_is_one_run_no_shorter_than_its_title() {
    assert_eq!(title_underline("=====", "Title"), Some('='));
    assert_eq!(title_underline("======", "Title"), Some('='));
    assert_eq!(
        title_underline("====", "Title"),
        None,
        "shorter than its title"
    );
    assert_eq!(title_underline("==-==", "Title"), None, "more than one run");
    assert_eq!(title_underline("=", "T"), None, "a single character");
    assert_eq!(
        title_underline("==", "Ti"),
        Some('='),
        "two characters are a run"
    );
    assert_eq!(title_underline("aaaaa", "Title"), None, "alphanumeric");
    assert_eq!(title_underline("   ", "Title"), None, "whitespace");
    assert_eq!(
        title_underline("=====", "Title   "),
        Some('='),
        "the title's trailing space is not part of its length"
    );
}

/// Every reference form answers to its own spelling, and exactly one of them
/// is an image.
#[test]
fn the_reference_vocabulary_is_distinct_and_names_one_image() {
    let table = [
        ReferenceKind::InlineHyperlink,
        ReferenceKind::NamedTarget,
        ReferenceKind::Image,
        ReferenceKind::Include,
        ReferenceKind::FileOption,
        ReferenceKind::DocRole,
        ReferenceKind::RefRole,
    ];
    let spellings: std::collections::BTreeSet<&str> =
        table.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(spellings.len(), table.len(), "no two share a spelling");
    assert!(spellings.iter().all(|name| !name.is_empty()));

    let images: Vec<&str> = table
        .iter()
        .filter(|kind| kind.is_image())
        .map(|kind| kind.as_str())
        .collect();
    assert_eq!(images, ["rst-image-directive"]);
}

/// The extraction counts the blocks it read and how deep they nested, which
/// is what the resource meters are charged against.
#[test]
fn an_extraction_reports_its_block_count_and_deepest_nesting() {
    let flat = extract(b"One\n\nTwo\n").expect("utf-8");
    assert_eq!(flat.blocks, 2);
    assert_eq!(flat.nesting, 0);

    let nested = extract(b"Top\n\n  Middle\n\n      Deep\n").expect("utf-8");
    assert_eq!(nested.blocks, 3);
    assert_eq!(nested.nesting, 6, "the deepest indent, not the last one");
}

/// The `:doc:` suffix rule leg by leg: an extensionless relative target gains
/// the source suffix, a source-root-absolute target keeps its slash untouched,
/// and a dotted final segment already names a file.
#[test]
fn a_doc_role_destination_answers_by_the_suffix_rule() {
    let analysis = amiss_rst::analyze(
        "See :doc:`guide` then :doc:`/abs/guide` then :doc:`pages/note.txt`.\n".as_bytes(),
    )
    .expect("utf-8 source");
    let extraction = analysis.extraction.expect("rst always extracts");
    let pairs: Vec<(&str, &str)> = extraction
        .occurrences
        .iter()
        .map(|occurrence| {
            (
                occurrence.raw_destination.as_str(),
                occurrence.semantic_destination.as_str(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("guide", "guide.rst"),
            ("/abs/guide", "/abs/guide"),
            ("pages/note.txt", "pages/note.txt"),
        ],
    );
}

/// Two references in one paragraph count up within the block; a new block
/// resets the ordinal to zero.
#[test]
fn within_block_ordinals_count_up_and_reset_per_block() {
    let analysis =
        amiss_rst::analyze("See `a <a.rst>`_ and `b <b.rst>`_.\n\nThen `c <c.rst>`_.\n".as_bytes())
            .expect("utf-8 source");
    let extraction = analysis.extraction.expect("rst always extracts");
    let paths: Vec<&[usize]> = extraction
        .occurrences
        .iter()
        .map(|occurrence| occurrence.node_path.as_slice())
        .collect();
    assert_eq!(paths.len(), 3);
    assert_eq!(
        (paths[0][1], paths[1][1], paths[2][1]),
        (0, 1, 0),
        "ordinals: {paths:?}"
    );
    assert_eq!(paths[0][0], paths[1][0], "one paragraph, one block");
    assert_ne!(
        paths[1][0], paths[2][0],
        "the second paragraph is its own block"
    );
}
