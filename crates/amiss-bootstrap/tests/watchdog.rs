use std::process::{Child, Command, Stdio};
use std::time::Duration;

use amiss_bootstrap::supervise::{Supervised, supervise};

/// A child that never finishes on its own. It is this same test binary,
/// re-entered on the ignored test at the bottom of the file, and it outlives
/// any watchdog the suite sets. Cargo builds that executable for every
/// platform, where a POSIX `sleep` exists on only some of them, so the
/// watchdog is proven wherever the bootstrap ships rather than on unix alone.
#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn hung_child() -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args(["outlives_any_watchdog", "--exact", "--ignored"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn the_watchdog_kills_a_hung_engine() {
    let mut child = hung_child();
    let outcome = supervise(&mut child, Duration::from_millis(200)).unwrap();
    assert!(matches!(outcome, Supervised::Killed), "{outcome:?}");
}

/// `amiss-manifest` refuses its own invalid invocation and exits at once,
/// which is a prompt child on every platform, where `true` is one on only
/// some.
#[test]
fn a_prompt_engine_completes_with_its_status() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_amiss-manifest"))
        .arg("--not-a-flag")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let outcome = supervise(&mut child, Duration::from_secs(30)).unwrap();
    let Supervised::Completed(status) = outcome else {
        panic!("completed: {outcome:?}");
    };
    assert!(
        !status.success(),
        "the child's own status is reported, not a synthetic one"
    );
}

#[test]
#[ignore = "the hung child the watchdog kills, never run on its own"]
fn outlives_any_watchdog() {
    std::thread::sleep(Duration::from_mins(10));
}
