use amiss_wire::controls::GitMode;
use amiss_wire::controls::ProjectionSource;
use amiss_wire::report::model::FindingFactEvidence;
use amiss_wire::report::model::ProjectionDifference;
use amiss_wire::report::model::RowsProjectionDifference;
use amiss_wire::resolution::Resolution;
mod parse;
mod record;
mod site;

use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_wire::de::{Error, ErrorKind};
use amiss_wire::model::Digest;
use amiss_wire::model::{ArtifactId, RepoPath};
pub use amiss_wire::report::model::SemanticEvidenceProvenance;
use amiss_wire::report::{ErrorDetail, model::AnalysisErrorCode};

pub(crate) use parse::{parse, validated_envelope};
pub(crate) use site::{fragment_target, navigation_contains};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    pub(crate) candidate_bindings: Vec<Digest>,
    pub(crate) labels: Arc<BTreeMap<String, InventoryLabel>>,
    pub(crate) record_sets: Arc<BTreeMap<ArtifactId, RecordSet>>,
    pub(crate) routes: Arc<BTreeMap<String, SiteRoute>>,
    pub(crate) site: SiteEvaluation,
    pub(crate) provenance: Vec<SemanticEvidenceProvenance>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Input {
    #[default]
    None,
    Bound(Inputs),
    Template(amiss_wire::semantic::SemanticEvidenceTemplate<'static>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecordSet {
    pub(crate) complete: bool,
    pub(crate) records: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InventoryLabel {
    Unique(String),
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SiteRoute {
    Unique(SiteClaim),
    Ambiguous {
        sources: Vec<RepoPath>,
        claims: Vec<Digest>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteClaim {
    pub(crate) source: Option<RepoPath>,
    pub(crate) digest: Digest,
    pub(crate) target: SiteTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SiteTarget {
    Page {
        backing: SitePageBacking,
        anchors: Vec<String>,
    },
    Redirect {
        destination: String,
        fragment: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SitePageBacking {
    Repository,
    Generated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteNavigation {
    pub(crate) root: Option<RepoPath>,
    pub(crate) manifest: RepoPath,
    pub(crate) entrypoints: Vec<String>,
    pub(crate) reachable: Vec<RepoPath>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SiteEvaluation {
    pub(crate) navigation: Option<Arc<SiteNavigation>>,
    pub(crate) defects: Arc<[SiteDefect]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteDefect {
    pub(crate) id: Digest,
    pub(crate) evidence: FindingFactEvidence<
        RepoPath,
        Resolution<RepoPath>,
        ProjectionSource,
        ProjectionDifference<Box<RowsProjectionDifference>>,
        GitMode,
    >,
    pub(crate) source: Option<RepoPath>,
    pub(crate) member_count: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Context {
    pub(crate) labels: Arc<BTreeMap<String, InventoryLabel>>,
    pub(crate) record_sets: Arc<BTreeMap<ArtifactId, RecordSet>>,
    pub(crate) routes: Arc<BTreeMap<String, SiteRoute>>,
    pub(crate) site: SiteEvaluation,
    pub(crate) provenance: Vec<SemanticEvidenceProvenance>,
}

#[derive(Clone, Copy)]
pub(crate) struct View<'a> {
    pub(crate) labels: &'a BTreeMap<String, InventoryLabel>,
    pub(crate) routes: Option<&'a BTreeMap<String, SiteRoute>>,
}

pub(crate) fn bind(input: &Input, candidate: Digest) -> Result<Context, ErrorDetail> {
    let parsed;
    let inputs = match input {
        Input::None => return Ok(Context::default()),
        Input::Bound(inputs) => inputs,
        Input::Template(template) => {
            parsed = amiss_wire::semantic::bind_template(template, candidate)
                .and_then(|envelope| parse([Ok(envelope)]))
                .map_err(|error| configuration_detail(&error))?;
            &parsed
        }
    };
    if inputs
        .candidate_bindings
        .iter()
        .any(|binding| *binding != candidate)
    {
        return Err(ErrorDetail {
            code: AnalysisErrorCode::ControlBindingMismatch,
            path: None,
            path_bytes: None,
            resource: None,
        });
    }
    Ok(Context {
        labels: inputs.labels.clone(),
        record_sets: inputs.record_sets.clone(),
        routes: inputs.routes.clone(),
        site: inputs.site.clone(),
        provenance: inputs.provenance.clone(),
    })
}

/// Maps one strict external-input defect into the scanner's public analysis taxonomy.
#[must_use]
pub fn configuration_detail(error: &Error) -> ErrorDetail {
    let analysis = match &error.kind {
        ErrorKind::Json(message)
            if message.starts_with("invalid utf-8") || message.starts_with("incomplete utf-8") =>
        {
            AnalysisErrorCode::InvalidUtf8
        }
        ErrorKind::Json(message) if message.starts_with("duplicate JSON key") => {
            AnalysisErrorCode::DuplicateJsonKey
        }
        ErrorKind::Json(_) => AnalysisErrorCode::InvalidJson,
        ErrorKind::UnknownField => AnalysisErrorCode::UnknownField,
        ErrorKind::DigestMismatch => AnalysisErrorCode::DigestMismatch,
        ErrorKind::UnsortedSet | ErrorKind::DuplicateMember => AnalysisErrorCode::NoncanonicalArray,
        ErrorKind::MissingField
        | ErrorKind::WrongType
        | ErrorKind::InvalidValue
        | ErrorKind::LimitExceeded
        | ErrorKind::Inconsistent
        | ErrorKind::Noncanonical => AnalysisErrorCode::ConfigurationInvalid,
    };
    code(analysis)
}

pub(crate) const fn code(code: AnalysisErrorCode) -> ErrorDetail {
    ErrorDetail {
        code,
        path: None,
        path_bytes: None,
        resource: None,
    }
}
