use serde::Serialize;

use amiss_wire::digest::Digest;
use amiss_wire::report::{AnalysisErrorCode, FindingKind};

#[derive(Serialize)]
pub(crate) struct Log<'report> {
    #[serde(rename = "$schema")]
    pub(super) schema: &'static str,
    pub(super) runs: [Run<'report>; 1],
    pub(super) version: &'static str,
}

#[derive(Serialize)]
pub(super) struct Run<'report> {
    pub(super) invocations: [Invocation<'report>; 1],
    pub(super) results: Vec<FindingResult<'report>>,
    pub(super) tool: Tool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Invocation<'report> {
    pub(super) execution_successful: bool,
    pub(super) exit_code: u8,
    pub(super) tool_execution_notifications: Vec<Notification<'report>>,
}

#[derive(Serialize)]
pub(super) struct Notification<'report> {
    pub(super) descriptor: Descriptor,
    pub(super) level: Level,
    pub(super) message: Message<'report>,
}

#[derive(Serialize)]
pub(super) struct Descriptor {
    pub(super) id: AnalysisErrorCode,
}

#[derive(Serialize)]
pub(super) struct Tool {
    pub(super) driver: Driver,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Driver {
    pub(super) information_uri: &'static str,
    pub(super) name: &'static str,
    pub(super) rules: Vec<Rule>,
    pub(super) semantic_version: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Rule {
    pub(super) id: FindingKind,
    pub(super) short_description: Message<'static>,
}

#[derive(Serialize)]
pub(super) struct Message<'report> {
    pub(super) text: &'report str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FindingResult<'report> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) fixes: Option<[Fix<'report>; 1]>,
    pub(super) level: Level,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) locations: Option<[Location; 1]>,
    pub(super) message: Message<'report>,
    pub(super) partial_fingerprints: Fingerprints,
    pub(super) rule_id: FindingKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) rule_index: Option<usize>,
}

#[derive(Serialize)]
pub(super) struct Fingerprints {
    #[serde(rename = "amissFindingKey/v1")]
    pub(super) finding_key: Digest,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Fix<'report> {
    pub(super) artifact_changes: [ArtifactChange<'report>; 1],
    pub(super) description: Message<'report>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArtifactChange<'report> {
    pub(super) artifact_location: ArtifactLocation,
    pub(super) replacements: [Replacement<'report>; 1],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Replacement<'report> {
    pub(super) deleted_region: ByteRegion,
    pub(super) inserted_content: Message<'report>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ByteRegion {
    pub(super) byte_length: u64,
    pub(super) byte_offset: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Location {
    pub(super) physical_location: PhysicalLocation,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PhysicalLocation {
    pub(super) artifact_location: ArtifactLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) region: Option<Region>,
}

#[derive(Serialize)]
pub(super) struct ArtifactLocation {
    pub(super) uri: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Region {
    pub(super) end_column: u64,
    pub(super) end_line: u64,
    pub(super) start_column: u64,
    pub(super) start_line: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Level {
    Error,
    Warning,
    Note,
}
