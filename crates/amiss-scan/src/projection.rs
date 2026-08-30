use std::borrow::Cow;
use std::collections::BTreeMap;

use amiss_wire::controls::{ProjectionAssertion, ProjectionKind, ProjectionSource};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::{ArtifactId, RepoPath};

use crate::Error;
use crate::discovery::{DocumentStatus, SnapshotDiscovery};
use crate::resolve::Resolver;
use crate::resources::Aggregate;
use crate::scan::{SemanticCodeSink, SpanDisplay};
use crate::semantic::RecordSet;

mod inventory;
mod repository;

pub use repository::{
    RepositoryProjectionLimits, RepositoryProjectionOutcome, RepositoryProjectionRequest,
    project_repository,
};

pub(crate) const CODE_TEXT_SOURCE_DOMAIN: &str = "amiss/scanner-code-text-source";

pub(crate) fn normalized_line_endings(selected: &[u8]) -> Cow<'_, [u8]> {
    if !selected.contains(&b'\r') {
        return Cow::Borrowed(selected);
    }
    let mut normalized = Vec::with_capacity(selected.len());
    let mut bytes = selected.iter().copied().peekable();
    while let Some(byte) = bytes.next() {
        if byte == b'\r' {
            if bytes.peek() == Some(&b'\n') {
                let _line_feed = bytes.next();
            }
            normalized.push(b'\n');
        } else {
            normalized.push(byte);
        }
    }
    Cow::Owned(normalized)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum DriftReason {
    SinkDocumentUnavailable,
    SinkAbsent,
    SinkAmbiguous,
    SinkNotAdjacent,
    SourceAbsent,
    SourceNotABlob,
    SourceLfsPointer,
    SourceLinesOutOfRange,
    SourceStartMarkerAbsent,
    SourceStartMarkerAmbiguous,
    SourceEndMarkerAbsent,
    SourceEndMarkerAmbiguous,
    SourceRegionOrderInvalid,
    SourceRegionNotUtf8,
    SourceTreeRootAbsent,
    SourceTreeRootNotATree,
    SourceTreePathNotUtf8,
    SourceTreePathNotARow,
    SourceRecordSetAbsent,
    SourceRecordSetIncomplete,
    SourceRecordAbsent,
    SourceRecordUnproven,
    ContentDiffers,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowDifference {
    pub ordering_only: bool,
    pub expected_records: u64,
    pub observed_records: u64,
    pub missing_records: u64,
    pub extra_records: u64,
    pub missing_preview: Vec<String>,
    pub extra_preview: Vec<String>,
    pub missing_omitted: u64,
    pub extra_omitted: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Difference {
    Rows(Box<RowDifference>),
    Count {
        expected_count: u64,
        observed_count: Option<u64>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Attested,
    Drift {
        reason: DriftReason,
        expected_digest: Option<Digest>,
        observed_digest: Option<Digest>,
        expected_bytes: Option<u64>,
        observed_bytes: Option<u64>,
        difference: Option<Difference>,
    },
}

pub(crate) fn unavailable(reason: DriftReason, sink: &SemanticCodeSink) -> Verdict {
    Verdict::Drift {
        reason,
        expected_digest: None,
        observed_digest: Some(sink.digest),
        expected_bytes: None,
        observed_bytes: Some(u64::try_from(sink.value.len()).unwrap_or(u64::MAX)),
        difference: None,
    }
}

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
    reason: DriftReason,
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
    use ProjectionKind::{CodeTextV1, DecimalCountV1, SortedRowsV1};
    use ProjectionSource::{BlobLines, NamedRegion, RecordSet, RecordValue, TreePaths};

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
            DriftReason::SinkDocumentUnavailable,
        ));
    };
    let carriers: Vec<_> = scanned
        .governed
        .iter()
        .filter(|governed| {
            matches!(
                &governed.form,
                crate::claim::GovernedForm::Projection { name } if name == &assertion.name
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
            DriftReason::SinkAbsent
        } else {
            DriftReason::SinkAmbiguous
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
            DriftReason::SinkNotAdjacent,
        ));
    };
    let verdict = match (assertion.projection, &assertion.source) {
        (CodeTextV1, BlobLines(_) | NamedRegion(_)) => {
            resolver.resolve_code_projection(&assertion.source, sink)?
        }
        (CodeTextV1, RecordValue(_)) | (SortedRowsV1 | DecimalCountV1, RecordSet(_)) => {
            record_projection(
                record_sets,
                &assertion.source,
                assertion.projection,
                sink,
                resolver.scan,
            )?
        }
        (SortedRowsV1 | DecimalCountV1, TreePaths(selection)) => inventory::evaluate(
            discovery,
            selection,
            assertion.projection,
            sink,
            resolver.scan,
        )?,
        (CodeTextV1, TreePaths(_) | RecordSet(_))
        | (SortedRowsV1 | DecimalCountV1, BlobLines(_) | NamedRegion(_) | RecordValue(_)) => {
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
        | ProjectionSource::TreePaths(_) => return Err(Error::Internal),
    };
    let Some(set) = record_sets.get(set_name) else {
        return Ok(unavailable(DriftReason::SourceRecordSetAbsent, sink));
    };
    match source {
        ProjectionSource::RecordValue(selection) => {
            if projection != ProjectionKind::CodeTextV1 {
                return Err(Error::Internal);
            }
            let Some(value) = set.records.get(&selection.key) else {
                return Ok(unavailable(
                    if set.complete {
                        DriftReason::SourceRecordAbsent
                    } else {
                        DriftReason::SourceRecordUnproven
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
            if value == &sink.value {
                return Ok(Verdict::Attested);
            }
            Ok(Verdict::Drift {
                reason: DriftReason::ContentDiffers,
                expected_digest: Some(hb(CODE_TEXT_SOURCE_DOMAIN, value.as_bytes())),
                observed_digest: Some(sink.digest),
                expected_bytes: Some(u64::try_from(value.len()).unwrap_or(u64::MAX)),
                observed_bytes: Some(u64::try_from(sink.value.len()).unwrap_or(u64::MAX)),
                difference: None,
            })
        }
        ProjectionSource::RecordSet(_) => {
            if projection == ProjectionKind::CodeTextV1 {
                return Err(Error::Internal);
            }
            if !set.complete {
                return Ok(unavailable(DriftReason::SourceRecordSetIncomplete, sink));
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
                ProjectionKind::CodeTextV1 => Err(Error::Internal),
            }
        }
        ProjectionSource::BlobLines(_)
        | ProjectionSource::NamedRegion(_)
        | ProjectionSource::TreePaths(_) => Err(Error::Internal),
    }
}
