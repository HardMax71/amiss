use std::collections::BTreeMap;

use amiss_wire::controls::{ProjectionAssertion, ProjectionKind, ProjectionSource};
use amiss_wire::model::Digest;
use amiss_wire::model::{ArtifactId, RepoPath};
use amiss_wire::report::model::ProjectionObserved;

use crate::Error;
use crate::discovery::{DocumentStatus, SnapshotDiscovery};
use crate::resolve::Resolver;
use crate::resources::Aggregate;
use crate::scanned::{SemanticCodeSink, SpanDisplay, Verdict, unavailable};
use crate::semantic::RecordSet;

mod inventory;
mod repository;

pub use repository::{
    RepositoryProjectionLimits, RepositoryProjectionOutcome, RepositoryProjectionRequest,
    project_repository,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub assertion: ProjectionAssertion,
    pub carrier_digests: Vec<Digest>,
    pub answered_spans: Vec<(usize, usize)>,
    pub representative_span: Option<(usize, usize)>,
    pub representative_display: Option<SpanDisplay>,
    pub verdict: Verdict,
}

fn drift(
    assertion: &ProjectionAssertion,
    carrier_digests: Vec<Digest>,
    answered_spans: Vec<(usize, usize)>,
    representative: Option<((usize, usize), SpanDisplay)>,
    reason: ProjectionObserved,
) -> Outcome {
    Outcome {
        assertion: assertion.clone(),
        carrier_digests,
        answered_spans,
        representative_span: representative.map(|value| value.0),
        representative_display: representative.map(|value| value.1),
        verdict: Verdict::Drift {
            reason,
            expected_digest: None,
            observed_digest: None,
            expected_bytes: None,
            observed_bytes: None,
            difference: None,
        },
    }
}

pub(crate) fn evaluate(
    resolver: &mut Resolver<'_>,
    discovery: &SnapshotDiscovery,
    record_sets: &BTreeMap<ArtifactId, RecordSet>,
    assertion: &ProjectionAssertion,
) -> Result<Outcome, Error> {
    use ProjectionKind::{CodeTextV1, ContainsV1, DecimalCountV1, SortedRowsV1};
    use ProjectionSource::{
        Blob, BlobLines, KeyValue, NamedRegion, RecordSet, RecordValue, TreePaths,
    };

    resolver.scan.charge(Aggregate::ProjectionAssertions, 1)?;
    let document = RepoPath::from(&assertion.document);
    let Some(scanned) =
        discovery
            .document(document.as_bytes())
            .and_then(|record| match &record.status {
                DocumentStatus::Scanned(scanned) => Some(scanned),
                DocumentStatus::ExcludedBuiltIn
                | DocumentStatus::Unsupported(_)
                | DocumentStatus::Failed(_) => None,
            })
    else {
        return Ok(drift(
            assertion,
            Vec::new(),
            Vec::new(),
            None,
            ProjectionObserved::SinkDocumentUnavailable,
        ));
    };
    let carriers: Vec<_> = scanned
        .governed
        .iter()
        .filter(|governed| {
            matches!(
                &governed.form,
                crate::scanned::GovernedForm::Projection { name } if name == &assertion.name
            )
        })
        .collect();
    let carrier_digests = carriers.iter().map(|carrier| carrier.digest).collect();
    let answered_spans = carriers.iter().map(|carrier| carrier.span).collect();
    let representative = carriers
        .first()
        .map(|carrier| (carrier.span, carrier.display));
    let [carrier] = carriers.as_slice() else {
        let reason = if carriers.is_empty() {
            ProjectionObserved::SinkAbsent
        } else {
            ProjectionObserved::SinkAmbiguous
        };
        return Ok(drift(
            assertion,
            carrier_digests,
            answered_spans,
            representative,
            reason,
        ));
    };
    let Some(sink) = &carrier.previous_code else {
        return Ok(drift(
            assertion,
            carrier_digests,
            answered_spans,
            representative,
            ProjectionObserved::SinkNotAdjacent,
        ));
    };
    let verdict = match (assertion.projection, &assertion.source) {
        (CodeTextV1 | ContainsV1, BlobLines(_) | NamedRegion(_) | Blob(_) | KeyValue(_)) => {
            resolver.resolve_code_projection(&assertion.source, assertion.projection, sink)?
        }
        (CodeTextV1 | ContainsV1, RecordValue(_))
        | (SortedRowsV1 | DecimalCountV1, RecordSet(_)) => record_projection(
            record_sets,
            &assertion.source,
            assertion.projection,
            sink,
            resolver.scan,
        )?,
        (SortedRowsV1 | DecimalCountV1, TreePaths(selection)) => inventory::evaluate(
            discovery,
            selection,
            assertion.projection,
            sink,
            resolver.scan,
        )?,
        (CodeTextV1 | ContainsV1, TreePaths(_) | RecordSet(_))
        | (
            SortedRowsV1 | DecimalCountV1,
            BlobLines(_) | NamedRegion(_) | RecordValue(_) | Blob(_) | KeyValue(_),
        ) => {
            return Err(Error::Internal);
        }
    };
    Ok(Outcome {
        assertion: assertion.clone(),
        carrier_digests,
        answered_spans,
        representative_span: Some(sink.span),
        representative_display: Some(sink.display),
        verdict,
    })
}

