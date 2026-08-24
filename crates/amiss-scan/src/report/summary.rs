use amiss_wire::json::Value;
use amiss_wire::report::{Disposition, FindingKind, IntentKind};
use amiss_wire::resolution::Resolution;

use crate::correlate::Comparison;
use crate::discovery::{DocumentRecord, DocumentStatus};
use crate::evaluate::{Attribution, Finding};

use super::documents::PairedDocument;
use super::{integer, object};

pub(super) struct Counts {
    pub(super) documents: Value,
    pub(super) references: Value,
    pub(super) findings: Value,
}

#[derive(Default)]
struct DocumentCountSet {
    discovered: u64,
    scanned: u64,
    unsupported: u64,
    excluded_builtin: u64,
    frontmatter_documents: u64,
    frontmatter_bytes: u64,
    opaque_mdx_documents: u64,
    opaque_mdx_regions: u64,
    opaque_mdx_bytes: u64,
    opaque_html_documents: u64,
    opaque_html_regions: u64,
    opaque_html_bytes: u64,
}

fn region_bytes(spans: &[(usize, usize)]) -> u64 {
    spans.iter().fold(0, |total, (start, end)| {
        total.saturating_add(u64::try_from(end.saturating_sub(*start)).unwrap_or(u64::MAX))
    })
}

fn document_counts<'a>(
    candidate_records: impl IntoIterator<Item = &'a DocumentRecord>,
    unlinked: u64,
) -> Value {
    let mut counts = DocumentCountSet::default();
    for record in candidate_records {
        counts.discovered = counts.discovered.saturating_add(1);
        match &record.status {
            DocumentStatus::Scanned(scanned) => {
                counts.scanned = counts.scanned.saturating_add(1);
                let opaque = &scanned.opaque;
                counts.frontmatter_documents = counts
                    .frontmatter_documents
                    .saturating_add(u64::from(opaque.frontmatter_bytes > 0));
                counts.frontmatter_bytes = counts
                    .frontmatter_bytes
                    .saturating_add(u64::try_from(opaque.frontmatter_bytes).unwrap_or(u64::MAX));
                counts.opaque_mdx_documents = counts
                    .opaque_mdx_documents
                    .saturating_add(u64::from(!opaque.mdx.is_empty()));
                counts.opaque_mdx_regions = counts
                    .opaque_mdx_regions
                    .saturating_add(u64::try_from(opaque.mdx.len()).unwrap_or(u64::MAX));
                counts.opaque_mdx_bytes = counts
                    .opaque_mdx_bytes
                    .saturating_add(region_bytes(&opaque.mdx));
                counts.opaque_html_documents = counts
                    .opaque_html_documents
                    .saturating_add(u64::from(!opaque.html.is_empty()));
                counts.opaque_html_regions = counts
                    .opaque_html_regions
                    .saturating_add(u64::try_from(opaque.html.len()).unwrap_or(u64::MAX));
                counts.opaque_html_bytes = counts
                    .opaque_html_bytes
                    .saturating_add(region_bytes(&opaque.html));
            }
            DocumentStatus::Unsupported(_) => {
                counts.unsupported = counts.unsupported.saturating_add(1);
            }
            DocumentStatus::ExcludedBuiltIn => {
                counts.excluded_builtin = counts.excluded_builtin.saturating_add(1);
            }
            DocumentStatus::Failed(_) => {}
        }
    }
    object(vec![
        ("discovered", integer(counts.discovered)),
        ("outside_document_set", integer(0)),
        ("scanned", integer(counts.scanned)),
        ("unsupported", integer(counts.unsupported)),
        ("excluded_builtin", integer(counts.excluded_builtin)),
        ("unlinked", integer(unlinked)),
        (
            "frontmatter_documents",
            integer(counts.frontmatter_documents),
        ),
        ("opaque_mdx_documents", integer(counts.opaque_mdx_documents)),
        (
            "opaque_html_documents",
            integer(counts.opaque_html_documents),
        ),
        ("opaque_mdx_regions", integer(counts.opaque_mdx_regions)),
        ("opaque_mdx_bytes", integer(counts.opaque_mdx_bytes)),
        ("opaque_html_regions", integer(counts.opaque_html_regions)),
        ("opaque_html_bytes", integer(counts.opaque_html_bytes)),
        ("frontmatter_regions", integer(counts.frontmatter_documents)),
        ("frontmatter_bytes", integer(counts.frontmatter_bytes)),
    ])
}

#[derive(Default)]
struct ReferenceCountSet {
    extracted: u64,
    explicit_local: u64,
    same_repository: u64,
    external_out_of_scope: u64,
    unsupported: u64,
    resolved: u64,
    missing: u64,
}

