use amiss_scan::{Classification, classify, excluded_by_built_in};
use amiss_wire::model::Adapter;

#[test]
fn suffixes_are_exact_and_lowercase() {
    assert_eq!(
        classify(b"docs/guide.md"),
        Some(Classification::StructuredMarkdown)
    );
    assert_eq!(
        classify(b"a/b.markdown"),
        Some(Classification::StructuredMarkdown)
    );
    assert_eq!(
        classify(b"pages/home.mdx"),
        Some(Classification::StructuredMdx)
    );
    assert_eq!(
        classify(b"CLAUDE.md"),
        Some(Classification::StructuredMarkdown)
    );
    assert_eq!(
        classify(b"AGENTS.md"),
        Some(Classification::StructuredMarkdown)
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
            Some(Classification::ExtensionlessMarkdown),
            "{name}"
        );
        assert_eq!(
            classify(format!("docs/{name}").as_bytes()),
            Some(Classification::ExtensionlessMarkdown),
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
        Some(Classification::PlainAdvisory)
    );
    assert_eq!(
        classify(b"docs/llms.txt"),
        Some(Classification::PlainAdvisory)
    );
    assert_eq!(classify(b"LLMS.txt"), None);
    assert_eq!(classify(b"a/b.cursorrules"), None);
    assert_eq!(
        Classification::PlainAdvisory.adapter(),
        Some(Adapter::PlainAdvisory)
    );
}

#[test]
fn classifications_map_to_their_adapters() {
    assert_eq!(
        Classification::StructuredMarkdown.adapter(),
        Some(Adapter::Markdown)
    );
    assert_eq!(
        Classification::ExtensionlessMarkdown.adapter(),
        Some(Adapter::Markdown)
    );
    assert_eq!(Classification::StructuredMdx.adapter(), Some(Adapter::Mdx));
    assert_eq!(
        Classification::StructuredMarkdown.as_str(),
        "structured-markdown"
    );
    assert_eq!(Classification::StructuredMdx.as_str(), "structured-mdx");
    assert_eq!(
        Classification::ExtensionlessMarkdown.as_str(),
        "extensionless-markdown"
    );
    assert_eq!(Classification::PlainAdvisory.as_str(), "plain-advisory");
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
    assert!(!excluded_by_built_in(b"a/my-target/x.md"));
    assert!(!excluded_by_built_in(b"TARGET/x.md"));
    assert!(!excluded_by_built_in(b"docs/guide.md"));
    assert!(!excluded_by_built_in(b"targets/x.md"));
}
