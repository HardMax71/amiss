use std::collections::BTreeSet;

use amiss_git::{GitResources, Repository};
use amiss_wire::model::Oid;
use amiss_wire::report::{AnalysisErrorCode, EngineProvenance, ErrorDetail};

use crate::pipeline::{detail, side_observations};
use crate::policy::DebtContext;
use crate::resolve::GithubContext;
use crate::resources::{ScanLimits, ScanResources};

const fn mismatch() -> ErrorDetail {
    ErrorDetail {
        code: AnalysisErrorCode::ControlBindingMismatch,
        path: None,
        resource: None,
    }
}

/// The historical debt binding is not trusted merely because its digest is
/// present: the adoption tree is reopened and every distinct debt document
/// policy-free re-evaluated under the current adapter contracts. Each item
/// must reproduce exactly one ordinary occurrence with the embedded key
/// input and accepted fact; zero, multiple, or different reproduction is a
/// control-binding mismatch, and an acquisition or parse defect inside the
/// adoption evaluation is its own ordinary fatal error. The reproduction
/// consumes ordinary budgets on the adoption snapshot's own ledger.
///
/// # Errors
///
/// One typed detail: the binding mismatch or the first ordinary defect.
pub fn reproduce(
    repo: &Repository,
    git: &mut GitResources,
    engine: &EngineProvenance,
    github: Option<&GithubContext>,
    scan_limits: ScanLimits,
    context: &DebtContext,
) -> Result<(), ErrorDetail> {
    if context.adoption_tree.object_format != repo.object_format() {
        return Err(mismatch());
    }
    let Some(tree) = Oid::new(repo.object_format(), context.adoption_tree.tree_oid.clone()) else {
        return Err(mismatch());
    };
    let documents: BTreeSet<String> = context
        .items
        .iter()
        .map(|item| item.key_input.scope.document.as_str().to_owned())
        .collect();

    let mut scan = ScanResources::new(scan_limits);
    let includes = crate::policy::Includes::default();
    let discovery =
        crate::discovery::discover_scoped(repo, git, &mut scan, &includes, &tree, &documents)
            .map_err(|defect| detail(&defect, None))?;
    let (side, failures) = side_observations(repo, git, &mut scan, engine, github, &discovery)?;
    if let Some(first) = failures.into_iter().next() {
        return Err(first);
    }

    let facts = crate::evaluate::structural_facts(&side.observations);
    for item in &context.items {
        if facts.get(&item.finding_key) != Some(&(1, item.accepted_fact_digest)) {
            return Err(mismatch());
        }
    }
    Ok(())
}
