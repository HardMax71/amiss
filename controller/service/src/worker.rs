mod tests;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use amiss_controller::{
    AcquiringRunner, Acquisition, AdapterRegistry, Controller, ControllerClock, ControllerError,
    DeliveryHeader, DeliveryLedger, DeliveryRoute, HandleOutcome, IngressPolicy, PlanRegistry,
    ProviderAdapter, RegistryError, Runner, UntrustedDelivery,
};

use crate::{
    AdmissionRequest, ClaimOutcome, ClaimedDelivery, CompleteOutcome, Delivery, DeliveryAdmission,
    DeliveryLease, Inbox, InboxError, Operations, RenewOutcome, RetryOutcome,
};

const RENEWAL_POLL: Duration = Duration::from_secs(5);
pub(crate) const MAX_RETRY_DELAY: Duration = Duration::from_hours(24);

#[derive(Debug, thiserror::Error)]
pub enum DeliveryWorkerError {
    #[error("delivery worker timing is invalid")]
    InvalidTiming,
    #[error("delivery inbox lock is unavailable")]
    InboxLock,
    #[error("delivery inbox cannot be trusted")]
    InboxRead(#[source] InboxError),
    #[error("delivery inbox belongs to another route")]
    InboxRoute,
    #[error("claimed delivery names another route")]
    ClaimedRoute,
    #[error("controller state cannot be trusted")]
    Controller(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("controller worker cannot start")]
    WorkerStart(#[source] std::io::Error),
    #[error("controller worker stopped without a result")]
    WorkerStopped(#[source] mpsc::RecvTimeoutError),
    #[error("delivery inbox cannot be renewed")]
    InboxRenew(#[source] InboxError),
    #[error("delivery inbox lease was lost")]
    InboxLeaseLost,
    #[error("controller worker panicked")]
    WorkerPanicked,
    #[error("delivery inbox cannot complete a row")]
    InboxComplete(#[source] InboxError),
    #[error("delivery inbox cannot schedule a retry")]
    InboxRetry(#[source] InboxError),
    #[error("retry time is too large")]
    RetryTimeConversion(#[source] std::num::TryFromIntError),
    #[error("retry time is too large")]
    RetryTimeOverflow,
    #[error("controller time is unavailable")]
    Clock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkOutcome {
    Processed,
    Waiting { ready_at_unix_millis: i64 },
    Empty,
}

pub struct DeliveryWorkerInput<L, R> {
    pub inbox: Arc<Mutex<Inbox>>,
    pub controller: Controller<L, R>,
    pub admission: Arc<dyn DeliveryAdmission>,
    pub route: DeliveryRoute,
    pub route_id: String,
    pub retry_min: Duration,
    pub retry_max: Duration,
    pub idle_poll: Duration,
    pub clock: Arc<dyn ControllerClock>,
    pub operations: Operations,
}

pub struct AcquiringWorkerSettings {
    pub bootstrap: PathBuf,
    pub scratch: PathBuf,
    pub bootstrap_timeout: Duration,
    pub statement_validity: Duration,
    pub ingress: IngressPolicy,
    pub route: DeliveryRoute,
    pub route_id: String,
    pub retry_min: Duration,
    pub retry_max: Duration,
    pub idle_poll: Duration,
}

pub struct AcquiringWorkerContext<L> {
    pub settings: AcquiringWorkerSettings,
    pub plans: PlanRegistry,
    pub ledger: L,
    pub admission: Arc<dyn DeliveryAdmission>,
    pub clock: Arc<dyn ControllerClock>,
    pub artifacts: Arc<amiss_controller::FileArtifactStore>,
}

#[derive(Debug, thiserror::Error)]
pub enum AcquiringWorkerBuildError {
    #[error("bootstrap runner limits are invalid")]
    InvalidRunnerLimits,
    #[error("provider adapter cannot be registered")]
    Registry(#[from] RegistryError),
    #[error("delivery worker cannot start")]
    Worker(#[from] DeliveryWorkerError),
}

/// Builds the acquisition runner, controller, and durable delivery worker for one lane.
///
/// # Errors
///
/// The runner limits, adapter registry, worker timing, or persisted route is invalid.
pub fn acquiring_worker<L, A>(
    context: AcquiringWorkerContext<L>,
    inbox: Arc<Mutex<Inbox>>,
    operations: Operations,
    adapter: Arc<dyn ProviderAdapter>,
    acquisition: A,
) -> Result<DeliveryWorker<L, AcquiringRunner<A>>, AcquiringWorkerBuildError>
where
    L: DeliveryLedger + Send,
    A: Acquisition + 'static,
{
    let settings = context.settings;
    let runner = AcquiringRunner::new(
        acquisition,
        settings.bootstrap,
        settings.scratch,
        settings.bootstrap_timeout,
        settings.statement_validity,
        Arc::clone(&context.clock),
    )
    .ok_or(AcquiringWorkerBuildError::InvalidRunnerLimits)?;
    let mut registry = AdapterRegistry::new();
    registry.register(adapter)?;
    let controller = Controller::new_with_clock(
        registry,
        context.plans,
        context.ledger,
        runner,
        settings.ingress,
        Arc::clone(&context.clock),
    )
    .with_external_sink(Arc::new(operations.clone()))
    .with_artifact_store(context.artifacts);
    Ok(DeliveryWorker::new(DeliveryWorkerInput {
        inbox,
        controller,
        admission: context.admission,
        route: settings.route,
        route_id: settings.route_id,
        retry_min: settings.retry_min,
        retry_max: settings.retry_max,
        idle_poll: settings.idle_poll,
        clock: context.clock,
        operations,
    })?)
}

/// Drains one durable raw-delivery inbox through the provider-neutral controller.
pub struct DeliveryWorker<L, R> {
    inbox: Arc<Mutex<Inbox>>,
    controller: Controller<L, R>,
    admission: Arc<dyn DeliveryAdmission>,
    route: DeliveryRoute,
    route_id: String,
    retry_min: Duration,
    retry_max: Duration,
    idle_poll: Duration,
    clock: Arc<dyn ControllerClock>,
    operations: Operations,
}

impl<L, R> DeliveryWorker<L, R>
where
    L: DeliveryLedger + Send,
    R: Runner + Send,
{
    /// Builds a worker after checking its timing and persisted-route invariants.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid retry timing, an inaccessible inbox, or a
    /// persisted row that belongs to another configured route.
    pub fn new(input: DeliveryWorkerInput<L, R>) -> Result<Self, DeliveryWorkerError> {
        if input.retry_min.is_zero()
            || input.retry_max < input.retry_min
            || input.retry_max > MAX_RETRY_DELAY
            || input.idle_poll.is_zero()
        {
            return Err(DeliveryWorkerError::InvalidTiming);
        }
        let routes_match = input
            .inbox
            .lock()
            .map_err(|_defect| DeliveryWorkerError::InboxLock)?
            .entries()
            .map_err(DeliveryWorkerError::InboxRead)?
            .iter()
            .all(|entry| entry.route == input.route_id);
        if !routes_match {
            return Err(DeliveryWorkerError::InboxRoute);
        }
        Ok(Self {
            inbox: input.inbox,
            controller: input.controller,
            admission: input.admission,
            route: input.route,
            route_id: input.route_id,
            retry_min: input.retry_min,
            retry_max: input.retry_max,
            idle_poll: input.idle_poll,
            clock: input.clock,
            operations: input.operations,
        })
    }

    /// Processes at most one ready row without sleeping.
    ///
    /// # Errors
    ///
    /// Returns an error when trusted time, durable inbox ownership, or local
    /// controller state cannot be established.
    pub fn work_once(&mut self) -> Result<WorkOutcome, DeliveryWorkerError> {
        let Some(claim) = self.claim(None)? else {
            return Ok(WorkOutcome::Empty);
        };
        self.process_claim(claim)
    }

    fn claim(
        &self,
        stop: Option<&AtomicBool>,
    ) -> Result<Option<ClaimOutcome>, DeliveryWorkerError> {
        let now = self.now()?;
        let mut inbox = self
            .inbox
            .lock()
            .map_err(|_defect| DeliveryWorkerError::InboxLock)?;
        if stop.is_some_and(|stop| stop.load(Ordering::Acquire)) {
            return Ok(None);
        }
        inbox
            .claim(now)
            .map(Some)
            .map_err(DeliveryWorkerError::InboxRead)
    }

    fn process_claim(&mut self, claim: ClaimOutcome) -> Result<WorkOutcome, DeliveryWorkerError> {
        match claim {
            ClaimOutcome::Claimed(claimed) => {
                self.operations.delivery_attempts.inc();
                self.process(claimed)?;
                Ok(WorkOutcome::Processed)
            }
            ClaimOutcome::Waiting {
                ready_at_unix_millis,
            } => Ok(WorkOutcome::Waiting {
                ready_at_unix_millis,
            }),
            ClaimOutcome::Empty => Ok(WorkOutcome::Empty),
        }
    }

    /// Processes rows until `stop` is set or a fatal local invariant fails.
    ///
    /// # Errors
    ///
    /// Returns an error when [`Self::work_once`] cannot safely continue.
    pub fn run(mut self, stop: &AtomicBool) -> Result<(), DeliveryWorkerError> {
        while let Some(claim) = self.claim(Some(stop))? {
            match self.process_claim(claim)? {
                WorkOutcome::Processed => {}
                WorkOutcome::Waiting {
                    ready_at_unix_millis,
                } => {
                    let now = self.now()?;
                    sleep_until(now, ready_at_unix_millis, self.idle_poll);
                }
                WorkOutcome::Empty => std::thread::sleep(self.idle_poll),
            }
        }
        Ok(())
    }

    fn process(&mut self, mut claimed: ClaimedDelivery) -> Result<(), DeliveryWorkerError> {
        if claimed.delivery.route != self.route_id {
            return Err(DeliveryWorkerError::ClaimedRoute);
        }
        let discarded = !self.reauthenticate(&claimed.delivery);
        let decision = if discarded {
            Disposition::Complete
        } else {
            disposition(self.handle(&mut claimed)?)
        };
        match decision {
            Disposition::Complete => {
                self.update_inbox(
                    &claimed,
                    DeliveryWorkerError::InboxComplete,
                    |inbox, lease, now| {
                        inbox
                            .complete(lease, now)
                            .map(|outcome| outcome == CompleteOutcome::Completed)
                    },
                )?;
                self.operations.delivery_completions.inc();
                if discarded {
                    self.operations.delivery_discards.inc();
                }
                Ok(())
            }
            Disposition::Retry(at) => self.retry(&claimed, at),
            Disposition::Backoff => {
                let now = self.now()?;
                let delay = retry_delay(claimed.lease.attempt, self.retry_min, self.retry_max);
                let at = add(now, delay)?;
                self.retry(&claimed, at)
            }
            Disposition::Fatal(error) => Err(DeliveryWorkerError::Controller(Box::new(error))),
        }
    }

    fn reauthenticate(&self, delivery: &Delivery) -> bool {
        let request = AdmissionRequest {
            received_at_unix_millis: delivery.received_at_unix_millis,
            headers: &delivery.headers,
            body: &delivery.body,
        };
        self.admission.admit(request).is_ok_and(|admitted| {
            admitted.is_some_and(|admitted| {
                admitted.route == delivery.route && admitted.source_id == delivery.source_id
            })
        })
    }

    fn handle(
        &mut self,
        claimed: &mut ClaimedDelivery,
    ) -> Result<Result<HandleOutcome, ControllerError<L::Error>>, DeliveryWorkerError> {
        let headers = claimed
            .delivery
            .headers
            .iter()
            .map(|header| DeliveryHeader {
                name: &header.name,
                value: &header.value,
            })
            .collect::<Vec<_>>();
        let input = UntrustedDelivery {
            route: &self.route,
            received_at_unix_millis: claimed.delivery.received_at_unix_millis,
            headers: &headers,
            body: &claimed.delivery.body,
        };
        let mut lease = claimed.lease.clone();
        let inbox = Arc::clone(&self.inbox);
        let clock = Arc::clone(&self.clock);
        let controller = &mut self.controller;
        let (sender, receiver) = mpsc::sync_channel(1);
        let result = std::thread::scope(|scope| {
            let worker = std::thread::Builder::new()
                .name("amiss-controller-delivery".to_owned())
                .spawn_scoped(scope, || {
                    let result = controller.handle(input);
                    let _ignored = sender.send(result);
                })
                .map_err(DeliveryWorkerError::WorkerStart)?;
            let controller_result = loop {
                let wait = renewal_wait(&lease, clock.as_ref())?;
                match receiver.recv_timeout(wait) {
                    Ok(result) => break Ok(result),
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break Err(DeliveryWorkerError::WorkerStopped(
                            mpsc::RecvTimeoutError::Disconnected,
                        ));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let now = trusted_time(clock.as_ref())?;
                        let renewed = inbox
                            .lock()
                            .map_err(|_defect| DeliveryWorkerError::InboxLock)?
                            .renew(&lease, now)
                            .map_err(DeliveryWorkerError::InboxRenew)?;
                        match renewed {
                            RenewOutcome::Renewed(replacement) => lease = replacement,
                            RenewOutcome::Lost => {
                                break Err(DeliveryWorkerError::InboxLeaseLost);
                            }
                        }
                    }
                }
            };
            let joined = worker.join();
            match (controller_result, joined) {
                (Ok(result), Ok(())) => Ok(result),
                (Err(error), Ok(())) => Err(error),
                (Ok(_) | Err(_), Err(_panic)) => Err(DeliveryWorkerError::WorkerPanicked),
            }
        })?;
        claimed.lease = lease;
        Ok(result)
    }

    fn retry(
        &self,
        claimed: &ClaimedDelivery,
        requested_at: i64,
    ) -> Result<(), DeliveryWorkerError> {
        self.update_inbox(
            claimed,
            DeliveryWorkerError::InboxRetry,
            |inbox, lease, now| {
                inbox
                    .retry(lease, now, requested_at.max(now))
                    .map(|outcome| outcome == RetryOutcome::Scheduled)
            },
        )?;
        self.operations.delivery_retries.inc();
        Ok(())
    }

    fn update_inbox(
        &self,
        claimed: &ClaimedDelivery,
        failure: fn(InboxError) -> DeliveryWorkerError,
        update: impl FnOnce(&mut Inbox, &DeliveryLease, i64) -> Result<bool, InboxError>,
    ) -> Result<(), DeliveryWorkerError> {
        let now = self.now()?;
        let mut inbox = self
            .inbox
            .lock()
            .map_err(|_defect| DeliveryWorkerError::InboxLock)?;
        let owned = update(&mut inbox, &claimed.lease, now).map_err(failure)?;
        owned
            .then_some(())
            .ok_or(DeliveryWorkerError::InboxLeaseLost)
    }

    fn now(&self) -> Result<i64, DeliveryWorkerError> {
        trusted_time(self.clock.as_ref())
    }
}

enum Disposition<E> {
    Complete,
    Retry(i64),
    Backoff,
    Fatal(ControllerError<E>),
}

fn disposition<E>(result: Result<HandleOutcome, ControllerError<E>>) -> Disposition<E> {
    match result {
        Ok(HandleOutcome::InProgress {
            retry_at_unix_millis,
            ..
        }) => Disposition::Retry(retry_at_unix_millis),
        Err(
            ControllerError::Provider(_)
            | ControllerError::Publish(_)
            | ControllerError::LeaseLost
            | ControllerError::CompletionLost
            | ControllerError::Artifact(
                amiss_controller::ArtifactError::Full
                | amiss_controller::ArtifactError::Clock
                | amiss_controller::ArtifactError::Io(_),
            ),
        ) => Disposition::Backoff,
        Ok(HandleOutcome::Published { .. } | HandleOutcome::Duplicate { .. })
        | Err(
            ControllerError::Ingress(_)
            | ControllerError::WrongChangeIdentity
            | ControllerError::WrongProviderRun
            | ControllerError::DeliveryBindingConflict,
        ) => Disposition::Complete,
        Err(
            error @ (ControllerError::UnknownProvider
            | ControllerError::Plan(_)
            | ControllerError::Ledger(_)
            | ControllerError::Completion(_)
            | ControllerError::Artifact(
                amiss_controller::ArtifactError::AlreadyOpen
                | amiss_controller::ArtifactError::Configuration
                | amiss_controller::ArtifactError::Corrupt
                | amiss_controller::ArtifactError::Conflict
                | amiss_controller::ArtifactError::NotFound
                | amiss_controller::ArtifactError::TooLarge,
            )),
        ) => Disposition::Fatal(error),
    }
}

fn retry_delay(attempt: u64, minimum: Duration, maximum: Duration) -> Duration {
    let shift = u32::try_from(attempt.saturating_sub(1).min(16)).unwrap_or(16);
    minimum.saturating_mul(1_u32 << shift).min(maximum)
}

fn renewal_wait(
    lease: &DeliveryLease,
    clock: &dyn ControllerClock,
) -> Result<Duration, DeliveryWorkerError> {
    let now = trusted_time(clock)?;
    let remaining = lease.expires_at_unix_millis.saturating_sub(now);
    let millis = u64::try_from(remaining)
        .ok()
        .filter(|millis| *millis > 1)
        .map_or(1, |millis| millis / 2);
    Ok(Duration::from_millis(millis).min(RENEWAL_POLL))
}

fn sleep_until(now: i64, ready_at: i64, maximum: Duration) {
    let millis = ready_at.saturating_sub(now);
    let delay = u64::try_from(millis)
        .ok()
        .map_or(Duration::ZERO, Duration::from_millis)
        .min(maximum);
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
}

fn add(now: i64, duration: Duration) -> Result<i64, DeliveryWorkerError> {
    let millis =
        i64::try_from(duration.as_millis()).map_err(DeliveryWorkerError::RetryTimeConversion)?;
    now.checked_add(millis)
        .ok_or(DeliveryWorkerError::RetryTimeOverflow)
}

fn trusted_time(clock: &dyn ControllerClock) -> Result<i64, DeliveryWorkerError> {
    clock
        .now_unix_millis()
        .filter(|now| *now >= 0)
        .ok_or(DeliveryWorkerError::Clock)
}
