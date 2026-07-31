use amiss_rst::{Kind, ReferenceKind, Refusal, blocks, extract};

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
