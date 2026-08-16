use amiss_wire::controls::{STATEMENT_TTL_MAX_SECONDS, TrustedTimeStatement};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hj;
use amiss_wire::json;
use amiss_wire::model::UtcInstant;

use crate::support::TIME_STATEMENT;

#[test]
fn a_run_id_answers_to_every_clause_that_bounds_it() {
    let with_id = |id: &str| {
        TrustedTimeStatement::parse(TIME_STATEMENT.replace("pipeline/01J2Z9-7", id).as_bytes())
    };
    assert!(with_id(&"a".repeat(129)).is_err(), "over the length bound");
    assert!(with_id(&"a".repeat(128)).is_ok(), "at the length bound");
    assert!(with_id("").is_err(), "empty");
    assert!(with_id("-lead").is_err(), "must open alphanumeric");
    assert!(with_id("trail-").is_err(), "must close alphanumeric");
    assert!(with_id("has space").is_err(), "space is not allowed");
}

#[test]
fn instants_are_strictly_gregorian() {
    for valid in [
        "2026-02-28T23:59:59Z",
        "2024-02-29T00:00:00Z",
        "2000-02-29T12:00:00Z",
        "0001-01-01T00:00:00Z",
    ] {
        assert!(UtcInstant::new(valid.to_owned()).is_some(), "{valid}");
    }
    for invalid in [
        "2026-02-29T00:00:00Z",
        "1900-02-29T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-13-01T00:00:00Z",
        "2026-00-10T00:00:00Z",
        "2026-07-00T00:00:00Z",
        "2026-07-12T24:00:00Z",
        "2026-07-12T00:00:60Z",
        "2026-07-12T00:00:00",
        "2026-7-12T00:00:00Z",
    ] {
        assert!(UtcInstant::new(invalid.to_owned()).is_none(), "{invalid}");
    }
}

#[test]
fn instants_round_trip_unix_seconds() {
    for value in [
        "0000-01-01T00:00:00Z",
        "1969-12-31T23:59:59Z",
        "1970-01-01T00:00:00Z",
        "2000-02-29T12:34:56Z",
        "2026-07-22T10:00:00Z",
        "9999-12-31T23:59:59Z",
    ] {
        let instant = UtcInstant::new(value.to_owned()).unwrap();
        assert_eq!(
            UtcInstant::from_epoch_seconds(instant.epoch_seconds()),
            Some(instant)
        );
    }
    assert!(UtcInstant::from_epoch_seconds(-62_167_219_201).is_none());
    assert!(UtcInstant::from_epoch_seconds(253_402_300_800).is_none());
}

#[test]
fn parses_a_trusted_time_statement_and_enforces_the_ttl() {
    assert_eq!(STATEMENT_TTL_MAX_SECONDS, 600);
    let statement = TrustedTimeStatement::parse(TIME_STATEMENT.as_bytes()).unwrap();
    assert_eq!(statement.schema(), "amiss/scanner-trusted-time-statement");
    assert_eq!(statement.controller(), "external-required-check-clock");
    assert_eq!(statement.provider(), "gitlab-ci");
    assert_eq!(
        statement.digest(),
        hj(
            "amiss/scanner-trusted-time-statement",
            &json::parse(TIME_STATEMENT.as_bytes()).unwrap()
        )
    );
    assert_eq!(statement.provider_run_id(), "pipeline/01J2Z9-7");
    assert_eq!(statement.provider_run_attempt(), 2);
    assert_eq!(
        statement.evaluation_instant().as_str(),
        "2026-07-12T10:00:00Z"
    );
    assert_eq!(
        statement.valid_until().epoch_seconds() - statement.evaluation_instant().epoch_seconds(),
        600
    );

    let too_long = TIME_STATEMENT.replace("10:10:00Z", "10:10:01Z");
    assert_eq!(
        TrustedTimeStatement::parse(too_long.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let not_after = TIME_STATEMENT.replace("10:10:00Z", "10:00:00Z");
    assert_eq!(
        TrustedTimeStatement::parse(not_after.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let trailing_separator = TIME_STATEMENT.replace("pipeline/01J2Z9-7", "pipeline/");
    assert_eq!(
        TrustedTimeStatement::parse(trailing_separator.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let uppercase_provider = TIME_STATEMENT.replace("gitlab-ci", "GitLab-CI");
    assert_eq!(
        TrustedTimeStatement::parse(uppercase_provider.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let numeric_run = TIME_STATEMENT.replace("pipeline/01J2Z9-7", "987654321");
    assert!(TrustedTimeStatement::parse(numeric_run.as_bytes()).is_ok());
}
