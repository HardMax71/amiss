use sha2::Digest as _;
use std::collections::BTreeMap;

use amiss_wire::controls::{
    KeyFormat, KeyValueSelection, ProjectionAssertion, ProjectionKind, ProjectionSource,
};
use amiss_wire::model::Digest;
use amiss_wire::model::{ArtifactId, RepoPath};
use amiss_wire::report::model::ProjectionObserved;

use crate::Error;
use crate::discovery::{DocumentStatus, SnapshotDiscovery};
use crate::resolve::Resolver;
use crate::resources::Aggregate;
use crate::scanned::{
    CODE_TEXT_SOURCE_DOMAIN, SemanticCodeSink, SpanDisplay, Verdict, unavailable,
};
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
            Ok(code_verdict(value.as_bytes(), projection, sink))
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

/// The verdict one code-text value earns against the visible block: equal to
/// it under `code-text-v1`, somewhere inside it under `contains-v1`.
pub(crate) fn code_verdict(
    expected: &[u8],
    projection: ProjectionKind,
    sink: &SemanticCodeSink,
) -> Verdict {
    let observed = sink.value.as_bytes();
    let (held, reason) = match projection {
        ProjectionKind::ContainsV1 => (
            memchr::memmem::find(observed, expected).is_some(),
            ProjectionObserved::ContentAbsent,
        ),
        ProjectionKind::CodeTextV1
        | ProjectionKind::SortedRowsV1
        | ProjectionKind::DecimalCountV1 => {
            (expected == observed, ProjectionObserved::ContentDiffers)
        }
    };
    if held {
        return Verdict::Attested;
    }
    Verdict::Drift {
        reason,
        expected_digest: Some(Digest::from(
            sha2::Sha256::new_with_prefix(CODE_TEXT_SOURCE_DOMAIN)
                .chain_update([0_u8])
                .chain_update(expected)
                .finalize()
                .0,
        )),
        observed_digest: Some(sink.digest),
        expected_bytes: Some(u64::try_from(expected.len()).unwrap_or(u64::MAX)),
        observed_bytes: Some(u64::try_from(observed.len()).unwrap_or(u64::MAX)),
        difference: None,
    }
}

/// The scalar one key path names in a TOML or JSON file, spelled the way the
/// file's own grammar prints it: a string as its text, a number, boolean or
/// date as written. A table, array or null names no one value.
pub(crate) fn key_value(
    body: &[u8],
    selection: &KeyValueSelection,
) -> Result<String, ProjectionObserved> {
    let text = std::str::from_utf8(body).map_err(|_defect| ProjectionObserved::SourceUnparsable)?;
    match selection.format {
        KeyFormat::Toml => {
            let mut table: toml::Table =
                toml::from_str(text).map_err(|_defect| ProjectionObserved::SourceUnparsable)?;
            let (last, parents) = selection
                .key
                .split_last()
                .ok_or(ProjectionObserved::SourceKeyAbsent)?;
            for segment in parents {
                match table.remove(segment) {
                    Some(toml::Value::Table(inner)) => table = inner,
                    Some(_) | None => return Err(ProjectionObserved::SourceKeyAbsent),
                }
            }
            match table
                .remove(last)
                .ok_or(ProjectionObserved::SourceKeyAbsent)?
            {
                toml::Value::String(value) => Ok(value),
                toml::Value::Integer(value) => Ok(value.to_string()),
                toml::Value::Float(value) => Ok(value.to_string()),
                toml::Value::Boolean(value) => Ok(value.to_string()),
                toml::Value::Datetime(value) => Ok(value.to_string()),
                toml::Value::Array(_) | toml::Value::Table(_) => {
                    Err(ProjectionObserved::SourceKeyNotScalar)
                }
            }
        }
        KeyFormat::Json => {
            let mut value: serde_json::Value = serde_json::from_str(text)
                .map_err(|_defect| ProjectionObserved::SourceUnparsable)?;
            for segment in &selection.key {
                value = match value {
                    serde_json::Value::Object(mut object) => object
                        .remove(segment)
                        .ok_or(ProjectionObserved::SourceKeyAbsent)?,
                    serde_json::Value::Null
                    | serde_json::Value::Bool(_)
                    | serde_json::Value::Number(_)
                    | serde_json::Value::String(_)
                    | serde_json::Value::Array(_) => {
                        return Err(ProjectionObserved::SourceKeyAbsent);
                    }
                };
            }
            match value {
                serde_json::Value::String(value) => Ok(value),
                serde_json::Value::Number(value) => Ok(value.to_string()),
                serde_json::Value::Bool(value) => Ok(value.to_string()),
                serde_json::Value::Null
                | serde_json::Value::Array(_)
                | serde_json::Value::Object(_) => Err(ProjectionObserved::SourceKeyNotScalar),
            }
        }
    }
}
