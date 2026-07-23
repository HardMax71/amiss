use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use amiss_controller::{
    Controller, ControllerClock, ControllerError, DeliveryHeader, DeliveryLedger, DeliveryRoute,
    HandleOutcome, Runner, UntrustedDelivery,
};

use crate::{
    AdmissionRequest, ClaimOutcome, ClaimedDelivery, CompleteOutcome, Delivery, DeliveryAdmission,
    DeliveryLease, Inbox, InboxError, RenewOutcome, RetryOutcome,
};

const RENEWAL_POLL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliveryWorkerError(&'static str);

impl fmt::Display for DeliveryWorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for DeliveryWorkerError {}

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
            || input.idle_poll.is_zero()
        {
            return Err(DeliveryWorkerError("delivery worker timing is invalid"));
        }
        let routes_match = input
            .inbox
            .lock()
            .map_err(|_defect| DeliveryWorkerError("delivery inbox lock is unavailable"))?
            .entries()
            .map_err(|_defect| DeliveryWorkerError("delivery inbox cannot be trusted"))?
            .iter()
            .all(|entry| entry.route == input.route_id);
        if !routes_match {
            return Err(DeliveryWorkerError(
                "delivery inbox belongs to another route",
            ));
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
        })
    }

    /// Processes at most one ready row without sleeping.
    ///
    /// # Errors
    ///
    /// Returns an error when trusted time, durable inbox ownership, or local
    /// controller state cannot be established.
    pub fn work_once(&mut self) -> Result<WorkOutcome, DeliveryWorkerError> {
        let now = self.now()?;
        let claim = self
            .inbox
            .lock()
            .map_err(|_defect| DeliveryWorkerError("delivery inbox lock is unavailable"))?
            .claim(now)
            .map_err(|_defect| DeliveryWorkerError("delivery inbox cannot be trusted"))?;
        match claim {
            ClaimOutcome::Claimed(claimed) => {
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
        while !stop.load(Ordering::Acquire) {
            match self.work_once()? {
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
            return Err(DeliveryWorkerError("claimed delivery names another route"));
        }
        if !self.reauthenticate(&claimed.delivery) {
            return self.complete(&claimed);
        }
        let result = self.handle(&mut claimed)?;
        match disposition(&result) {
            Disposition::Complete => self.complete(&claimed),
            Disposition::Retry(at) => self.retry(&claimed, at),
            Disposition::Backoff => {
                let now = self.now()?;
                let delay = retry_delay(claimed.lease.attempt, self.retry_min, self.retry_max);
                let at = add(now, delay)?;
                self.retry(&claimed, at)
            }
            Disposition::Fatal => Err(DeliveryWorkerError("controller state cannot be trusted")),
        }
    }

    fn reauthenticate(&self, delivery: &Delivery) -> bool {
        let request = AdmissionRequest {
            received_at_unix_millis: delivery.received_at_unix_millis,
            headers: &delivery.headers,
            body: &delivery.body,
        };
        self.admission.admit(request).is_ok_and(|admitted| {
            admitted.route == delivery.route && admitted.source_id == delivery.source_id
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
                .map_err(|_defect| DeliveryWorkerError("controller worker cannot start"))?;
            let controller_result = loop {
                let wait = renewal_wait(&lease, clock.as_ref())?;
                match receiver.recv_timeout(wait) {
                    Ok(result) => break Ok(result),
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break Err(DeliveryWorkerError(
                            "controller worker stopped without a result",
                        ));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let now = trusted_time(clock.as_ref())?;
                        let renewed = inbox
                            .lock()
                            .map_err(|_defect| {
                                DeliveryWorkerError("delivery inbox lock is unavailable")
                            })?
                            .renew(&lease, now)
                            .map_err(|_defect| {
                                DeliveryWorkerError("delivery inbox cannot be renewed")
                            })?;
                        match renewed {
                            RenewOutcome::Renewed(replacement) => lease = replacement,
                            RenewOutcome::Lost => {
                                break Err(DeliveryWorkerError("delivery inbox lease was lost"));
                            }
                        }
                    }
                }
            };
            let joined = worker.join();
            match (controller_result, joined) {
                (Ok(result), Ok(())) => Ok(result),
                (Err(error), Ok(())) => Err(error),
                (Ok(_) | Err(_), Err(_panic)) => {
                    Err(DeliveryWorkerError("controller worker panicked"))
                }
            }
        })?;
        claimed.lease = lease;
        Ok(result)
    }

    fn complete(&self, claimed: &ClaimedDelivery) -> Result<(), DeliveryWorkerError> {
        self.update_inbox(
            claimed,
            "delivery inbox cannot complete a row",
            |inbox, lease, now| {
                inbox
                    .complete(lease, now)
                    .map(|outcome| outcome == CompleteOutcome::Completed)
            },
        )
    }

    fn retry(
        &self,
        claimed: &ClaimedDelivery,
        requested_at: i64,
    ) -> Result<(), DeliveryWorkerError> {
        self.update_inbox(
            claimed,
            "delivery inbox cannot schedule a retry",
            |inbox, lease, now| {
                inbox
                    .retry(lease, now, requested_at.max(now))
                    .map(|outcome| outcome == RetryOutcome::Scheduled)
            },
        )
    }

    fn update_inbox(
        &self,
        claimed: &ClaimedDelivery,
        failure: &'static str,
        update: impl FnOnce(&mut Inbox, &DeliveryLease, i64) -> Result<bool, InboxError>,
    ) -> Result<(), DeliveryWorkerError> {
        let now = self.now()?;
        let mut inbox = self
            .inbox
            .lock()
            .map_err(|_defect| DeliveryWorkerError("delivery inbox lock is unavailable"))?;
        let owned = update(&mut inbox, &claimed.lease, now)
            .map_err(|_defect| DeliveryWorkerError(failure))?;
        owned
            .then_some(())
            .ok_or(DeliveryWorkerError("delivery inbox lease was lost"))
    }

    fn now(&self) -> Result<i64, DeliveryWorkerError> {
        trusted_time(self.clock.as_ref())
    }
}

enum Disposition {
    Complete,
    Retry(i64),
    Backoff,
    Fatal,
}

fn disposition<E>(result: &Result<HandleOutcome, ControllerError<E>>) -> Disposition {
    match result {
        Ok(HandleOutcome::InProgress {
            retry_at_unix_millis,
            ..
        }) => Disposition::Retry(*retry_at_unix_millis),
        Err(
            ControllerError::Provider(_)
            | ControllerError::Publish(_)
            | ControllerError::LeaseLost
            | ControllerError::CompletionLost,
        ) => Disposition::Backoff,
        Ok(HandleOutcome::Published(_) | HandleOutcome::Duplicate { .. })
        | Err(
            ControllerError::Ingress(_)
            | ControllerError::WrongChangeIdentity
            | ControllerError::WrongProviderRun
            | ControllerError::DeliveryBindingConflict,
        ) => Disposition::Complete,
        Err(
            ControllerError::UnknownProvider
            | ControllerError::Plan(_)
            | ControllerError::Ledger(_)
            | ControllerError::Completion(_),
        ) => Disposition::Fatal,
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
    let millis = i64::try_from(duration.as_millis())
        .map_err(|_defect| DeliveryWorkerError("retry time is too large"))?;
    now.checked_add(millis)
        .ok_or(DeliveryWorkerError("retry time is too large"))
}

fn trusted_time(clock: &dyn ControllerClock) -> Result<i64, DeliveryWorkerError> {
    clock
        .now_unix_millis()
        .filter(|now| *now >= 0)
        .ok_or(DeliveryWorkerError("controller time is unavailable"))
}
