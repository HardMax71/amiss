use std::time::Duration;

use amiss_wire::model::Oid;

use crate::{
    AcceptedDelivery, ArtifactBundle, ArtifactError, ArtifactReference, AuthenticatedDelivery,
    CheckBinding, ControllerClock, FileArtifactStore, ProviderAdapter,
};

use super::{ControllerError, HandleOutcome};
use crate::orchestration::ledger::{
    DeliveryClaim, DeliveryLease, DeliveryLedger, LeaseCompletion, LeaseRenewal, Publication,
    StageOutcome, StagedPublication,
};
use crate::orchestration::model::{ChangeSnapshot, HeartbeatOutcome, RunHeartbeat, RunIdentity};

pub(super) struct LedgerHeartbeat<'a, L: DeliveryLedger> {
    ledger: &'a mut L,
    delivery: &'a AcceptedDelivery,
    lease: &'a mut DeliveryLease,
    clock: &'a dyn ControllerClock,
    failure: Option<ControllerError<L::Error>>,
}

impl<'a, L: DeliveryLedger> LedgerHeartbeat<'a, L> {
    pub(super) fn new(
        ledger: &'a mut L,
        delivery: &'a AcceptedDelivery,
        lease: &'a mut DeliveryLease,
        clock: &'a dyn ControllerClock,
    ) -> Self {
        Self {
            ledger,
            delivery,
            lease,
            clock,
            failure: None,
        }
    }

