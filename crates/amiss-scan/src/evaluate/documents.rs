use amiss_wire::controls::Profile;
use amiss_wire::report::FindingKind;
use amiss_wire::report::model::DocumentFindingKeyScopeKind;

use super::finding::simple;
use super::{
    Attribution, DocumentInput, DocumentSide, Finding, FindingKeyScope, Location, LocationSide,
};

pub(super) fn document_findings(
    document: &DocumentInput,
    profile: Profile,
    navigation: Option<&crate::semantic::SiteNavigation>,
    findings: &mut Vec<Finding>,
) {
    let path = &document.path;
    if document.base.is_some() && document.candidate.is_none() {
        findings.push(simple(
            FindingKind::DocumentRemoved,
            FindingKeyScope::Document {
                document: path.clone(),
                kind: DocumentFindingKeyScopeKind::Document,
            },
            Attribution::NotApplicable,
            Vec::new(),
            Location {
                side: LocationSide::Base,
                path: Some(path.clone()),
                span: None,
                display: None,
            },
            profile,
        ));
        return;
    }
    let candidate_location = || Location {
        side: LocationSide::Candidate,
        path: Some(path.clone()),
        span: None,
        display: None,
    };
    match document.candidate {
        None | Some(DocumentSide::ExcludedBuiltIn) => {}
        Some(DocumentSide::Unsupported) => {
            findings.push(simple(
                FindingKind::UnsupportedDocumentFormat,
                FindingKeyScope::Document {
                    document: path.clone(),
                    kind: DocumentFindingKeyScopeKind::Document,
                },
                Attribution::NotApplicable,
                Vec::new(),
                candidate_location(),
                profile,
            ));
        }
        Some(DocumentSide::Scanned {
            mdx_regions,
            html_regions,
            ..
        }) => {
            if mdx_regions > 0 {
                findings.push(simple(
                    FindingKind::OpaqueMdxRegion,
                    FindingKeyScope::Document {
                        document: path.clone(),
                        kind: DocumentFindingKeyScopeKind::Document,
                    },
                    Attribution::NotApplicable,
                    Vec::new(),
                    candidate_location(),
                    profile,
                ));
            }
            if html_regions > 0 {
                findings.push(simple(
                    FindingKind::OpaqueHtmlRegion,
                    FindingKeyScope::Document {
                        document: path.clone(),
                        kind: DocumentFindingKeyScopeKind::Document,
                    },
                    Attribution::NotApplicable,
                    Vec::new(),
                    candidate_location(),
                    profile,
                ));
            }
            if navigation.is_some_and(|navigation| {
                crate::semantic::navigation_contains(navigation.root.as_ref(), path)
                    && path != &navigation.manifest
                    && navigation.reachable.binary_search(path).is_err()
            }) {
                findings.push(simple(
                    FindingKind::UnlinkedDocument,
                    FindingKeyScope::Document {
                        document: path.clone(),
                        kind: DocumentFindingKeyScopeKind::Document,
                    },
                    Attribution::NotApplicable,
                    Vec::new(),
                    candidate_location(),
                    profile,
                ));
            }
        }
    }
}
