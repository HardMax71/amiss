use amiss_controller::{
    BootstrapJobError, OpaqueId, PolicyControls, ProviderInstance, ProviderNamespace,
    SemanticEvidenceTemplate, WorkflowArtifactExpectation, check_binding, check_plan,
};
use amiss_wire::controls::{
    Profile, canonical_debt_snapshot, canonical_organization_floor, canonical_waiver_bundle,
};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::{ArtifactId, RepoPathText, RepositoryIdentity};
use amiss_wire::requests::RequestTrust;

use super::{execution, site_acquisition, workflow_acquisition};

fn full_policy() -> PolicyControls {
    PolicyControls {
        semantic_evidence: vec![
            amiss_wire::semantic::parse_template(&super::semantic_template(hb(
                "amiss/test-site-context",
                b"direct",
            )))
            .unwrap(),
        ],
        semantic_acquisitions: vec![
            site_acquisition(hb("amiss/test-site-context", b"acquired")).expectation,
        ],
        workflow_artifacts: vec![
            workflow_acquisition(
                "workflow-site",
                "site \"β\"",
                hb("amiss/test-site-context", b"workflow"),
            )
            .0,
        ],
        ..super::policy()
    }
}

#[test]
fn check_plan_v6_digests_keep_existing_bindings() {
    for (profile, policy, digest) in [
        (
            Profile::Observe,
            PolicyControls::default(),
            "sha256:a9564a9c111776215388924bea0a1ae978cab6d5e9227fcb9d3641dc1586ec15",
        ),
        (
            Profile::Enforce,
            PolicyControls {
                semantic_acquisitions: Vec::new(),
                workflow_artifacts: Vec::new(),
                ..full_policy()
            },
            "sha256:56568d4a36eddd0151794df0a52f3bcba0ade1c34f524a9fe50d16606ab4bf44",
        ),
        (
            Profile::Enforce,
            full_policy(),
            "sha256:930dd90a08eca216aca074083f2d584d3386df148c081db5b862f077f2c489e6",
        ),
        (
            Profile::EnforceIntroduced,
            full_policy(),
            "sha256:61c07539d08b731cdfd1e7f70524675ef53721f84930624336a8a9038a660747",
        ),
    ] {
        let plan = check_plan(profile, policy, execution()).unwrap();
        assert_eq!(plan.digest.to_string(), digest);
    }
}

#[test]
fn retained_control_digests_are_verified_before_a_plan_is_used() {
    let fields: [fn(&mut PolicyControls) -> &mut Digest; 3] = [
        |policy| &mut policy.organization_floor.as_mut().unwrap().expected_digest,
        |policy| &mut policy.debt_snapshot.as_mut().unwrap().expected_digest,
        |policy| &mut policy.waiver_bundle.as_mut().unwrap().expected_digest,
    ];
    let original = check_plan(Profile::Enforce, full_policy(), execution()).unwrap();
    for select in fields {
        let mut changed = original.clone();
        *select(&mut changed.policy) = hb("amiss/test-control", b"changed");
        assert!(check_binding(&changed).is_err());
    }
}

#[test]
fn changing_a_typed_control_and_its_digest_cannot_preserve_a_frozen_plan() {
    let changes: [fn(&mut PolicyControls); 6] = [
        |policy| {
            let floor = policy.organization_floor.as_mut().unwrap();
            floor.value.floor_id = ArtifactId::new("other-floor".to_owned()).unwrap();
            floor.expected_digest = canonical_organization_floor(&floor.value).unwrap().1;
        },
        |policy| {
            let debt = policy.debt_snapshot.as_mut().unwrap();
            debt.value.organization_floor_digest = hb("amiss/test-floor", b"changed");
            debt.expected_digest = canonical_debt_snapshot(&debt.value).unwrap().1;
        },
        |policy| {
            let waiver = policy.waiver_bundle.as_mut().unwrap();
            waiver.value.organization_floor_digest = hb("amiss/test-floor", b"changed");
            waiver.expected_digest = canonical_waiver_bundle(&waiver.value).unwrap().1;
        },
        |policy| {
            policy.organization_floor.as_mut().unwrap().trust_source =
                RequestTrust::ExternalRequiredCheck;
        },
        |policy| {
            policy.debt_snapshot.as_mut().unwrap().trust_source =
                RequestTrust::ExternalRequiredCheck;
        },
        |policy| {
            policy.waiver_bundle.as_mut().unwrap().trust_source =
                RequestTrust::ExternalRequiredCheck;
        },
    ];
    assert_frozen_binding_changes(&changes, |policy| policy);
}

