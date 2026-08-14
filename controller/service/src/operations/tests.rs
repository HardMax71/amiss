#![cfg(test)]

use std::io;

use axum::http::StatusCode;

use super::{EventLine, Operations, ServiceComponent, ServiceEvent, write_event_to};

#[test]
fn events_have_one_closed_redacted_shape() {
    let cases = [
        (
            ServiceEvent::Ready,
            r#"{"schema":"amiss/controller-event/v1","level":"info","event":"ready","component":"service"}"#,
        ),
        (
            ServiceEvent::Draining,
            r#"{"schema":"amiss/controller-event/v1","level":"info","event":"draining","component":"service"}"#,
        ),
        (
            ServiceEvent::Stopped,
            r#"{"schema":"amiss/controller-event/v1","level":"info","event":"stopped","component":"service"}"#,
        ),
        (
            ServiceEvent::Failed(ServiceComponent::Worker),
            r#"{"schema":"amiss/controller-event/v1","level":"error","event":"failed","component":"worker"}"#,
        ),
        (
            ServiceEvent::Failed(ServiceComponent::Maintenance),
            r#"{"schema":"amiss/controller-event/v1","level":"error","event":"failed","component":"maintenance"}"#,
        ),
    ];
    for (event, expected) in cases {
        assert_eq!(
            serde_json::to_string(&EventLine::from(event)).unwrap(),
            expected
        );
    }
}

#[test]
fn metrics_are_fixed_and_label_free() {
    let operations = Operations::default();
    operations.record_response(StatusCode::ACCEPTED);
    operations.record_response(StatusCode::UNAUTHORIZED);
    operations.record_response(StatusCode::SERVICE_UNAVAILABLE);
    let mut encoded = String::new();
    operations.encode(&mut encoded).unwrap();
    let samples = encoded
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>();

    assert_eq!(samples.len(), 14);
    assert!(samples.iter().all(|line| !line.contains('{')));
    assert!(samples.contains(&"amiss_controller_provider_requests_total 3"));
    assert!(samples.contains(&"amiss_controller_external_refuted_total 0"));
    assert!(encoded.ends_with("# EOF\n"));
}

#[test]
fn event_writer_reports_output_failure() {
    struct FailedOutput;

    impl io::Write for FailedOutput {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    assert!(write_event_to(&mut FailedOutput, ServiceEvent::Ready).is_err());
}
