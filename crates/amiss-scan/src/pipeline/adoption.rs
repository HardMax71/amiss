use std::collections::BTreeSet;

use amiss_git::{GitResources, Repository};
use amiss_wire::model::RepoPath;
use amiss_wire::report::{EngineProvenance, ErrorDetail, model::AnalysisErrorCode};

use super::{ObservationContext, detail, side_observations};
use crate::policy::DebtContext;
use crate::resolve::ForgeContext;
use crate::resources::{ScanLimits, ScanResources};

const fn mismatch() -> ErrorDetail {
    ErrorDetail {
        code: AnalysisErrorCode::ControlBindingMismatch,
        path: None,
        path_bytes: None,
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
pub(super) fn reproduce(
    repo: &Repository,
    git: &mut GitResources,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    scan_limits: ScanLimits,
    context: &DebtContext,
) -> Result<(), ErrorDetail> {
    if context.adoption_tree.object_format != repo.object_format() {
        return Err(mismatch());
    }
    let tree = context.adoption_tree.tree_oid.clone();
    let documents: BTreeSet<RepoPath> = context
        .items
        .iter()
        .map(|item| RepoPath::from(&item.accepted_fact.key_input.scope.document))
        .collect();

    let mut scan = ScanResources::new(scan_limits);
    let policy = crate::policy::acquire(repo, git, &mut scan, &tree)
        .map_err(|details| details.into_iter().next().unwrap_or_else(mismatch))?;
    let forge = crate::policy::aliased(forge, &policy);
    let renderers: Option<BTreeSet<String>> = policy
        .policy
        .and_then(|policy| policy.anchor_renderers)
        .map(|names| names.into_iter().collect());
    let includes = crate::policy::Includes::default();
    let discovery =
        crate::discovery::discover_scoped(repo, git, &mut scan, &includes, &tree, &documents)
            .map_err(|defect| detail(&defect, None))?;
    let semantic = crate::semantic::Context::default();
    let (side, failures) = side_observations(
        repo,
        git,
        &mut scan,
        ObservationContext {
            engine,
            forge: forge.as_ref(),
            semantic: crate::semantic::View {
                labels: semantic.labels.as_ref(),
                routes: None,
            },
            renderers: renderers.as_ref(),
        },
        &discovery,
        None,
    )?;
    if let Some(first) = failures.into_iter().next() {
        return Err(first);
    }

    let facts = crate::evaluate::structural_facts(&side.observations)
        .map_err(|defect| detail(&defect, None))?;
    for item in &context.items {
        if facts.get(&item.finding_key) != Some(&(1, item.accepted_fact_digest)) {
            return Err(mismatch());
        }
    }
    Ok(())
}
