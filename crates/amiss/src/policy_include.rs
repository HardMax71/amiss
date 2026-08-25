use std::process::ExitCode;

use amiss_git::{GitLimits, GitResources, Repository, parse_index_file};
use amiss_scan::policy::{Includes, PolicySide};
use amiss_wire::controls::document_include_value;
use amiss_wire::json;
use amiss_wire::model::RepoPath;

use crate::invocation::{PolicyIncludeInvocation, PolicyIncludePreview};

#[expect(clippy::print_stderr, reason = "authoring refusal channel")]
pub(crate) fn run(invocation: &PolicyIncludeInvocation) -> ExitCode {
    let Some(include) = invocation.policy.document_includes().first() else {
        eprintln!("amiss policy-include: the validated selector is unavailable");
        return ExitCode::FAILURE;
    };
    let result = match &invocation.preview {
        None => crate::output::write_json(&document_include_value(include.clone())),
        Some(preview) => {
            let Some(paths) = staged_paths(invocation, preview) else {
                return ExitCode::FAILURE;
            };
            crate::output::write_json_array(&paths, |path| json::canonical(&path.to_value()))
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(defect) if defect.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(_defect) => {
            eprintln!("amiss policy-include: stdout could not be written");
            ExitCode::FAILURE
        }
    }
}

#[expect(clippy::print_stderr, reason = "authoring refusal channel")]
fn staged_paths(
    invocation: &PolicyIncludeInvocation,
    preview: &PolicyIncludePreview,
) -> Option<Vec<RepoPath>> {
    let repository = match Repository::open(&preview.repo, preview.object_format) {
        Ok(repository) => repository,
        Err(_defect) => {
            eprintln!("amiss policy-include: the repository is unavailable");
            return None;
        }
    };
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let index_bytes = match repository.read_index_bytes(&mut resources) {
        Ok(bytes) => bytes,
        Err(_defect) => {
            eprintln!("amiss policy-include: the staged index is unavailable");
            return None;
        }
    };
    let index = match parse_index_file(preview.object_format, &index_bytes) {
        Ok(index) => index,
        Err(_defect) => {
            eprintln!("amiss policy-include: the staged index is invalid");
            return None;
        }
    };
    let observed = u64::try_from(index.entries.len()).unwrap_or(u64::MAX);
    if observed > resources.limits().tree_entries_per_snapshot {
        eprintln!("amiss policy-include: the staged index exceeds the scanner entry ceiling");
        return None;
    }

    let candidate = PolicySide {
        digest: Some(invocation.policy.digest()),
        policy: Some(invocation.policy.clone()),
    };
    let includes = Includes::union(&PolicySide::default(), &candidate);
    let paths = index
        .entries
        .into_iter()
        .filter_map(|entry| RepoPath::from_bytes(entry.path))
        .filter(|path| includes.matches(path))
        .collect();
    if repository
        .verify_index_unchanged(&mut resources, &index_bytes)
        .is_err()
    {
        eprintln!("amiss policy-include: the staged index changed during the preview");
        return None;
    }
    Some(paths)
}
