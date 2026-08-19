use amiss_wire::controls::Profile;
use amiss_wire::report::FindingKind;

use super::finding::{document_scope, simple};
use super::{Attribution, DocumentInput, DocumentSide, Finding, Location, LocationSide};

pub(super) fn document_findings(
    document: &DocumentInput,
    profile: Profile,
    findings: &mut Vec<Finding>,
) {
    let path = &document.path;
    if document.base.is_some() && document.candidate.is_none() {
        findings.push(simple(
            FindingKind::DocumentRemoved,
            document_scope(path),
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
                document_scope(path),
                Attribution::NotApplicable,
                Vec::new(),
                candidate_location(),
                profile,
            ));
        }
        Some(DocumentSide::Scanned {
            mdx_regions,
            html_regions,
            extracted_references,
        }) => {
            if mdx_regions > 0 {
                findings.push(simple(
                    FindingKind::OpaqueMdxRegion,
                    document_scope(path),
                    Attribution::NotApplicable,
                    Vec::new(),
                    candidate_location(),
                    profile,
                ));
            }
            if html_regions > 0 {
                findings.push(simple(
                    FindingKind::OpaqueHtmlRegion,
                    document_scope(path),
                    Attribution::NotApplicable,
                    Vec::new(),
                    candidate_location(),
                    profile,
                ));
            }
            if extracted_references == 0 {
                findings.push(simple(
                    FindingKind::UnlinkedDocument,
                    document_scope(path),
                    Attribution::NotApplicable,
                    Vec::new(),
                    candidate_location(),
                    profile,
                ));
            }
        }
    }
}
