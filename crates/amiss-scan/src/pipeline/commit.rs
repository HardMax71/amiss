use amiss_git::{GitResources, Repository};
use amiss_wire::model::Oid;
use amiss_wire::report::{EngineProvenance, ErrorDetail};

use crate::Error;
use crate::report::{Built, CandidateBlock, Setup, SnapshotIdentity, construct_incomplete};
use crate::resolve::ForgeContext;
use crate::resources::{ScanLimits, ScanResources};

use super::external::{external_gate, external_reason};
use super::{
    Evaluated, ExternalVerified, PipelineFailure, PipelineResult, ResolvedTree, SetupShell,
    binding_mismatch, conclude, controls_failure, detail, effective_limits, effective_shell,
    evaluate_tree, floor_gate, pair_effects, policy_unavailable_reason, resolve_tree,
};

/// The fallback identity projection when a snapshot cannot be established:
/// each supplied commit OID stands in for both identity fields.
fn oid_fallback(
    repo: &Repository,
    setup_shell: &SetupShell,
    base_oid: &Oid,
    candidate_oid: &Oid,
) -> Setup {
    let placeholder = |oid: &Oid| SnapshotIdentity {
        object_format: repo.object_format().into(),
        commit_oid: oid.as_str().to_owned(),
        tree_oid: oid.as_str().to_owned(),
    };
    setup_shell.with(
        placeholder(base_oid),
        CandidateBlock::Commit(placeholder(candidate_oid)),
    )
}

/// Resolves both commit trees, then settles a pending floor binding
/// mismatch against the real snapshot identities.
fn pair_trees(
    repo: &Repository,
    git_resources: &mut GitResources,
    setup_shell: &SetupShell,
    floor_mismatch: Option<ErrorDetail>,
    base_oid: &Oid,
    candidate_oid: &Oid,
) -> PipelineResult<(ResolvedTree, ResolvedTree)> {
    let trees = resolve_tree(repo, git_resources, base_oid).and_then(|base_tree| {
        resolve_tree(repo, git_resources, candidate_oid)
            .map(|candidate_tree| (base_tree, candidate_tree))
    });
    let (base_tree, candidate_tree) = trees.map_err(|defect_detail| {
        PipelineFailure::one(
            oid_fallback(repo, setup_shell, base_oid, candidate_oid),
            defect_detail,
        )
    })?;
    if let Some(row) = floor_mismatch {
        return Err(binding_mismatch(
            setup_shell,
            base_tree.1,
            CandidateBlock::Commit(candidate_tree.1),
            row,
        ));
    }
    if let Some((reason, row)) = &setup_shell.external_defect {
        return Err(controls_failure(
            setup_shell,
            base_tree.1,
            CandidateBlock::Commit(candidate_tree.1),
            reason,
            row.clone(),
        ));
    }
    Ok((base_tree, candidate_tree))
}

/// The commit-pair external-control stage in the fatal order: trusted time,
/// debt binding with its adoption reproduction, then waiver binding.
#[expect(
    clippy::too_many_arguments,
    reason = "the external-control context is the contract's"
)]
fn commit_controls(
    repo: &Repository,
    git_resources: &mut GitResources,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    setup_shell: &SetupShell,
    verified_floor: Option<&crate::policy::FloorInput>,
    scan_limits: ScanLimits,
    base_tree: &ResolvedTree,
    candidate_tree: &ResolvedTree,
) -> PipelineResult<ExternalVerified> {
    let failure = |reason: &'static str, row: ErrorDetail| {
        controls_failure(
            setup_shell,
            base_tree.1.clone(),
            CandidateBlock::Commit(candidate_tree.1.clone()),
            reason,
            row,
        )
    };
    let provisional = setup_shell.with(
        base_tree.1.clone(),
        CandidateBlock::Commit(candidate_tree.1.clone()),
    );
    let Some(tree_identity) = amiss_wire::model::TreeIdentity::new(
        repo.object_format(),
        candidate_tree.0.as_str().to_owned(),
    ) else {
        return Err(failure("not-parsed", detail(&Error::Internal, None)));
    };
    let external = external_gate(
        setup_shell,
        verified_floor,
        scan_limits,
        &provisional,
        Some(tree_identity),
    )
    .map_err(|(reason, row)| failure(reason, row))?;
    if let Some(context) = external.debt() {
        crate::adoption::reproduce(repo, git_resources, engine, forge, scan_limits, context)
            .map_err(|row| failure(external_reason(&row), row))?;
    }
    Ok(external)
}

/// The complete commit-pair run: both sides, correlation, and construction.
/// Any accumulated typed error makes the run incomplete with every safely
/// established row retained; the report is emitted either way.
#[must_use]
pub fn commit_pair(
    repo: &Repository,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    setup_shell: &SetupShell,
    base_oid: &Oid,
    candidate_oid: &Oid,
) -> Built {
    commit_pair_result(repo, engine, forge, setup_shell, base_oid, candidate_oid)
        .unwrap_or_else(PipelineFailure::into_built)
}