    pub(super) fn finish(self) -> Result<(), ControllerError<L::Error>> {
        match self.failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl<L: DeliveryLedger> RunHeartbeat for LedgerHeartbeat<'_, L> {
    fn renew(&mut self) -> HeartbeatOutcome {
        if self.failure.is_some() {
            return HeartbeatOutcome::Stop;
        }
        let renewed = renew_lease(self.ledger, self.delivery, self.lease).and_then(|lease| {
            let renew_within =
                renewal_window(&lease, self.clock).ok_or(ControllerError::LeaseLost)?;
            Ok((lease, renew_within))
        });
        match renewed {
            Ok((lease, renew_within)) => {
                *self.lease = lease;
                HeartbeatOutcome::Renewed { renew_within }
            }
            Err(error) => {
                self.failure = Some(error);
                HeartbeatOutcome::Stop
            }
        }
    }
}

fn renewal_window(lease: &DeliveryLease, clock: &dyn ControllerClock) -> Option<Duration> {
    let now = clock.now_unix_millis()?;
    let millis = u64::try_from(lease.expires_at_unix_millis.checked_sub(now)?).ok()?;
    let remaining = Duration::from_millis(millis);
    (!remaining.is_zero()).then_some(remaining)
}

pub(super) fn renew_lease<L: DeliveryLedger>(
    ledger: &mut L,
    delivery: &AcceptedDelivery,
    lease: &DeliveryLease,
) -> Result<DeliveryLease, ControllerError<L::Error>> {
    let renewal = ledger
        .renew(delivery, lease)
        .map_err(ControllerError::Ledger)?;
    let LeaseRenewal::Renewed(renewed) = renewal else {
        return Err(ControllerError::LeaseLost);
    };
    if renewed.evaluation_id != lease.evaluation_id
        || renewed.check != lease.check
        || renewed.fence != lease.fence
        || renewed.expires_at_unix_millis < lease.expires_at_unix_millis
    {
        return Err(ControllerError::LeaseLost);
    }
    Ok(renewed)
}

pub(super) fn stage_publication<L: DeliveryLedger>(
    ledger: &mut L,
    delivery: &AcceptedDelivery,
    lease: &DeliveryLease,
    publication: &Publication,
) -> Result<StagedPublication, ControllerError<L::Error>> {
    let outcome = ledger
        .stage(delivery, lease, publication)
        .map_err(ControllerError::Ledger)?;
    match outcome {
        StageOutcome::Staged(staged) if staged.publication.as_ref() == publication => {
            validate_staged_lease(lease, staged)
        }
        StageOutcome::Staged(_) | StageOutcome::Lost => Err(ControllerError::LeaseLost),
    }
}

fn validate_staged_lease<E>(
    lease: &DeliveryLease,
    staged: StagedPublication,
) -> Result<StagedPublication, ControllerError<E>> {
    if staged.evaluation_id != lease.evaluation_id
        || staged.fence != lease.fence
        || staged.publication.check != lease.check
    {
        return Err(ControllerError::LeaseLost);
    }
    Ok(staged)
}

pub(super) fn publish_staged<L: DeliveryLedger>(
    adapter: &dyn ProviderAdapter,
    artifacts: Option<&FileArtifactStore>,
    sink: Option<&dyn super::ExternalSink>,
    ledger: &mut L,
    delivery: &AcceptedDelivery,
    staged: &StagedPublication,
) -> Result<HandleOutcome, ControllerError<L::Error>> {
    if let Some(reference) = &staged.publication.artifact {
        if !crate::artifacts::reference_matches_report(
            reference,
            staged.publication.report.as_deref(),
        ) {
            return Err(ControllerError::Artifact(ArtifactError::Conflict));
        }
        artifacts
            .ok_or(ControllerError::Artifact(ArtifactError::NotFound))?
            .verify(reference)
            .map_err(ControllerError::Artifact)?;
    } else if artifacts.is_some() && staged.publication.report.is_some() {
        return Err(ControllerError::Artifact(ArtifactError::NotFound));
    }
    adapter
        .publish(delivery.delivery(), &staged.publication)
        .map_err(ControllerError::Publish)?;
    let outcome = match ledger
        .complete(delivery, staged)
        .map_err(ControllerError::Completion)?
    {
        LeaseCompletion::Completed => HandleOutcome::Published {
            conclusion: staged.publication.conclusion,
            artifact: staged.publication.artifact.clone(),
        },
        LeaseCompletion::Lost => return Err(ControllerError::CompletionLost),
    };
    observe_external(sink, &staged.publication);
    Ok(outcome)
}

pub(super) enum ClaimResolution {
    Execute(DeliveryLease),
    Publish(StagedPublication),
    Return(HandleOutcome),
}

pub(super) fn resolve_claim<E>(
    artifacts: Option<&FileArtifactStore>,
    delivery: &AcceptedDelivery,
    check: &CheckBinding,
    claim: DeliveryClaim,
) -> Result<ClaimResolution, ControllerError<E>> {
    let outcome = match claim {
        DeliveryClaim::Execute(lease) if lease.check == *check => {
            return Ok(ClaimResolution::Execute(lease));
        }
        DeliveryClaim::Execute(_) => return Err(ControllerError::LeaseLost),
        DeliveryClaim::Publish(staged) => {
            validate_staged(delivery.delivery(), check, &staged)?;
            return Ok(ClaimResolution::Publish(staged));
        }
        DeliveryClaim::Busy {
            evaluation_id,
            retry_at_unix_millis,
        } => ClaimResolution::Return(HandleOutcome::InProgress {
            evaluation_id,
            retry_at_unix_millis,
        }),
        DeliveryClaim::Duplicate { evaluation_id } => {
            let artifact = artifacts
                .map(|store| store.find(&evaluation_id))
                .transpose()
                .map_err(ControllerError::Artifact)?
                .flatten();
            ClaimResolution::Return(HandleOutcome::Duplicate {
                evaluation_id,
                artifact,
            })
        }
        DeliveryClaim::BindingConflict => {
            return Err(ControllerError::DeliveryBindingConflict);
        }
    };
    Ok(outcome)
}

pub(super) fn validate_staged<E>(
    delivery: &AuthenticatedDelivery,
    check: &CheckBinding,
    staged: &StagedPublication,
) -> Result<(), ControllerError<E>> {
    if staged.publication.evaluation_id != staged.evaluation_id {
        return Err(ControllerError::LeaseLost);
    }
    if staged.publication.provider_run != delivery.provider_run {
        return Err(ControllerError::WrongProviderRun);
    }
    if staged.publication.check != *check {
        return Err(ControllerError::DeliveryBindingConflict);
    }
    validate_run(delivery, &staged.publication.run)?;
    validate_gate_commit(&staged.publication.run, &staged.publication.gate_commit)
}

pub(super) fn validate_change<E>(
    delivery: &AuthenticatedDelivery,
    snapshot: &ChangeSnapshot,
) -> Result<(), ControllerError<E>> {
    validate_run(delivery, &snapshot.run)?;
    validate_gate_commit(&snapshot.run, &snapshot.gate_commit)
}

fn validate_gate_commit<E>(run: &RunIdentity, gate_commit: &Oid) -> Result<(), ControllerError<E>> {
    Oid::new(run.object_format, gate_commit.as_str().to_owned())
        .ok_or(ControllerError::WrongProviderRun)?;
    Ok(())
}

fn validate_run<E>(
    delivery: &AuthenticatedDelivery,
    run: &RunIdentity,
) -> Result<(), ControllerError<E>> {
    if run.change != delivery.change {
        return Err(ControllerError::WrongChangeIdentity);
    }
    if run.object_format != delivery.provider_run.object_format
        || run.commits.candidate != delivery.provider_run.candidate_commit
    {
        return Err(ControllerError::WrongProviderRun);
    }
    Ok(())
}

pub(super) fn retain_artifact(
    adapter: &dyn ProviderAdapter,
    store: &FileArtifactStore,
    verify_external: bool,
    clock: &dyn ControllerClock,
    publication: &Publication,
) -> Result<ArtifactReference, ArtifactError> {
    let report = publication
        .report
        .as_deref()
        .ok_or(ArtifactError::Corrupt)?;
    if let Some(reference) = store.find(&publication.evaluation_id)? {
        if reference.report_digest != amiss_wire::digest::sha256(report) {
            return Err(ArtifactError::Conflict);
        }
        store.verify(&reference)?;
        return Ok(reference);
    }
    let external = if verify_external {
        prepare_external(adapter, clock, report)
    } else {
        PreparedExternal::default()
    };
    store.retain(
        &publication.evaluation_id,
        ArtifactBundle {
            report,
            plan: external.plan.as_deref(),
            evidence: external.evidence.as_deref(),
            assessment: external.assessment.as_deref(),
            external_tally: external.tally,
            external_incomplete: external.incomplete,
        },
    )
}

#[derive(Default)]
struct PreparedExternal {
    plan: Option<Vec<u8>>,
    evidence: Option<Vec<u8>>,
    assessment: Option<Vec<u8>>,
    tally: Option<super::ExternalTally>,
    incomplete: bool,
}

fn prepare_external(
    adapter: &dyn ProviderAdapter,
    clock: &dyn ControllerClock,
    report: &[u8],
) -> PreparedExternal {
    let Ok(parsed) = amiss_wire::json::parse(report) else {
        return PreparedExternal {
            incomplete: true,
            ..PreparedExternal::default()
        };
    };
    let engine = parsed
        .member("payload")
        .and_then(|payload| payload.member("engine"));
    let (Some(version), Some(digest)) = (
        engine.and_then(|engine| engine.text("engine_version")),
        engine.and_then(|engine| engine.text("engine_digest")),
    ) else {
        return PreparedExternal {
            incomplete: true,
            ..PreparedExternal::default()
        };
    };
    let Ok(plan) = amiss_wire::external::plan(&parsed, version, digest) else {
        return PreparedExternal {
            incomplete: true,
            ..PreparedExternal::default()
        };
    };
    let plan_bytes = amiss_wire::json::canonical(&plan);
    let Some(now) = clock.now_unix_millis() else {
        return PreparedExternal {
            plan: Some(plan_bytes),
            incomplete: true,
            ..PreparedExternal::default()
        };
    };
    match adapter.verify_external(&plan, &now.to_string()) {
        Ok(Some(evidence)) => {
            match amiss_wire::external::assess(&plan, &evidence, version, digest) {
                Ok(assessment) => PreparedExternal {
                    plan: Some(plan_bytes),
                    evidence: Some(amiss_wire::json::canonical(&evidence)),
                    assessment: Some(amiss_wire::json::canonical(&assessment)),
                    tally: Some(tally(&assessment)),
                    incomplete: false,
                },
                Err(_defect) => PreparedExternal {
                    plan: Some(plan_bytes),
                    evidence: Some(amiss_wire::json::canonical(&evidence)),
                    incomplete: true,
                    ..PreparedExternal::default()
                },
            }
        }
        Ok(None) => PreparedExternal {
            plan: Some(plan_bytes),
            ..PreparedExternal::default()
        },
        Err(_defect) => PreparedExternal {
            plan: Some(plan_bytes),
            incomplete: true,
            ..PreparedExternal::default()
        },
    }
}

pub(super) fn observe_external(sink: Option<&dyn super::ExternalSink>, publication: &Publication) {
    let (Some(sink), Some(reference)) = (sink, publication.artifact.as_ref()) else {
        return;
    };
    if reference.external_incomplete {
        sink.incomplete();
    } else if let Some(tally) = &reference.external_tally {
        sink.assessed(tally);
    }
}

fn tally(assessment: &amiss_wire::json::Value) -> super::ExternalTally {
    let mut counted = super::ExternalTally::default();
    let verdicts = assessment
        .member("payload")
        .and_then(|payload| payload.member("verdicts"));
    if let Some(amiss_wire::json::Value::Array(verdicts)) = verdicts {
        for row in verdicts {
            match row.text("verdict") {
                Some("refuted") => counted.refuted = counted.refuted.saturating_add(1),
                Some("unproven") => counted.unproven = counted.unproven.saturating_add(1),
                Some("reachable") => counted.reachable = counted.reachable.saturating_add(1),
                Some(_) | None => {}
            }
        }
    }
    counted
}