fn reference_counts(comparisons: &[Comparison]) -> Value {
    let mut counts = ReferenceCountSet::default();
    for observation in comparisons.iter().flat_map(|comparison| {
        comparison
            .candidate
            .iter()
            .chain(comparison.alternatives_candidate.iter())
    }) {
        counts.extracted = counts.extracted.saturating_add(1);
        match observation.intent.kind {
            IntentKind::RepositoryPath => {
                counts.explicit_local = counts.explicit_local.saturating_add(1);
            }
            IntentKind::SameRepositoryGithub
            | IntentKind::SameRepositoryGitlab
            | IntentKind::SameRepositoryGitea => {
                counts.same_repository = counts.same_repository.saturating_add(1);
            }
            IntentKind::ExternalUrl => {
                counts.external_out_of_scope = counts.external_out_of_scope.saturating_add(1);
            }
            IntentKind::SiteRoute
                if matches!(&observation.resolution, Resolution::UnsupportedSemantics(_)) =>
            {
                counts.unsupported = counts.unsupported.saturating_add(1);
            }
            IntentKind::SiteRoute | IntentKind::Label => {}
            IntentKind::Unsupported => {
                counts.unsupported = counts.unsupported.saturating_add(1);
            }
        }
        match &observation.resolution {
            Resolution::Resolved(_)
            | Resolution::External(
                amiss_wire::resolution::ExternalReference::IntersphinxInventory,
            ) => {
                counts.resolved = counts.resolved.saturating_add(1);
            }
            Resolution::Missing(_) => {
                counts.missing = counts.missing.saturating_add(1);
            }
            Resolution::TypeMismatch(_)
            | Resolution::DeclaredUntracked(_)
            | Resolution::UnsupportedTarget(_)
            | Resolution::UnsupportedSemantics(_)
            | Resolution::UnsupportedVersion(_)
            | Resolution::Invalid(_)
            | Resolution::External(
                amiss_wire::resolution::ExternalReference::Url
                | amiss_wire::resolution::ExternalReference::ForeignRepository,
            ) => {}
        }
    }
    object(vec![
        ("extracted", integer(counts.extracted)),
        ("explicit_local", integer(counts.explicit_local)),
        ("same_repository", integer(counts.same_repository)),
        (
            "external_out_of_scope",
            integer(counts.external_out_of_scope),
        ),
        ("unsupported", integer(counts.unsupported)),
        ("resolved", integer(counts.resolved)),
        ("missing", integer(counts.missing)),
    ])
}

#[derive(Default)]
struct FindingCountSet {
    record: u64,
    warn: u64,
    fail: u64,
    introduced: u64,
    pre_existing: u64,
    resolved: u64,
    unknown: u64,
    not_applicable: u64,
    debt_tolerated: u64,
    waived: u64,
    unsupported_capabilities: u64,
    unlinked_documents: u64,
}

pub(super) fn summary_counts(
    paired: &[PairedDocument<'_>],
    comparisons: &[Comparison],
    findings: &[Finding],
    finding_rows_count: u64,
) -> Counts {
    let mut counts = FindingCountSet::default();
    for finding in findings {
        match finding.effective_disposition {
            Disposition::Record => counts.record = counts.record.saturating_add(1),
            Disposition::Warn => counts.warn = counts.warn.saturating_add(1),
            Disposition::Fail => counts.fail = counts.fail.saturating_add(1),
        }
        match finding.attribution {
            Attribution::Introduced => {
                counts.introduced = counts.introduced.saturating_add(1);
            }
            Attribution::PreExisting => {
                counts.pre_existing = counts.pre_existing.saturating_add(1);
            }
            Attribution::Resolved => {
                counts.resolved = counts.resolved.saturating_add(1);
            }
            Attribution::Unknown => {
                counts.unknown = counts.unknown.saturating_add(1);
            }
            Attribution::NotApplicable => {
                counts.not_applicable = counts.not_applicable.saturating_add(1);
            }
        }
        counts.debt_tolerated = counts
            .debt_tolerated
            .saturating_add(u64::from(finding.debt.is_some()));
        counts.waived = counts
            .waived
            .saturating_add(u64::from(finding.waiver.is_some()));
        counts.unsupported_capabilities = counts.unsupported_capabilities.saturating_add(
            u64::from(finding.kind() == FindingKind::UnsupportedCapability),
        );
        counts.unlinked_documents = counts
            .unlinked_documents
            .saturating_add(u64::from(finding.kind() == FindingKind::UnlinkedDocument));
    }
    let documents = document_counts(
        paired.iter().filter_map(|pair| pair.candidate),
        counts.unlinked_documents,
    );
    let findings_value = object(vec![
        ("total", integer(finding_rows_count)),
        ("record", integer(counts.record)),
        ("warn", integer(counts.warn)),
        ("fail", integer(counts.fail)),
        ("introduced", integer(counts.introduced)),
        ("pre_existing", integer(counts.pre_existing)),
        ("resolved", integer(counts.resolved)),
        ("unknown", integer(counts.unknown)),
        ("not_applicable", integer(counts.not_applicable)),
        ("debt_tolerated", integer(counts.debt_tolerated)),
        ("waived", integer(counts.waived)),
        ("analysis_errors", integer(0)),
        (
            "unsupported_capabilities",
            integer(counts.unsupported_capabilities),
        ),
    ]);
    Counts {
        documents,
        references: reference_counts(comparisons),
        findings: findings_value,
    }
}

pub(super) fn zero_counts(analysis_errors: u64) -> Counts {
    Counts {
        documents: document_counts(std::iter::empty::<&DocumentRecord>(), 0),
        references: reference_counts(&[]),
        findings: object(vec![
            ("total", integer(0)),
            ("record", integer(0)),
            ("warn", integer(0)),
            ("fail", integer(0)),
            ("introduced", integer(0)),
            ("pre_existing", integer(0)),
            ("resolved", integer(0)),
            ("unknown", integer(0)),
            ("not_applicable", integer(0)),
            ("debt_tolerated", integer(0)),
            ("waived", integer(0)),
            ("analysis_errors", integer(analysis_errors)),
            ("unsupported_capabilities", integer(0)),
        ]),
    }
}
