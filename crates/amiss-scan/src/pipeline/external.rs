use amiss_wire::report::{AnalysisErrorCode, ErrorDetail};

use crate::report::Setup;
use crate::resources::ScanLimits;

use super::SetupShell;

/// The verified external controls after the gate, ready to join the run's
/// effects.
#[derive(Default)]
pub(super) struct ExternalVerified {
    debt: Option<crate::policy::DebtContext>,
    waiver: Option<crate::policy::WaiverContext>,
    time: Option<crate::policy::TimeContext>,
    constraint: Option<crate::policy::ConstraintContext>,
    pub(super) semantic: crate::semantic::Context,
}

impl ExternalVerified {
    pub(super) fn install(
        self,
        effects: &mut crate::policy::Effects,
    ) -> crate::semantic::SiteEvaluation {
        effects.debt = self.debt;
        effects.waiver = self.waiver;
        effects.time = self.time;
        effects.constraint = self.constraint;
        effects.semantic_evidence = self.semantic.provenance;
        self.semantic.site
    }

    pub(super) const fn debt(&self) -> Option<&crate::policy::DebtContext> {
        self.debt.as_ref()
    }
}

/// Verifies the wrapper-supplied external controls against the resolved run
/// identity in the fatal order: trusted time, then debt, then waiver. An
/// expiry-bearing control without a verified trusted instant is invalid, and
/// a mismatched control has no effect beyond its typed row and reason.
pub(super) fn external_gate(
    setup_shell: &SetupShell,
    verified_floor: Option<&crate::policy::FloorInput>,
    scan_limits: ScanLimits,
    provisional: &Setup,
    candidate_tree: Option<amiss_wire::model::TreeIdentity>,
) -> Result<ExternalVerified, (&'static str, ErrorDetail)> {
    let repository = setup_shell.repository.as_ref();
    let target_ref = setup_shell.target_ref.as_deref();
    let identity = crate::report::candidate_identity_digest(provisional);
    let time = setup_shell
        .time
        .as_ref()
        .map(|input| crate::policy::verify_time(input, repository, target_ref, &identity))
        .transpose()
        .map_err(|row| ("invalid-external-control", row))?;
    let constraint = setup_shell
        .constraint
        .as_ref()
        .map(crate::policy::verify_constraint)
        .transpose()
        .map_err(|row| ("invalid-external-control", row))?;
    let semantic = crate::semantic::bind(&setup_shell.semantic, identity)
        .map_err(|row| (external_reason(&row), row))?;
    let Some(tree) = candidate_tree else {
        // Debt and waiver values are tree-bound and legal only for a
        // complete Git candidate snapshot; the staged mode rejects them.
        if setup_shell.debt.is_some() || setup_shell.waiver.is_some() {
            return Err((
                "control-binding-mismatch",
                ErrorDetail {
                    code: AnalysisErrorCode::ControlBindingMismatch,
                    path: None,
                    path_bytes: None,
                    resource: None,
                },
            ));
        }
        return Ok(ExternalVerified {
            debt: None,
            waiver: None,
            time,
            constraint,
            semantic,
        });
    };
    if (setup_shell.debt.is_some() || setup_shell.waiver.is_some()) && time.is_none() {
        return Err((
            "invalid-external-control",
            crate::policy::trusted_time_invalid_row(),
        ));
    }
    let debt = setup_shell
        .debt
        .as_ref()
        .zip(time.as_ref())
        .map(|(input, context)| {
            crate::policy::verify_debt(
                input,
                repository,
                target_ref,
                verified_floor,
                &context.statement.evaluation_instant,
                scan_limits.debt_items,
            )
            .map(|()| crate::policy::DebtContext {
                digest: input.digest,
                trust_source: input.trust_source,
                adoption_tree: input.snapshot.adoption_tree.clone(),
                items: input.snapshot.items.clone(),
            })
        })
        .transpose()
        .map_err(|row| (external_reason(&row), row))?;
    let waiver = setup_shell
        .waiver
        .as_ref()
        .zip(time.as_ref())
        .map(|(input, context)| {
            crate::policy::verify_waiver(
                input,
                repository,
                target_ref,
                verified_floor,
                &context.statement.evaluation_instant,
                scan_limits.waiver_items,
            )?;
            let (authorized_issuers, waivable_kinds) =
                verified_floor.map(waiver_authority).unwrap_or_default();
            Ok(crate::policy::WaiverContext {
                digest: input.digest,
                trust_source: input.trust_source,
                candidate_tree: tree,
                items: input.bundle.items.clone(),
                authorized_issuers,
                waivable_kinds,
            })
        })
        .transpose()
        .map_err(|row: ErrorDetail| (external_reason(&row), row))?;
    Ok(ExternalVerified {
        debt,
        waiver,
        time,
        constraint,
        semantic,
    })
}

fn waiver_authority(
    floor: &crate::policy::FloorInput,
) -> (
    Vec<amiss_wire::model::OwnerId>,
    Vec<amiss_wire::controls::EligibleFindingKind>,
) {
    (
        floor.floor.authorized_waiver_issuers().to_vec(),
        floor.floor.waivable_finding_kinds().to_vec(),
    )
}

/// The controls-unavailable reason a rejected external control anchors:
/// binding mismatches and invalid controls name themselves, and any other
/// defect leaves the stage merely not parsed.
pub(super) fn external_reason(row: &ErrorDetail) -> &'static str {
    use amiss_wire::report::AnalysisErrorCode as Code;
    if row.code == Code::ControlBindingMismatch {
        "control-binding-mismatch"
    } else if row.code == Code::TrustedTimeInvalid || row.code == Code::ConfigurationInvalid {
        "invalid-external-control"
    } else {
        "not-parsed"
    }
}