#[test]
fn every_workflow_identity_member_changes_the_frozen_binding() {
    let changes: [fn(&mut WorkflowArtifactExpectation); 15] = [
        |artifact| {
            artifact.provider.namespace = ProviderNamespace::new("other".to_owned()).unwrap();
        },
        |artifact| {
            artifact.provider.instance =
                ProviderInstance::new("gitlab.other.internal".to_owned()).unwrap();
            artifact.repository = RepositoryIdentity::new(
                "gitlab.other.internal".to_owned(),
                "platform/security".to_owned(),
                "docs".to_owned(),
            )
            .unwrap();
        },
        |artifact| {
            artifact.repository = RepositoryIdentity::new(
                "gitlab.example.internal".to_owned(),
                "platform/other".to_owned(),
                "docs".to_owned(),
            )
            .unwrap();
        },
        |artifact| {
            artifact.repository = RepositoryIdentity::new(
                "gitlab.example.internal".to_owned(),
                "platform/security".to_owned(),
                "other".to_owned(),
            )
            .unwrap();
        },
        |artifact| artifact.workflow_identity = OpaqueId::new("other.yml".to_owned()).unwrap(),
        |artifact| artifact.event = OpaqueId::new("push".to_owned()).unwrap(),
        |artifact| artifact.artifact_name = "other \"β\"".to_owned(),
        |artifact| {
            artifact.payload_file = RepoPathText::new("other/template.json".to_owned()).unwrap();
        },
        |artifact| artifact.archive_byte_limit = 2048,
        |artifact| artifact.file_byte_limit = 1024,
        |artifact| {
            artifact.semantic.acquisition_identity =
                ArtifactId::new("other-source".to_owned()).unwrap();
        },
        |artifact| {
            artifact.semantic.producer_kind = amiss_wire::semantic::SemanticProducerKind::RecordSet;
        },
        |artifact| {
            artifact.semantic.producer_identity =
                ArtifactId::new("other-producer".to_owned()).unwrap();
        },
        |artifact| artifact.semantic.producer_version = "0.5.2".to_owned(),
        |artifact| artifact.semantic.context_digest = hb("amiss/test-site-context", b"other"),
    ];
    assert_frozen_binding_changes(&changes, |policy| {
        policy.workflow_artifacts.first_mut().unwrap()
    });
}

#[test]
fn semantic_template_identity_members_cannot_change_under_a_frozen_plan() {
    let changes: [fn(&mut SemanticEvidenceTemplate<'static>); 6] = [
        |template| template.complete = !template.complete,
        |template| template.producer.context_digest = hb("amiss/test-context", b"other"),
        |template| template.producer.input_digest = hb("amiss/test-input", b"other"),
        |template| template.producer.kind = amiss_wire::semantic::SemanticProducerKind::RecordSet,
        |template| {
            template.producer.identity = ArtifactId::new("other-producer".to_owned()).unwrap();
        },
        |template| template.producer.version = "0.5.2".to_owned(),
    ];
    assert_frozen_binding_changes(&changes, |policy| {
        policy.semantic_evidence.first_mut().unwrap()
    });
}

fn assert_frozen_binding_changes<T>(
    mutations: &[fn(&mut T)],
    select: fn(&mut PolicyControls) -> &mut T,
) {
    let original = check_plan(Profile::Enforce, full_policy(), execution()).unwrap();
    for (index, mutate) in mutations.iter().enumerate() {
        let mut modified = original.clone();
        mutate(select(&mut modified.policy));
        assert_ne!(
            check_plan(
                modified.profile,
                modified.policy.clone(),
                modified.execution.clone()
            )
            .unwrap()
            .digest,
            original.digest,
            "member {index}"
        );
        assert_eq!(check_binding(&modified), Err(BootstrapJobError::CheckPlan));
    }
}
