#![cfg(test)]

use std::fs;

use super::{CheckPlanFiles, ExternalPolicy, load_plan};

#[test]
fn policy_files_decode_at_ingress_and_do_not_follow_later_file_changes() {
    let directory = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        (
            "constraint",
            include_bytes!("../../../../../spec/examples/scanner-execution-constraint.json")
                .as_slice(),
        ),
        (
            "floor",
            include_bytes!("../../../../../spec/examples/organization-floor.json").as_slice(),
        ),
        (
            "debt",
            include_bytes!("../../../../../spec/examples/debt-snapshot.json").as_slice(),
        ),
        (
            "waiver",
            include_bytes!("../../../../../spec/examples/waiver-bundle.json").as_slice(),
        ),
    ] {
        fs::write(directory.path().join(name), bytes).unwrap();
    }
    let files = CheckPlanFiles {
        profile: "enforce".to_owned(),
        external_policy: ExternalPolicy::Advisory,
        execution_constraint_file: directory.path().join("constraint"),
        organization_floor_file: Some(directory.path().join("floor")),
        debt_snapshot_file: Some(directory.path().join("debt")),
        waiver_bundle_file: Some(directory.path().join("waiver")),
        intersphinx_inventories: Vec::new(),
        workflow_artifacts: Vec::new(),
    };
    let plan = load_plan(&files, None).unwrap();
    let binding = amiss_controller::check_binding(&plan).unwrap();
    assert_eq!(
        plan.policy.organization_floor.as_ref().unwrap().value,
        super::parse_organization_floor(&fs::read(directory.path().join("floor")).unwrap())
            .unwrap(),
    );
    assert_eq!(
        plan.policy.debt_snapshot.as_ref().unwrap().value,
        super::parse_debt_snapshot(&fs::read(directory.path().join("debt")).unwrap()).unwrap(),
    );
    assert_eq!(
        plan.policy.waiver_bundle.as_ref().unwrap().value,
        super::parse_waiver_bundle(&fs::read(directory.path().join("waiver")).unwrap()).unwrap(),
    );

    for name in ["floor", "debt", "waiver"] {
        let path = directory.path().join(name);
        let original = fs::read_to_string(&path).unwrap();
        for invalid in [
            "null".to_owned(),
            "[]".to_owned(),
            "{}".to_owned(),
            original.replacen('{', "{\"unexpected\":false,", 1),
            original.replacen('{', "{\"schema\":\"amiss/unknown\",", 1),
        ] {
            fs::write(&path, invalid).unwrap();
            assert!(load_plan(&files, None).is_err(), "{name}");
            assert_eq!(amiss_controller::check_binding(&plan).unwrap(), binding);
        }
        fs::write(path, original).unwrap();
    }
}
