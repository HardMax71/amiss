use amiss_wire::controls::{
    EligibleFindingKind, FloorDefect, ORGANIZATION_POLICY_ENTRIES_LIMIT, OrganizationFloorSchema,
    ResourceName, canonical_organization_floor, canonical_scanner_policy, parse_organization_floor,
    parse_scanner_policy,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hj;
use amiss_wire::json;
use amiss_wire::model::BranchRef;

use crate::support::{FLOOR, POLICY};

#[test]
fn a_floor_may_require_warn_where_the_fixture_requires_fail() {
    let doc = String::from_utf8(FLOOR.to_vec())
        .unwrap()
        .replace(r#""disposition": "fail""#, r#""disposition": "warn""#);
    let floor =
        parse_organization_floor(doc.as_bytes()).expect("warn is a disposition a floor may set");
    assert_ne!(
        canonical_organization_floor(&floor).unwrap().1,
        canonical_organization_floor(&parse_organization_floor(FLOOR).unwrap())
            .unwrap()
            .1
    );
}

#[test]
fn parses_the_floor_fixture() {
    let floor = parse_organization_floor(FLOOR).unwrap();
    assert_eq!(floor.schema, OrganizationFloorSchema::Current);
    assert_eq!(
        canonical_organization_floor(&floor).unwrap().1,
        hj("amiss/organization-floor", &json::parse(FLOOR).unwrap())
    );
    assert_eq!(floor.floor_id.as_str(), "platform/scanner-floor-2026-07");
    assert_eq!(floor.ref_name.as_str(), "refs/heads/main");
    assert_eq!(floor.resource_limits.len(), 2);
    let owners: Vec<&str> = floor
        .authorized_debt_owners
        .iter()
        .map(amiss_wire::model::OwnerId::as_str)
        .collect();
    assert_eq!(owners, ["team:docs-platform"]);
    let issuers: Vec<&str> = floor
        .authorized_waiver_issuers
        .iter()
        .map(amiss_wire::model::OwnerId::as_str)
        .collect();
    assert_eq!(issuers, ["team:release-engineering"]);
    let waivable: Vec<&str> = floor
        .waivable_finding_kinds
        .iter()
        .map(AsRef::as_ref)
        .collect();
    let expected: &'static str = EligibleFindingKind::ExplicitTargetMissing.into();
    assert_eq!(waivable, [expected]);
    assert_eq!(expected, "explicit-target-missing");
    assert_ne!(
        canonical_organization_floor(&floor).unwrap().1,
        canonical_scanner_policy(&parse_scanner_policy(POLICY).unwrap())
            .unwrap()
            .1
    );
}

#[test]
fn parses_a_floor_declaring_every_resource() {
    let mut declared: Vec<(&'static str, i64)> = ResourceName::all()
        .map(|resource| {
            let maximum = if resource == ResourceName::MachineJsonBytes {
                268_435_456
            } else if resource == ResourceName::TypedAnalysisErrorsRetained {
                64
            } else {
                i64::try_from(ORGANIZATION_POLICY_ENTRIES_LIMIT).unwrap_or(i64::MAX)
            };
            (resource.as_str(), maximum)
        })
        .collect();
    declared.sort_unstable();
    let rows: Vec<String> = declared
        .iter()
        .map(|(resource, maximum)| {
            format!("{{ \"resource\": \"{resource}\", \"maximum\": {maximum} }}")
        })
        .collect();
    let doc = format!(
        r#"{{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/every-resource",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": [],
  "protected_control_paths": [],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": [{rows}]
}}"#,
        rows = rows.join(",")
    );
    let floor = parse_organization_floor(doc.as_bytes()).unwrap();
    assert_eq!(floor.resource_limits.len(), ResourceName::all().len());
}

#[expect(clippy::panic, reason = "test helper narrowing the defect family")]
fn floor_schema_kind(defect: FloorDefect) -> ErrorKind {
    match defect {
        FloorDefect::Schema(error) => error.kind,
        FloorDefect::Entries { .. } => panic!("expected a schema defect, got an entries crossing"),
    }
}

#[test]
fn rejects_floor_bound_defects() {
    let doc = String::from_utf8(FLOOR.to_vec()).unwrap();
    let wrong_ceiling = doc.replace("268435456", "268435455");
    assert_eq!(
        floor_schema_kind(parse_organization_floor(wrong_ceiling.as_bytes()).unwrap_err()),
        ErrorKind::InvalidValue
    );

    let wrong_errors = doc.replace("\"maximum\": 64", "\"maximum\": 65");
    assert_eq!(
        floor_schema_kind(parse_organization_floor(wrong_errors.as_bytes()).unwrap_err()),
        ErrorKind::InvalidValue
    );

    let unsorted_limits = doc.replace(
        "{ \"resource\": \"machine-json-bytes\", \"maximum\": 268435456 },\n    { \"resource\": \"typed-analysis-errors-retained\", \"maximum\": 64 }",
        "{ \"resource\": \"typed-analysis-errors-retained\", \"maximum\": 64 },\n    { \"resource\": \"machine-json-bytes\", \"maximum\": 268435456 }",
    );
    assert_eq!(
        floor_schema_kind(parse_organization_floor(unsorted_limits.as_bytes()).unwrap_err()),
        ErrorKind::UnsortedSet
    );
}

#[test]
fn canonical_floor_rechecks_mutable_public_fields() {
    let mut floor = parse_organization_floor(FLOOR).unwrap();
    floor
        .resource_limits
        .first_mut()
        .expect("the fixture has resource limits")
        .maximum = -1;
    assert_eq!(
        canonical_organization_floor(&floor).unwrap_err(),
        FloorDefect::Schema(amiss_wire::de::Error::new(
            "$.resource_limits[0].maximum",
            ErrorKind::InvalidValue,
        ))
    );
}

#[test]
fn rejects_floors_over_the_combined_entry_limit() {
    let paths = |count: usize, prefix: &str| {
        let items: Vec<String> = (0..count)
            .map(|index| format!("\"{prefix}/{index:07}.md\""))
            .collect();
        items.join(",")
    };
    let doc = format!(
        r#"{{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/too-big",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": [{inventory}],
  "protected_control_paths": [{controls}],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": []
}}"#,
        inventory = paths(60_000, "docs/a"),
        controls = paths(45_000, "ops/b"),
    );
    assert_eq!(
        parse_organization_floor(doc.as_bytes()).unwrap_err(),
        FloorDefect::Entries {
            configured_limit: ORGANIZATION_POLICY_ENTRIES_LIMIT,
            observed_lower_bound: ORGANIZATION_POLICY_ENTRIES_LIMIT + 1,
        }
    );
}