fn record_projection(
    record_sets: &BTreeMap<ArtifactId, RecordSet>,
    source: &ProjectionSource,
    projection: ProjectionKind,
    sink: &SemanticCodeSink,
    resources: &mut crate::resources::ScanResources,
) -> Result<Verdict, Error> {
    let set_name = match source {
        ProjectionSource::RecordValue(selection) => &selection.set,
        ProjectionSource::RecordSet(selection) => &selection.set,
        ProjectionSource::BlobLines(_)
        | ProjectionSource::NamedRegion(_)
        | ProjectionSource::TreePaths(_)
        | ProjectionSource::Blob(_)
        | ProjectionSource::KeyValue(_) => return Err(Error::Internal),
    };
    let Some(set) = record_sets.get(set_name) else {
        return Ok(unavailable(ProjectionObserved::SourceRecordSetAbsent, sink));
    };
    match source {
        ProjectionSource::RecordValue(selection) => {
            if !matches!(
                projection,
                ProjectionKind::CodeTextV1 | ProjectionKind::ContainsV1
            ) {
                return Err(Error::Internal);
            }
            let Some(value) = set.records.get(&selection.key) else {
                return Ok(unavailable(
                    if set.complete {
                        ProjectionObserved::SourceRecordAbsent
                    } else {
                        ProjectionObserved::SourceRecordUnproven
                    },
                    sink,
                ));
            };
            resources.charge(
                Aggregate::ProjectionSelectedBytes,
                u64::try_from(selection.key.len().saturating_add(value.len())).unwrap_or(u64::MAX),
            )?;
            resources.charge(
                Aggregate::ProjectionProjectedBytes,
                u64::try_from(value.len()).unwrap_or(u64::MAX),
            )?;
            Ok(crate::scanned::code_verdict(
                value.as_bytes(),
                projection,
                sink,
            ))
        }
        ProjectionSource::RecordSet(_) => {
            if matches!(
                projection,
                ProjectionKind::CodeTextV1 | ProjectionKind::ContainsV1
            ) {
                return Err(Error::Internal);
            }
            if !set.complete {
                return Ok(unavailable(
                    ProjectionObserved::SourceRecordSetIncomplete,
                    sink,
                ));
            }
            resources.charge(
                Aggregate::ProjectionSelectedBytes,
                set.records.iter().fold(0_u64, |total, (key, value)| {
                    total
                        .saturating_add(u64::try_from(key.len()).unwrap_or(u64::MAX))
                        .saturating_add(u64::try_from(value.len()).unwrap_or(u64::MAX))
                }),
            )?;
            match projection {
                ProjectionKind::SortedRowsV1 => {
                    let mut rows: Vec<&str> = set.records.values().map(String::as_str).collect();
                    rows.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
                    inventory::compare_rows(&rows, sink, resources)
                }
                ProjectionKind::DecimalCountV1 => inventory::compare_count(
                    u64::try_from(set.records.len()).unwrap_or(u64::MAX),
                    sink,
                    resources,
                ),
                ProjectionKind::CodeTextV1 | ProjectionKind::ContainsV1 => Err(Error::Internal),
            }
        }
        ProjectionSource::BlobLines(_)
        | ProjectionSource::NamedRegion(_)
        | ProjectionSource::TreePaths(_)
        | ProjectionSource::Blob(_)
        | ProjectionSource::KeyValue(_) => Err(Error::Internal),
    }
}
