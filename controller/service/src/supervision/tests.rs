#![cfg(test)]

use std::future::Future;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use tokio::sync::Notify;

use super::{RuntimeState, SupervisionError, coordinate};
use crate::probe::work_permits;
use crate::{Operations, ServiceComponent, ServiceEvent};

async fn supervision<H>(
    server: impl FnOnce(Arc<Notify>) -> H,
    component: impl Future<Output = Result<(), SupervisionError>>,
    shutdown: impl Future<Output = io::Result<()>>,
    stop: impl FnOnce() -> Result<(), SupervisionError> + Send + 'static,
) -> (Result<(), SupervisionError>, Vec<(ServiceEvent, bool)>)
where
    H: Future<Output = io::Result<()>>,
{
    let (_permits, endpoint) = work_permits(1).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&events);
    let ready = Arc::new(AtomicBool::new(false));
    let observed_ready = Arc::clone(&ready);
    let operations = Operations::with_event_sink(move |event| {
        observed
            .lock()
            .unwrap()
            .push((event, observed_ready.load(Ordering::Acquire)));
    });
    let server_stop = Arc::new(Notify::new());
    let observed_stop = Arc::clone(&server_stop);
    let server = server(observed_stop);
    let result = coordinate(
        RuntimeState {
            ready,
            operations,
            endpoint,
            component: ServiceComponent::Worker,
        },
        server,
        move || server_stop.notify_one(),
        component,
        shutdown,
        stop,
    )
    .await;
    let events = events.lock().unwrap().clone();
    (result, events)
}

async fn stopped_server(stop: Arc<Notify>) -> io::Result<()> {
    stop.notified().await;
    Ok(())
}

#[tokio::test]
async fn signal_drains_in_order() {
    let stop = Arc::new(Notify::new());
    let observed = Arc::clone(&stop);
    let (result, events) = supervision(
        stopped_server,
        async move {
            observed.notified().await;
            Ok(())
        },
        async {
            tokio::task::yield_now().await;
            Ok(())
        },
        move || {
            stop.notify_one();
            Ok(())
        },
    )
    .await;

    assert_eq!(result, Ok(()));
    assert_eq!(
        events,
        [
            (ServiceEvent::Ready, true),
            (ServiceEvent::Draining, false),
            (ServiceEvent::Stopped, false),
        ]
    );
}

#[tokio::test]
async fn pending_shutdown_skips_readiness() {
    let stop = Arc::new(Notify::new());
    let observed = Arc::clone(&stop);
    let (result, events) = supervision(
        stopped_server,
        async move {
            observed.notified().await;
            Ok(())
        },
        async { Ok(()) },
        move || {
            stop.notify_one();
            Ok(())
        },
    )
    .await;

    assert_eq!(result, Ok(()));
    assert_eq!(
        events,
        [
            (ServiceEvent::Draining, false),
            (ServiceEvent::Stopped, false),
        ]
    );
}

#[tokio::test]
async fn shutdown_handler_failure_is_not_a_signal() {
    let stop = Arc::new(Notify::new());
    let observed = Arc::clone(&stop);
    let (result, events) = supervision(
        stopped_server,
        async move {
            observed.notified().await;
            Ok(())
        },
        async { Err(io::Error::other("handler stopped")) },
        move || {
            stop.notify_one();
            Ok(())
        },
    )
    .await;

    assert_eq!(
        result,
        Err(SupervisionError("shutdown signal handler stopped"))
    );
    assert_eq!(
        events,
        [
            (ServiceEvent::Draining, false),
            (ServiceEvent::Stopped, false),
        ]
    );
}

#[tokio::test]
async fn failure_while_draining_is_reported() {
    let stop = Arc::new(Notify::new());
    let observed = Arc::clone(&stop);
    let (result, events) = supervision(
        stopped_server,
        async move {
            observed.notified().await;
            Err(SupervisionError("component failed"))
        },
        async {
            tokio::task::yield_now().await;
            Ok(())
        },
        move || {
            stop.notify_one();
            Ok(())
        },
    )
    .await;

    assert_eq!(result, Err(SupervisionError("component failed")));
    assert_eq!(
        events,
        [
            (ServiceEvent::Ready, true),
            (ServiceEvent::Draining, false),
            (ServiceEvent::Failed(ServiceComponent::Worker), false),
            (ServiceEvent::Stopped, false),
        ]
    );
}

#[tokio::test]
async fn server_failure_starts_drain_before_or_after_readiness() {
    for delayed in [false, true] {
        let stop = Arc::new(Notify::new());
        let observed = Arc::clone(&stop);
        let service = supervision(
            move |_stop| async move {
                if delayed {
                    tokio::task::yield_now().await;
                }
                Err(io::Error::other("server failed"))
            },
            async move {
                observed.notified().await;
                Ok(())
            },
            std::future::pending(),
            move || {
                stop.notify_one();
                Ok(())
            },
        );
        let (result, events) = tokio::time::timeout(Duration::from_secs(1), service)
            .await
            .unwrap();
        let expected = delayed
            .then_some((ServiceEvent::Ready, true))
            .into_iter()
            .chain([
                (ServiceEvent::Draining, false),
                (ServiceEvent::Stopped, false),
            ])
            .collect::<Vec<_>>();

        assert_eq!(result, Err(SupervisionError("HTTP service stopped")));
        assert_eq!(events, expected);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn draining_is_visible_while_component_stop_is_blocked() {
    let (_permits, endpoint) = work_permits(1).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&events);
    let ready = Arc::new(AtomicBool::new(false));
    let observed_ready = Arc::clone(&ready);
    let operations = Operations::with_event_sink(move |event| {
        observed
            .lock()
            .unwrap()
            .push((event, observed_ready.load(Ordering::Acquire)));
    });
    let server_stop = Arc::new(Notify::new());
    let observed_server_stop = Arc::clone(&server_stop);
    let component_stop = Arc::new(Notify::new());
    let observed_component_stop = Arc::clone(&component_stop);
    let stop_started = Arc::new(Notify::new());
    let observed_stop_started = Arc::clone(&stop_started);
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let observed_release = Arc::clone(&release);
    let service = tokio::spawn(coordinate(
        RuntimeState {
            ready: Arc::clone(&ready),
            operations,
            endpoint,
            component: ServiceComponent::Worker,
        },
        async move {
            observed_server_stop.notified().await;
            Ok(())
        },
        move || server_stop.notify_one(),
        async move {
            observed_component_stop.notified().await;
            Ok(())
        },
        async {
            tokio::task::yield_now().await;
            Ok(())
        },
        move || {
            observed_stop_started.notify_one();
            let (lock, released) = &*observed_release;
            let mut done = lock.lock().unwrap();
            while !*done {
                done = released.wait(done).unwrap();
            }
            component_stop.notify_one();
            Ok(())
        },
    ));

    stop_started.notified().await;
    assert!(!ready.load(Ordering::Acquire));
    assert_eq!(
        *events.lock().unwrap(),
        [(ServiceEvent::Ready, true), (ServiceEvent::Draining, false),]
    );

    let (lock, released) = &*release;
    *lock.lock().unwrap() = true;
    released.notify_one();
    assert_eq!(service.await.unwrap(), Ok(()));
    assert_eq!(
        *events.lock().unwrap(),
        [
            (ServiceEvent::Ready, true),
            (ServiceEvent::Draining, false),
            (ServiceEvent::Stopped, false),
        ]
    );
}
