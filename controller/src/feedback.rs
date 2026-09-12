mod tests;

use amiss_wire::human::{atom, atom_bytes, decode_hex};
use serde::Deserialize;

use crate::ArtifactReference;

const DISPLAYED_ITEMS: usize = 10;
const SEMANTIC_PROJECTION_PREFIXES: [&str; 2] = ["semantic-input: ", "semantic-input-artifact: "];
const OPTIONAL_PROJECTION_PREFIXES: [&str; 4] = [
    "semantic-input: ",
    "semantic-input-artifact: ",
    "assessment-artifact: ",
    "external-assessment: ",
];

/// Every repository-derived value passes the human-atom law before it
/// reaches provider markdown.
#[must_use]
pub fn with_feedback(
    text: &str,
    report: Option<&[u8]>,
    artifact: Option<&ArtifactReference>,
) -> Option<String> {
    let report_digest = amiss_wire::digest::sha256(report.unwrap_or_default());
    let mut lines = vec![format!("report: {report_digest}")];
    if let Some(artifact) = artifact {
        if report.is_none() || artifact.report_digest != report_digest {
            return None;
        }
        let component_root = artifact.locator.strip_suffix("/report")?;
        lines.extend([
            format!("artifact: {}", artifact.locator),
            "artifact-auth: bearer".to_owned(),
            format!(
                "artifact-expires-unix-millis: {}",
                artifact.expires_at_unix_millis
            ),
        ]);
        if let Some(digest) = artifact.semantic_digest {
            lines.push(format!("semantic-input: {digest}"));
            lines.push(format!(
                "semantic-input-artifact: {component_root}/semantic"
            ));
        }
        if let Some(digest) = artifact.assessment_digest {
            lines.push(format!("assessment: {digest}"));
            lines.push(format!("assessment-artifact: {component_root}/assessment"));
        }
        if artifact.external_incomplete {
            lines.push("external-assessment: incomplete".to_owned());
        } else if let Some(tally) = artifact.external_tally {
            lines.push(format!(
                "external-assessment: refuted {} unproven {} reachable {}",
                tally.refuted, tally.unproven, tally.reachable
            ));
        }
    }
    lines.extend(feedback_lines(report, artifact.is_some()));
    Some(format!("{text}\n{}", lines.join("\n")))
}

/// Matches the current provider projection or its admitted pre-metadata form.
#[must_use]
pub fn compatible_provider_feedback(actual: &str, expected: &str) -> bool {
    let omitting = |prefixes: &[&str]| {
        expected
            .lines()
            .filter(|line| !prefixes.iter().any(|prefix| line.starts_with(prefix)))
            .eq(actual.lines())
    };
    actual == expected
        || omitting(&SEMANTIC_PROJECTION_PREFIXES)
        || omitting(&OPTIONAL_PROJECTION_PREFIXES)
}

fn feedback_lines(report: Option<&[u8]>, retained: bool) -> Vec<String> {
    let Some(bytes) = report else {
        return Vec::new();
    };
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > amiss_wire::report::MACHINE_JSON_BYTES {
        return Vec::new();
    }
    let Ok(envelope) = amiss_wire::codec::decode::<FeedbackEnvelope>(bytes) else {
        return Vec::new();
    };
    let feedback = envelope.payload.feedback;
    if feedback.status != "available" {
        return Vec::new();
    }
    let items = feedback.items;
    let fixes = items.iter().filter(|item| item.action == "fix").count();
    let checks = items.iter().filter(|item| item.action == "check").count();
    let existing = feedback.existing_count;
    let mut lines = vec![format!(
        "findings: fix {fixes}, check {checks}, existing {existing}"
    )];
    lines.extend(items.iter().take(DISPLAYED_ITEMS).map(item_line));
    let overflow = items.len().saturating_sub(DISPLAYED_ITEMS);
    if overflow == 1 {
        lines.push(if retained {
            "- 1 more item in the retained report".to_owned()
        } else {
            "- 1 more item not displayed".to_owned()
        });
    } else if overflow > 1 {
        lines.push(if retained {
            format!("- {overflow} more items in the retained report")
        } else {
            format!("- {overflow} more items not displayed")
        });
    }
    lines
}

#[derive(Deserialize)]
struct FeedbackEnvelope {
    payload: FeedbackPayload,
}

#[derive(Deserialize)]
struct FeedbackPayload {
    feedback: Feedback,
}

#[derive(Deserialize)]
struct Feedback {
    status: String,
    #[serde(default)]
    items: Vec<FeedbackItem>,
    #[serde(default)]
    existing_count: i64,
}

#[derive(Deserialize)]
struct FeedbackItem {
    #[serde(default)]
    action: String,
    #[serde(default)]
    location_count: i64,
    #[serde(default)]
    target: Option<FeedbackTarget>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FeedbackTarget {
    Text(String),
    Bytes(ByteTarget),
    Other(serde::de::IgnoredAny),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, remote = "Self")]
struct ByteTarget {
    bytes_hex: String,
}

impl<'de> Deserialize<'de> for ByteTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(amiss_wire::codec::object(deserializer))
    }
}

fn item_line(item: &FeedbackItem) -> String {
    let mut action = item.action.clone();
    action.retain(|symbol| symbol.is_ascii_alphanumeric() || symbol == '-');
    if let Some(first) = action.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    let places = item.location_count;
    let target = match &item.target {
        Some(FeedbackTarget::Text(path)) => atom(path),
        Some(FeedbackTarget::Bytes(bytes)) => atom_bytes(&decode_hex(&bytes.bytes_hex)),
        None | Some(FeedbackTarget::Other(_)) => "-".to_owned(),
    };
    format!("- {action} target {target} affected places {places}")
}
