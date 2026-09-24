use amiss_scan::document::native_adapter;
use amiss_scan::{DocumentClassification, classify, excluded_by_built_in};
use amiss_wire::model::Adapter;

#[test]
fn suffixes_are_exact_and_lowercase() {
    assert_eq!(
        classify(b"docs/guide.md"),
        Some(DocumentClassification::StructuredMarkdown)
    );
    assert_eq!(
        classify(b"a/b.markdown"),
        Some(DocumentClassification::StructuredMarkdown)
    );
    assert_eq!(
        classify(b"pages/home.mdx"),
        Some(DocumentClassification::StructuredMdx)
    );
    assert_eq!(
        classify(b"CLAUDE.md"),
        Some(DocumentClassification::StructuredMarkdown)
    );
    assert_eq!(
        classify(b"AGENTS.md"),
        Some(DocumentClassification::StructuredMarkdown)
    );
    assert_eq!(classify(b"a.MD"), None);
    assert_eq!(classify(b"a.Markdown"), None);
    assert_eq!(classify(b"b.MDX"), None);
    assert_eq!(classify(b"notes.txt"), None);
    assert_eq!(classify(b"page.html"), None);
}

#[test]
fn extensionless_basenames_are_markdown() {
    for name in [
        "README",
        "CONTRIBUTING",
        "CHANGELOG",
        "SECURITY",
        "SUPPORT",
        "CODE_OF_CONDUCT",
    ] {
        assert_eq!(
            classify(name.as_bytes()),
            Some(DocumentClassification::ExtensionlessMarkdown),
            "{name}"
        );
        assert_eq!(
            classify(format!("docs/{name}").as_bytes()),
            Some(DocumentClassification::ExtensionlessMarkdown),
            "nested {name}"
        );
    }
    assert_eq!(classify(b"readme"), None);
    assert_eq!(classify(b"README.txt"), None);
    assert_eq!(classify(b"SUPPORTED"), None);
}

#[test]
fn advisory_basenames_run_no_grammar() {
    assert_eq!(
        classify(b".cursorrules"),
        Some(DocumentClassification::PlainAdvisory)
    );
    assert_eq!(
        classify(b"docs/llms.txt"),
        Some(DocumentClassification::PlainAdvisory)
    );
    assert_eq!(classify(b"LLMS.txt"), None);
    assert_eq!(classify(b"a/b.cursorrules"), None);
    assert_eq!(
        native_adapter(DocumentClassification::PlainAdvisory),
        Some(Adapter::PlainAdvisory)
    );
}

#[test]
fn classifications_map_to_their_adapters() {
    assert_eq!(
        native_adapter(DocumentClassification::StructuredMarkdown),
        Some(Adapter::Markdown)
    );
    assert_eq!(
        native_adapter(DocumentClassification::ExtensionlessMarkdown),
        Some(Adapter::Markdown)
    );
    assert_eq!(
        native_adapter(DocumentClassification::StructuredMdx),
        Some(Adapter::Mdx)
    );
    assert_eq!(
        DocumentClassification::StructuredMarkdown.as_ref(),
        "structured-markdown"
    );
    assert_eq!(
        DocumentClassification::StructuredMdx.as_ref(),
        "structured-mdx"
    );
    assert_eq!(
        DocumentClassification::ExtensionlessMarkdown.as_ref(),
        "extensionless-markdown"
    );
    assert_eq!(
        DocumentClassification::PlainAdvisory.as_ref(),
        "plain-advisory"
    );
}

#[test]
fn excluded_trees_are_directory_components() {
    for tree in [
        "node_modules",
        "vendor",
        "third_party",
        "dist",
        "build",
        ".next",
        "target",
        "test",
        "tests",
    ] {
        assert!(
            excluded_by_built_in(format!("{tree}/x.md").as_bytes()),
            "{tree}"
        );
        assert!(
            excluded_by_built_in(format!("a/{tree}/b/x.md").as_bytes()),
            "nested {tree}"
        );
    }
    assert!(!excluded_by_built_in(b"vendor.md"));
    assert!(
        !excluded_by_built_in(b"targets"),
        "a slashless name is never a directory component, even one byte over a tree name"
    );
    assert!(!excluded_by_built_in(b"a/my-target/x.md"));
    assert!(!excluded_by_built_in(b"TARGET/x.md"));
    assert!(!excluded_by_built_in(b"docs/guide.md"));
    assert!(!excluded_by_built_in(b"targets/x.md"));
    assert!(!excluded_by_built_in(b"testing/x.md"));
    assert!(!excluded_by_built_in(b"latest/x.md"));
}

#[test]
fn the_markup_suffixes_reach_their_own_adapters() {
    assert_eq!(
        classify(b"docs/guide.rst"),
        Some(DocumentClassification::StructuredRst),
    );
    assert_eq!(
        native_adapter(DocumentClassification::StructuredRst),
        Some(Adapter::Rst)
    );
    for path in [&b"docs/guide.adoc"[..], b"docs/guide.asciidoc"] {
        assert_eq!(
            classify(path),
            Some(DocumentClassification::StructuredAsciiDoc),
            "{}",
            String::from_utf8_lossy(path),
        );
    }
    assert_eq!(
        native_adapter(DocumentClassification::StructuredAsciiDoc),
        Some(Adapter::AsciiDoc),
    );
    for path in [&b"docs/guide.txt"[..], b"docs/guide.rst.bak", b"docs/RST"] {
        assert_ne!(
            classify(path),
            Some(DocumentClassification::UnparsedMarkup),
            "{}",
            String::from_utf8_lossy(path),
        );
    }
}

#[test]
fn a_notebook_or_org_file_is_a_document_that_is_never_read() {
    for path in [&b"docs/tour.ipynb"[..], b"notes/plan.org"] {
        assert_eq!(
            classify(path),
            Some(DocumentClassification::UnparsedMarkup),
            "{}",
            String::from_utf8_lossy(path),
        );
    }
    assert_eq!(native_adapter(DocumentClassification::UnparsedMarkup), None);
    for path in [
        &b"docs/tour.IPYNB"[..],
        b"docs/tour.ipynb.bak",
        b"planorg",
        b"docs/notes.txt",
    ] {
        assert_ne!(
            classify(path),
            Some(DocumentClassification::UnparsedMarkup),
            "{}",
            String::from_utf8_lossy(path),
        );
    }
}
