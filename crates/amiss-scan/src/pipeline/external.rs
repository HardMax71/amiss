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
    constraint: Option<(
        amiss_wire::controls::ExecutionConstraintDescriptor,
        &'static str,
    )>,
}

impl ExternalVerified {
    pub(super) fn install(self, effects: &mut crate::policy::Effects) {
        effects.debt = self.debt;
        effects.waiver = self.waiver;
        effects.time = self.time;
        effects.constraint = self.constraint;
    }

    pub(super) const fn debt(&self) -> Option<&crate::policy::DebtContext> {
        self.debt.as_ref()
    }
}

const fn time_invalid_row() -> ErrorDetail {
    ErrorDetail {
        code: AnalysisErrorCode::TrustedTimeInvalid,
        path: None,
        path_bytes: None,
        resource: None,
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
    let time = match &setup_shell.time {
        None => None,
        Some(input) => {
            let identity = crate::report::candidate_identity_digest(provisional);
            crate::policy::verify_time(input, repository, target_ref, &identity)
                .map_err(|row| ("invalid-external-control", row))?;
            Some(crate::policy::TimeContext {
                statement: input.statement.clone(),
                digest: input.statement.digest,
            })
        }
    };
    let constraint = setup_shell
        .constraint
        .as_ref()
        .map(|input| (input.descriptor.clone(), input.trust_source.as_str()));
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
        });
    };
    if (setup_shell.debt.is_some() || setup_shell.waiver.is_some()) && time.is_none() {
        return Err(("invalid-external-control", time_invalid_row()));
    }
    let debt = match (&setup_shell.debt, &time) {
        (None, _) | (Some(_), None) => None,
        (Some(input), Some(context)) => {
            crate::policy::verify_debt(
                input,
                repository,
                target_ref,
                verified_floor,
                &context.statement.evaluation_instant,
                scan_limits.debt_items,
            )
            .map_err(|row| (external_reason(&row), row))?;
            Some(crate::policy::DebtContext {
                digest: input.snapshot.digest,
                trust_source: input.trust_source.as_str(),
                adoption_tree: input.snapshot.adoption_tree.clone(),
                items: input.snapshot.items.clone(),
            })
        }
    };
    let waiver = match (&setup_shell.waiver, &time) {
        (None, _) | (Some(_), None) => None,
        (Some(input), Some(context)) => {
            crate::policy::verify_waiver(
                input,
                repository,
                target_ref,
                verified_floor,
                &context.statement.evaluation_instant,
                scan_limits.waiver_items,
            )
            .map_err(|row| (external_reason(&row), row))?;
            let floor_lists = verified_floor.map(|floor| {
                (
                    floor.floor.authorized_waiver_issuers.clone(),
                    floor.floor.waivable_finding_kinds.clone(),
                )
            });
            let (authorized_issuers, waivable_kinds) = floor_lists.unwrap_or_default();
            Some(crate::policy::WaiverContext {
                digest: input.bundle.digest,
                trust_source: input.trust_source.as_str(),
                candidate_tree: tree,
                items: input.bundle.items.clone(),
                authorized_issuers,
                waivable_kinds,
            })
        }
    };
    Ok(ExternalVerified {
        debt,
        waiver,
        time,
        constraint,
    })
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
