#![cfg(unix)]

use std::process::Command;
use std::time::Duration;

use amiss_wrapper::{Supervised, supervise};

#[test]
fn the_watchdog_kills_a_hung_evaluator() {
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    let outcome = supervise(&mut child, Duration::from_millis(200)).unwrap();
    assert!(matches!(outcome, Supervised::Killed), "{outcome:?}");
}

#[test]
fn a_prompt_evaluator_completes_with_its_status() {
    let mut child = Command::new("true").spawn().unwrap();
    let outcome = supervise(&mut child, Duration::from_secs(30)).unwrap();
    let Supervised::Completed(status) = outcome else {
        panic!("completed: {outcome:?}");
    };
    assert!(status.success());
}
