use std::collections::BTreeSet;
use std::path::PathBuf;

use amiss_wire::controls::{DocumentInclude, IncludeKind, ScannerPolicy};
use amiss_wire::model::{Adapter, RepoPath, RepoPathText};

use super::super::arguments::Gathered;
use super::super::{AuthorInvocation, Code, PolicyIncludeInvocation, PolicyIncludePreview};
use super::classify_target;

/// The path refuses the bytes the claim url and the extractor cannot carry.
pub(super) fn classify_claim(
    mut codes: BTreeSet<Code>,
    gathered: &Gathered,
) -> Result<AuthorInvocation, BTreeSet<Code>> {
    let foreign = [
        &gathered.object_format,
        &gathered.base,
        &gathered.candidate,
        &gathered.repository,
        &gathered.ref_name,
        &gathered.default_branch_ref,
        &gathered.forge,
        &gathered.profile,
        &gathered.format,
        &gathered.floor_digest,
        &gathered.debt_owner,
        &gathered.debt_reason,
        &gathered.created_at,
        &gathered.expires_at,
        &gathered.debt_output,
        &gathered.report,
        &gathered.plan,
        &gathered.evidence,
        &gathered.semantic_template,
        &gathered.target,
        &gathered.target_bytes_hex,
        &gathered.suffix,
        &gathered.adapter,
    ];
    if foreign.iter().any(|slot| slot.occurrences > 0)
        || gathered.index > 0
        || gathered.explain_scope > 0
    {
        codes.insert(Code::InvalidInvocation);
    }
    let repo = match gathered.repo.unique_value() {
        Some("") | None => {
            codes.insert(Code::InvalidInvocation);
            None
        }
        Some(path) => Some(PathBuf::from(path)),
    };
    let name = gathered
        .claim_name
        .unique_value()
        .filter(|value| amiss_wire::extraction::governed_name_valid(value));
    let line = gathered.claim_line.unique_value().and_then(|value| {
        let lawful = !value.is_empty()
            && value.len() <= 16
            && !value.starts_with('0')
            && value.bytes().all(|byte| byte.is_ascii_digit());
        let ceiling = u64::try_from(amiss_wire::json::MAX_SAFE_INTEGER).ok()?;
        if lawful {
            value.parse::<u64>().ok().filter(|line| *line <= ceiling)
        } else {
            None
        }
    });
    let path = gathered.claim_path.unique_value().filter(|value| {
        RepoPath::new((*value).to_owned()).is_some_and(|path| path.as_str().is_some())
            && !value.contains(['&', '<', '>', '"', ' ', '%', '?', '#', '\\'])
    });
    match (repo, name, line, path) {
        (Some(repo), Some(name), Some(line), Some(path)) if codes.is_empty() => {
            Ok(AuthorInvocation {
                repo,
                path: path.to_owned(),
                line,
                name: name.to_owned(),
            })
        }
        (_, _, _, _) => {
            codes.insert(Code::InvalidInvocation);
            Err(codes)
        }
    }
}

pub(super) fn classify_policy_include(
    mut codes: BTreeSet<Code>,
    gathered: &Gathered,
) -> Result<PolicyIncludeInvocation, BTreeSet<Code>> {
    let foreign = [
        &gathered.base,
        &gathered.candidate,
        &gathered.repository,
        &gathered.ref_name,
        &gathered.default_branch_ref,
        &gathered.forge,
        &gathered.profile,
        &gathered.format,
        &gathered.floor_digest,
        &gathered.debt_owner,
        &gathered.debt_reason,
        &gathered.created_at,
        &gathered.expires_at,
        &gathered.debt_output,
        &gathered.claim_line,
        &gathered.claim_name,
        &gathered.report,
        &gathered.plan,
        &gathered.evidence,
        &gathered.semantic_template,
        &gathered.target,
        &gathered.target_bytes_hex,
    ];
    if foreign.iter().any(|slot| slot.occurrences > 0) || gathered.explain_scope > 0 {
        codes.insert(Code::InvalidInvocation);
    }

    let path = gathered
        .claim_path
        .unique_value()
        .and_then(|value| RepoPathText::new(value.to_owned()));
    let suffix = gathered.suffix.unique_value().map(str::to_owned);
    let adapter = gathered
        .adapter
        .unique_value()
        .and_then(|value| value.parse::<Adapter>().ok());
    let policy = match (path, suffix, adapter) {
        (Some(path), Some(suffix), Some(adapter)) => ScannerPolicy::new(
            vec![DocumentInclude {
                path,
                kind: IncludeKind::Tree,
                suffix: Some(suffix),
                adapter: Some(adapter),
            }],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .ok(),
        (None, _, _) | (_, None, _) | (_, _, None) => None,
    };
    if policy.is_none() {
        codes.insert(Code::InvalidInvocation);
    }

    let preview_presence = [
        gathered.repo.occurrences > 0,
        gathered.object_format.occurrences > 0,
        gathered.index > 0,
    ];
    let preview = if preview_presence == [false, false, false] {
        Some(None)
    } else if preview_presence == [true, true, true] {
        classify_target(gathered).ok().map(|(repo, object_format)| {
            Some(PolicyIncludePreview {
                repo,
                object_format,
            })
        })
    } else {
        None
    };
    if preview.is_none() {
        codes.insert(Code::InvalidInvocation);
    }

    match (policy, preview) {
        (Some(policy), Some(preview)) if codes.is_empty() => {
            Ok(PolicyIncludeInvocation { policy, preview })
        }
        (_, _) => {
            codes.insert(Code::InvalidInvocation);
            Err(codes)
        }
    }
}
