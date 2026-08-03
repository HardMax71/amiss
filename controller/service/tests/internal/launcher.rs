#![cfg(test)]

use std::ffi::OsString;
use std::path::PathBuf;

use super::{config_path, version_request};

fn arguments(raw: &[&str]) -> Vec<OsString> {
    raw.iter().map(|value| OsString::from(*value)).collect()
}

/// The command line is one absolute path, optionally behind a check flag, and
/// nothing else.
#[test]
fn the_command_line_is_one_absolute_path() {
    assert_eq!(
        config_path(&arguments(&["/etc/amiss/service.json"])),
        Some((PathBuf::from("/etc/amiss/service.json"), false))
    );
    assert_eq!(
        config_path(&arguments(&["--check", "/etc/amiss/service.json"])),
        Some((PathBuf::from("/etc/amiss/service.json"), true))
    );

    for (reason, raw) in [
        ("nothing at all", vec![]),
        ("a relative path", vec!["service.json"]),
        ("a check flag with nothing after it", vec!["--check"]),
        (
            "a check flag over a relative path",
            vec!["--check", "service.json"],
        ),
        (
            "one argument too many",
            vec!["/etc/amiss/service.json", "extra"],
        ),
        (
            "one argument too many behind the flag",
            vec!["--check", "/etc/amiss/service.json", "extra"],
        ),
    ] {
        assert_eq!(config_path(&arguments(&raw)), None, "{reason}");
    }
}

/// The version request is that word alone, and nothing beside it.
#[test]
fn a_version_request_stands_alone() {
    assert!(version_request(&arguments(&["--version"])));
    for (reason, raw) in [
        ("nothing at all", vec![]),
        ("a word beside it", vec!["--version", "extra"]),
        ("another word", vec!["--help"]),
        ("a config path", vec!["/etc/amiss/service.json"]),
    ] {
        assert!(!version_request(&arguments(&raw)), "{reason}");
    }
}
