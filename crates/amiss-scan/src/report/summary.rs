use amiss_wire::report::{
    Disposition, DocumentCounts, FindingCounts, FindingKind, IntentKind, ReferenceCounts, Summary,
};
use amiss_wire::resolution::Resolution;

use crate::correlate::Comparison;
use crate::discovery::{DocumentRecord, DocumentStatus};
use crate::evaluate::{Attribution, Finding};

use super::documents::PairedDocument;

fn region_bytes(spans: &[(usize, usize)]) -> u64 {
    spans.iter().fold(0, |total, (start, end)| {
        total.saturating_add(u64::try_from(end.saturating_sub(*start)).unwrap_or(u64::MAX))
    })
}

fn document_counts<'a>(
    candidate_records: impl IntoIterator<Item = &'a DocumentRecord>,
    unlinked: u64,
) -> DocumentCounts {
    let mut counts = DocumentCounts::default();
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
    counts.unlinked = unlinked;
    counts.frontmatter_regions = counts.frontmatter_documents;
    counts
}

fn reference_counts(comparisons: &[Comparison]) -> ReferenceCounts {
    let mut counts = ReferenceCounts::default();
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
            | IntentKind::SameRepositoryGitea
            | IntentKind::SameRepositoryBitbucketCloud
            | IntentKind::SameRepositoryBitbucketDataCenter => {
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
                amiss_wire::resolution::ExternalReference::IntersphinxInventory
                | amiss_wire::resolution::ExternalReference::SiteBuild,
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
    counts
}

pub(super) fn summary_counts(
    paired: &[PairedDocument<'_>],
    comparisons: &[Comparison],
    findings: &[Finding],
    finding_rows_count: u64,
) -> Summary {
    let mut counts = FindingCounts::default();
    let mut unlinked_documents = 0_u64;
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
        unlinked_documents = unlinked_documents
            .saturating_add(u64::from(finding.kind() == FindingKind::UnlinkedDocument));
    }
    let documents = document_counts(
        paired.iter().filter_map(|pair| pair.candidate),
        unlinked_documents,
    );
    counts.total = finding_rows_count;
    Summary {
        counts_complete: true,
        documents,
        references: reference_counts(comparisons),
        findings: counts,
        ..Summary::default()
    }
}

pub(super) fn zero_counts(analysis_errors: u64) -> Summary {
    Summary {
        findings: FindingCounts {
            analysis_errors,
            ..FindingCounts::default()
        },
        ..Summary::default()
    }
}