fn commit_pair_result(
    repo: &Repository,
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    setup_shell: &SetupShell,
    base_oid: &Oid,
    candidate_oid: &Oid,
) -> PipelineResult<Built> {
    let (verified_floor, floor_mismatch) = floor_gate(setup_shell);
    let (scan_limits, git_limits) = effective_limits(verified_floor);
    let setup_shell = &effective_shell(setup_shell, &scan_limits);
    let mut git_resources = GitResources::new(git_limits);
    let (base_tree, candidate_tree) = pair_trees(
        repo,
        &mut git_resources,
        setup_shell,
        floor_mismatch,
        base_oid,
        candidate_oid,
    )?;
    let external = commit_controls(
        repo,
        &mut git_resources,
        engine,
        forge,
        setup_shell,
        verified_floor,
        scan_limits,
        &base_tree,
        &candidate_tree,
    )?;

    let mut base_scan = ScanResources::new(scan_limits);
    let mut candidate_scan = ScanResources::new(scan_limits);
    let (base_policy, candidate_policy) = pair_policies(
        repo,
        &mut git_resources,
        setup_shell,
        (&base_tree, &mut base_scan),
        (&candidate_tree, &mut candidate_scan),
    )?;
    let includes = crate::policy::Includes::union(&base_policy, &candidate_policy);

    let (base, candidate, claims) = evaluated_pair(
        repo,
        &mut git_resources,
        (&mut base_scan, &mut candidate_scan),
        engine,
        forge,
        &external.semantic,
        &includes,
        base_tree,
        candidate_tree,
    );
    Ok(match (base, candidate) {
        (Ok((base, base_failures)), Ok((candidate, candidate_failures))) => {
            let mut failures = base_failures;
            failures.extend(candidate_failures);
            let (effects, site) = pair_effects(
                repo,
                &mut git_resources,
                verified_floor,
                external,
                &base_policy,
                &candidate_policy,
                (&base.discovery, &mut base_scan),
                (&candidate.discovery, &mut candidate_scan),
                &mut failures,
            );
            let mut setup = setup_shell.with(
                base.identity.clone(),
                CandidateBlock::Commit(candidate.identity.clone()),
            );
            setup.policy = effects;
            setup.policy.errors_retained = setup_shell.errors_retained;
            setup.policy.complete_findings = scan_limits.complete_findings;
            conclude(
                &setup,
                (&base.discovery, base.side),
                (&candidate.discovery, candidate.side),
                &site,
                &claims,
                &failures,
            )
        }
        (Err(defect), Ok(_)) | (Ok(_), Err(defect)) => construct_incomplete(
            &oid_fallback(repo, setup_shell, base_oid, candidate_oid),
            &[defect],
        ),
        (Err(base_defect), Err(candidate_defect)) => construct_incomplete(
            &oid_fallback(repo, setup_shell, base_oid, candidate_oid),
            &[base_defect, candidate_defect],
        ),
    })
}

/// Acquires both repository policies, each side on its own per-snapshot
/// ledger, producing the fatal projection on any defect.
fn pair_policies(
    repo: &Repository,
    git_resources: &mut GitResources,
    setup_shell: &SetupShell,
    base: (&ResolvedTree, &mut ScanResources),
    candidate: (&ResolvedTree, &mut ScanResources),
) -> PipelineResult<(crate::policy::PolicySide, crate::policy::PolicySide)> {
    let (base_tree, base_scan) = base;
    let (candidate_tree, candidate_scan) = candidate;
    let fallback = |details: Vec<ErrorDetail>| {
        let mut setup = setup_shell.with(
            base_tree.1.clone(),
            CandidateBlock::Commit(candidate_tree.1.clone()),
        );
        setup.controls_unavailable = Some(policy_unavailable_reason(&details));
        PipelineFailure::new(setup, details)
    };
    let base_policy =
        crate::policy::acquire(repo, git_resources, base_scan, &base_tree.0).map_err(&fallback)?;
    let candidate_policy =
        crate::policy::acquire(repo, git_resources, candidate_scan, &candidate_tree.0)
            .map_err(fallback)?;
    Ok((base_policy, candidate_policy))
}

type Evaluation = Result<(Evaluated, Vec<ErrorDetail>), ErrorDetail>;

/// Both snapshot evaluations, with claim outcomes gathered on the candidate
/// side alone: a claim speaks for what the candidate asserts today.
#[expect(
    clippy::too_many_arguments,
    reason = "the two-sided evaluation context is the contract's"
)]
fn evaluated_pair(
    repo: &Repository,
    git_resources: &mut GitResources,
    scans: (&mut ScanResources, &mut ScanResources),
    engine: &EngineProvenance,
    forge: Option<&ForgeContext>,
    semantic: &crate::semantic::Context,
    includes: &crate::policy::Includes,
    base_tree: ResolvedTree,
    candidate_tree: ResolvedTree,
) -> (Evaluation, Evaluation, Vec<crate::claim::ClaimOutcome>) {
    let (base_scan, candidate_scan) = scans;
    let base = evaluate_tree(
        repo,
        git_resources,
        base_scan,
        engine,
        forge,
        crate::semantic::View {
            labels: semantic.labels.as_ref(),
            routes: None,
        },
        includes,
        base_tree,
        None,
    );
    candidate_scan.scans = std::mem::take(&mut base_scan.scans);
    let mut claims: Vec<crate::claim::ClaimOutcome> = Vec::new();
    let candidate = evaluate_tree(
        repo,
        git_resources,
        candidate_scan,
        engine,
        forge,
        crate::semantic::View {
            labels: semantic.labels.as_ref(),
            routes: Some(semantic.routes.as_ref()),
        },
        includes,
        candidate_tree,
        Some(&mut claims),
    );
    (base, candidate, claims)
}
