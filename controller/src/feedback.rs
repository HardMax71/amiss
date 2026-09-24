use sha2::Digest as _;
mod tests;

use amiss_wire::envelope::Payload as _;
use amiss_wire::human::{atom, atom_bytes};
use amiss_wire::model::RepoPath;
use amiss_wire::report::model::{Feedback, FeedbackAction, FeedbackItem, ReportPayload};

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
    let report_digest =
        amiss_wire::model::Digest::from(sha2::Sha256::digest(report.unwrap_or_default()).0);
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
    let Ok(report) = <ReportPayload>::parse(bytes) else {
        return Vec::new();
    };
    let Feedback::Available(feedback) = report.payload.feedback else {
        return Vec::new();
    };
    let items = &feedback.items;
    if items.iter().any(|item| {
        item.target.as_ref().is_some_and(|path| {
            path.as_str().is_none()
                && RepoPath::from_bytes(path.as_bytes().to_vec())
                    .is_none_or(|canonical| canonical.as_str().is_some())
        })
    }) {
        return Vec::new();
    }
    let displayed = items.iter().take(DISPLAYED_ITEMS).map(item_line);
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

fn item_line(item: &FeedbackItem) -> String {
    let mut action = item.action.as_ref().to_owned();
    if let Some(first) = action.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    let target = item.target.as_ref().map_or_else(
        || "-".to_owned(),
        |path| {
            path.as_str()
                .map_or_else(|| atom_bytes(path.as_bytes()), atom)
        },
    );
    format!(
        "- {action} target {target} affected places {}",
        item.location_count
    )
}
