use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentCounts {
    pub discovered: u64,
    pub excluded_builtin: u64,
    pub frontmatter_bytes: u64,
    pub frontmatter_documents: u64,
    pub frontmatter_regions: u64,
    pub opaque_html_bytes: u64,
    pub opaque_html_documents: u64,
    pub opaque_html_regions: u64,
    pub opaque_mdx_bytes: u64,
    pub opaque_mdx_documents: u64,
    pub opaque_mdx_regions: u64,
    pub outside_document_set: u64,
    pub scanned: u64,
    pub unlinked: u64,
    pub unsupported: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceCounts {
    pub explicit_local: u64,
    pub external_out_of_scope: u64,
    pub extracted: u64,
    pub missing: u64,
    pub resolved: u64,
    pub same_repository: u64,
    pub unsupported: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingCounts {
    pub analysis_errors: u64,
    pub debt_tolerated: u64,
    pub fail: u64,
    pub introduced: u64,
    pub not_applicable: u64,
    pub pre_existing: u64,
    pub record: u64,
    pub resolved: u64,
    pub total: u64,
    pub unknown: u64,
    pub unsupported_capabilities: u64,
    pub waived: u64,
    pub warn: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub counts_complete: bool,
    pub documents: DocumentCounts,
    pub findings: FindingCounts,
    pub governed_claims: u64,
    pub references: ReferenceCounts,
    pub unattested_claims: u64,
}