#[test]
fn accepts_a_floor_at_exactly_the_combined_entry_limit() {
    let paths = |count: usize, prefix: &str| {
        let items: Vec<String> = (0..count)
            .map(|index| format!("\"{prefix}/{index:07}.md\""))
            .collect();
        items.join(",")
    };
    let doc = format!(
        r#"{{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/at-the-brim",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": [{inventory}],
  "protected_control_paths": [{controls}],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": []
}}"#,
        inventory = paths(60_000, "docs/a"),
        controls = paths(40_000, "ops/b"),
    );
    let floor = parse_organization_floor(doc.as_bytes())
        .expect("a floor whose entries sum to the limit exactly is within it");
    assert_eq!(
        u64::try_from(floor.protected_inventory.len() + floor.protected_control_paths.len())
            .expect("entry counts fit"),
        ORGANIZATION_POLICY_ENTRIES_LIMIT
    );
}

#[test]
fn accepts_a_floor_meeting_its_own_declared_entry_limit_exactly() {
    let doc = br#"{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/self-consistent",
  "repository": { "host": "github.com", "owner": "acme", "name": "docs" },
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": ["docs/a.md", "docs/b.md", "docs/c.md"],
  "protected_control_paths": [],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": [
    { "resource": "organization-policy-entries", "maximum": 4 }
  ]
}"#;
    let floor = parse_organization_floor(doc)
        .expect("three paths and one limit row meet a declared maximum of four exactly");
    assert_eq!(floor.protected_inventory.len(), 3);
}

#[test]
fn rejects_floors_inconsistent_with_their_own_declared_entry_limit() {
    let doc = br#"{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/self-inconsistent",
  "repository": { "host": "github.com", "owner": "acme", "name": "docs" },
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": ["docs/a.md", "docs/b.md", "docs/c.md"],
  "protected_control_paths": [],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": [
    { "resource": "organization-policy-entries", "maximum": 3 }
  ]
}"#;
    assert_eq!(
        parse_organization_floor(doc).unwrap_err(),
        FloorDefect::Entries {
            configured_limit: 3,
            observed_lower_bound: 4,
        }
    );
}

#[test]
fn branch_refs_follow_ref_format() {
    let valid = [
        "refs/heads/main",
        "refs/heads/feature/a+b",
        "refs/heads/\u{e9}",
        "refs/heads/@",
        "refs/heads/-dash",
    ];
    for case in valid {
        assert!(BranchRef::new(case.to_owned()).is_some(), "{case}");
    }
    let invalid = [
        "refs/heads/".to_owned(),
        "refs/heads//main".to_owned(),
        "refs/heads/.hidden".to_owned(),
        "refs/heads/main.lock".to_owned(),
        "refs/heads/a..b".to_owned(),
        "refs/heads/a b".to_owned(),
        "refs/heads/a~b".to_owned(),
        "refs/heads/a?b".to_owned(),
        "refs/heads/a[b".to_owned(),
        "refs/heads/a\\b".to_owned(),
        "refs/heads/a@{b".to_owned(),
        "refs/heads/a.".to_owned(),
        format!("refs/heads/{}", "a".repeat(256)),
    ];
    for case in invalid {
        assert!(BranchRef::new(case.clone()).is_none(), "{case}");
    }
}
