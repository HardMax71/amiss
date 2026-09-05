mod tests;

use amiss_wire::human::{atom, atom_bytes};
use amiss_wire::json;
use amiss_wire::report::model::{AvailableFeedback, FeedbackAction, FeedbackItem, RepoPath};
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

#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
struct ReportFeedback {
    payload: FeedbackPayload,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
struct FeedbackPayload {
    feedback: AvailableFeedback,
}

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
    if json::parse(bytes).is_err() {
        return Vec::new();
    }
    let Ok(report) = serde_json::from_slice::<ReportFeedback>(bytes) else {
        return Vec::new();
    };
    let feedback = report.payload.feedback;
    let items = &feedback.items;
    if items.iter().any(|item| {
        let Some(RepoPath::Bytes(encoded)) = &item.target else {
            return false;
        };
        hex::decode(&encoded.bytes_hex)
            .ok()
            .and_then(amiss_wire::model::RepoPath::from_bytes)
            .is_none_or(|path| {
                path.as_str().is_some() || hex::encode(path.as_bytes()) != encoded.bytes_hex
            })
    }) {
        return Vec::new();
    }
    let Ok(displayed) = items
        .iter()
        .take(DISPLAYED_ITEMS)
        .map(item_line)
        .collect::<Result<Vec<_>, _>>()
    else {
        return Vec::new();
    };
    let fixes = items
        .iter()
        .filter(|item| item.action == FeedbackAction::Fix)
        .count();
    let checks = items
        .iter()
        .filter(|item| item.action == FeedbackAction::Check)
        .count();
    let existing = feedback.existing_count;
    let mut lines = vec![format!(
        "findings: fix {fixes}, check {checks}, existing {existing}"
    )];
    lines.extend(displayed);
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

fn item_line(item: &FeedbackItem) -> Result<String, hex::FromHexError> {
    let mut action = item.action.as_ref().to_owned();
    if let Some(first) = action.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    let target = match &item.target {
        Some(RepoPath::Text(path)) => atom(path.as_str()),
        Some(RepoPath::Bytes(path)) => atom_bytes(&hex::decode(&path.bytes_hex)?),
        None => "-".to_owned(),
    };
    Ok(format!(
        "- {action} target {target} affected places {}",
        item.location_count
    ))
}
