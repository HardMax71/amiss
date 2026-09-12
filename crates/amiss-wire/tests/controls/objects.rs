use amiss_wire::{controls, manifest};
use serde_json::Value;

struct Case {
    name: &'static str,
    source: &'static [u8],
    pointer: &'static str,
    order: &'static str,
    accepts: fn(Value) -> bool,
}

const FLOOR: &[u8] = include_bytes!("../../../../spec/examples/organization-floor.json");
const POLICY: &[u8] = include_bytes!("../../../../spec/examples/scanner-policy.json");
const DEBT: &[u8] = include_bytes!("../../../../spec/examples/debt-snapshot.json");
const WAIVER: &[u8] = include_bytes!("../../../../spec/examples/waiver-bundle.json");
const TIME: &[u8] = include_bytes!("../../../../spec/examples/scanner-trusted-time-statement.json");
const CONSTRAINT: &[u8] =
    include_bytes!("../../../../spec/examples/scanner-execution-constraint.json");
const MANIFEST: &[u8] = include_bytes!("../../../../spec/examples/scanner-release-manifest.json");

const CASES: &[Case] = &[
    Case {
        name: "OrganizationFloor",
        source: FLOOR,
        pointer: "",
        order: "schema floor_id repository ref minimum_profile minimum_dispositions protected_inventory protected_control_paths waivable_finding_kinds authorized_debt_owners authorized_waiver_issuers resource_limits",
        accepts: |value| serde_json::from_value::<controls::OrganizationFloor>(value).is_ok(),
    },
    Case {
        name: "ResourceLimit",
        source: FLOOR,
        pointer: "/resource_limits/0",
        order: "resource maximum",
        accepts: |value| serde_json::from_value::<controls::ResourceLimit>(value).is_ok(),
    },
    Case {
        name: "FindingDisposition",
        source: FLOOR,
        pointer: "/minimum_dispositions/0",
        order: "finding_kind disposition",
        accepts: |value| serde_json::from_value::<controls::FindingDisposition>(value).is_ok(),
    },
    Case {
        name: "ScannerPolicy",
        source: POLICY,
        pointer: "",
        order: "schema document_includes projection_assertions protected_inventory finding_dispositions",
        accepts: |value| serde_json::from_value::<controls::ScannerPolicy>(value).is_ok(),
    },
    Case {
        name: "ProjectionAssertion",
        source: POLICY,
        pointer: "/projection_assertions/0",
        order: "document name projection sink source",
        accepts: |value| serde_json::from_value::<controls::ProjectionAssertion>(value).is_ok(),
    },
    Case {
        name: "ProjectionSource",
        source: POLICY,
        pointer: "/projection_assertions/0/source",
        order: "kind path start_marker end_marker",
        accepts: |value| serde_json::from_value::<controls::ProjectionSource>(value).is_ok(),
    },
    Case {
        name: "DebtSnapshot",
        source: DEBT,
        pointer: "",
        order: "schema repository ref organization_floor_digest adoption_tree adoption_report_payload_digest created_at items",
        accepts: |value| serde_json::from_value::<controls::DebtSnapshot>(value).is_ok(),
    },
    Case {
        name: "DebtItem",
        source: DEBT,
        pointer: "/items/0",
        order: "debt_id finding_key accepted_fact accepted_fact_digest owner reason created_at expires_at",
        accepts: |value| serde_json::from_value::<controls::DebtItem>(value).is_ok(),
    },
    Case {
        name: "Fact",
        source: DEBT,
        pointer: "/items/0/accepted_fact",
        order: "evidence finding_kind key_input schema",
        accepts: |value| serde_json::from_value::<controls::Fact>(value).is_ok(),
    },
    Case {
        name: "FactEvidence",
        source: DEBT,
        pointer: "/items/0/accepted_fact/evidence",
        order: "kind resolution occurrence_multiplicity",
        accepts: |value| serde_json::from_value::<controls::FactEvidence>(value).is_ok(),
    },
    Case {
        name: "StructuralResolution",
        source: DEBT,
        pointer: "/items/0/accepted_fact/evidence/resolution",
        order: "kind reason near path",
        accepts: |value| serde_json::from_value::<controls::StructuralResolution>(value).is_ok(),
    },
    Case {
        name: "FindingKeyInput",
        source: DEBT,
        pointer: "/items/0/accepted_fact/key_input",
        order: "finding_kind schema scope",
        accepts: |value| serde_json::from_value::<controls::FindingKeyInput>(value).is_ok(),
    },
    Case {
        name: "FindingScope",
        source: DEBT,
        pointer: "/items/0/accepted_fact/key_input/scope",
        order: "document kind normalized_target_intent occurrence source_construct",
        accepts: |value| serde_json::from_value::<controls::FindingScope>(value).is_ok(),
    },
    Case {
        name: "FindingOccurrence",
        source: DEBT,
        pointer: "/items/0/accepted_fact/key_input/scope/occurrence",
        order: "kind source_projection_digest",
        accepts: |value| serde_json::from_value::<controls::FindingOccurrence>(value).is_ok(),
    },
    Case {
        name: "WaiverBundle",
        source: WAIVER,
        pointer: "",
        order: "schema repository ref organization_floor_digest created_at items",
        accepts: |value| serde_json::from_value::<controls::WaiverBundle>(value).is_ok(),
    },
    Case {
        name: "WaiverItem",
        source: WAIVER,
        pointer: "/items/0",
        order: "waiver_id finding_key authorized_fact authorized_fact_digest candidate_tree owner issuer reason created_at not_before expires_at residual_disposition",
        accepts: |value| serde_json::from_value::<controls::WaiverItem>(value).is_ok(),
    },
    Case {
        name: "TrustedTimeStatement",
        source: TIME,
        pointer: "",
        order: "candidate_identity_digest controller evaluation_instant provider provider_run_attempt provider_run_id ref repository schema valid_until",
        accepts: |value| serde_json::from_value::<controls::TrustedTimeStatement>(value).is_ok(),
    },
    Case {
        name: "ExecutionConstraintDescriptor",
        source: CONSTRAINT,
        pointer: "",
        order: "action_commit_oid action_object_format action_repository action_tree_oid bootstrap_contract bootstrap_digest manifest_path release_manifest_digest required_status_name schema selected_platform",
        accepts: |value| {
            serde_json::from_value::<controls::ExecutionConstraintDescriptor>(value).is_ok()
        },
    },
    Case {
        name: "ReleaseManifest",
        source: MANIFEST,
        pointer: "",
        order: "artifacts build_source dependency_lock dependency_lock_digest engine_version schema",
        accepts: |value| serde_json::from_value::<manifest::ReleaseManifest>(value).is_ok(),
    },
    Case {
        name: "ReleaseArtifact",
        source: MANIFEST,
        pointer: "/artifacts/0",
        order: "artifact_name binary_sha256 engine_digest environment_contract platform runtime_contract runtime_files tree_path",
        accepts: |value| serde_json::from_value::<manifest::ReleaseArtifact>(value).is_ok(),
    },
    Case {
        name: "RuntimeFile",
        source: MANIFEST,
        pointer: "/artifacts/0/runtime_files/0",
        order: "file_sha256 git_mode path role",
        accepts: |value| serde_json::from_value::<manifest::RuntimeFile>(value).is_ok(),
    },
    Case {
        name: "BuildSource",
        source: MANIFEST,
        pointer: "/build_source",
        order: "commit_oid object_format repository",
        accepts: |value| serde_json::from_value::<manifest::BuildSource>(value).is_ok(),
    },
    Case {
        name: "DependencyLockInput",
        source: MANIFEST,
        pointer: "/dependency_lock",
        order: "files schema",
        accepts: |value| serde_json::from_value::<manifest::DependencyLockInput>(value).is_ok(),
    },
    Case {
        name: "DependencyLockFile",
        source: MANIFEST,
        pointer: "/dependency_lock/files/0",
        order: "path raw_digest",
        accepts: |value| serde_json::from_value::<manifest::DependencyLockFile>(value).is_ok(),
    },
];

#[test]
fn control_and_manifest_objects_reject_positional_arrays() {
    for case in CASES {
        let document: Value = serde_json::from_slice(case.source).unwrap();
        let original = document.pointer(case.pointer).unwrap();
        assert!((case.accepts)(original.clone()), "{} object", case.name);
        let positional: Vec<Value> = case
            .order
            .split_whitespace()
            .map(|field| original.get(field).unwrap().clone())
            .collect();
        assert!(
            !(case.accepts)(Value::Array(positional)),
            "{} array",
            case.name
        );
    }
}
