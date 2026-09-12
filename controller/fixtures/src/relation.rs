use sha2::Digest as _;
use std::sync::Arc;

use amiss_controller::{
    IntegrationId, OidPair, OpaqueId, PlanScope, ProviderIdentity, ProviderInstance,
    ProviderNamespace, RelationLimits, RelationPlan, RelationStatusDestination, RelationSubject,
    RelationSubjectTransition, RelationTransition, TriggeredRelation, relation_audit_plan,
    relation_transition,
};
use amiss_wire::controls::{ProjectionKind, ProjectionSource, RecordSetSelection};
use amiss_wire::model::{ArtifactId, BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::relation::{
    RelationEvidence, RelationEvidenceSubject, RelationProjectedValue, RelationProjectionSlot,
    assess, evidence, parse_evidence, parse_plan,
};

const REPORT: &[u8] = include_bytes!("../../../spec/examples/scanner-report.json");

pub struct RelationAuditFixture {
    pub transition: RelationTransition,
    pub report: Vec<u8>,
    pub plan: Vec<u8>,
    pub evidence: Option<Vec<u8>>,
    pub assessment: Vec<u8>,
}

/// Builds one exact report-, registry-, and transition-bound relation audit.
#[must_use]
pub fn relation_audit(with_evidence: bool) -> Option<RelationAuditFixture> {
    relation_audit_with_coordination(with_evidence, "workflow/release-42")
}

/// Builds the same exact audit under one caller-selected coordination identity.
#[must_use]
pub fn relation_audit_with_coordination(
    with_evidence: bool,
    coordination: &str,
) -> Option<RelationAuditFixture> {
    let report = report()?;
    let transition = transition(coordination)?;
    let plan = relation_audit_plan(&transition, &report).ok()?;
    let parsed_plan = parse_plan(&plan).ok()?;
    let evidence = if with_evidence {
        Some(relation_evidence(&parsed_plan)?)
    } else {
        None
    };
    let parsed_evidence = evidence.as_deref().map(parse_evidence).transpose().ok()?;
    let assessment = assess(
        &parsed_plan,
        parsed_evidence.as_ref(),
        env!("CARGO_PKG_VERSION"),
        amiss_wire::model::Digest::from([28; 32]),
    )
    .ok()?;
    Some(RelationAuditFixture {
        transition,
        report,
        plan,
        evidence,
        assessment,
    })
}

fn transition(coordination: &str) -> Option<RelationTransition> {
    let registered = registered_relation()?;
    relation_transition(
        TriggeredRelation {
            plan: Arc::clone(&registered),
            trigger_role: ArtifactId::new("source".to_owned())?,
        },
        ArtifactId::new(coordination.to_owned())?,
        [
            frozen(
                "documentation",
                "1111111111111111111111111111111111111111",
                "2222222222222222222222222222222222222222",
                "3333333333333333333333333333333333333333",
                "4444444444444444444444444444444444444444",
            )?,
            frozen(
                "source",
                "d6fcf3ba62c34c4aa77073a6892f39834ef6c5cc",
                "fa2b3687cb16834e7b0ea56d46a0edd775c03d17",
                "d1a175a1986230e4ba44b6f6ed67c8dbccb29aaf",
                "7eed0bc378155f11543b2261997a1f363557e8cd",
            )?,
        ],
    )
    .ok()
}

fn registered_relation() -> Option<Arc<RelationPlan>> {
    let registered = Arc::new(RelationPlan {
        identity: ArtifactId::new("relation/public-api".to_owned())?,
        context_digest: amiss_wire::model::Digest::from([
            0xf3, 0x56, 0xae, 0x83, 0xe3, 0x5d, 0xa8, 0xec, 0x17, 0xf6, 0xaf, 0x65, 0xba, 0xf3,
            0x16, 0x67, 0xcd, 0x1b, 0xe0, 0x32, 0x88, 0xa7, 0xc1, 0x4b, 0x7e, 0xa8, 0x0e, 0x7d,
            0x76, 0xc8, 0x4a, 0x31,
        ]),
        projection: ProjectionKind::SortedRowsV1,
        subjects: [
            subject(
                "documentation",
                "github",
                "github.com",
                RepositoryIdentity::github("acme".to_owned(), "handbook".to_owned())?,
                "docs/public-api",
            )?,
            subject(
                "source",
                "gitlab",
                "git.example.internal",
                RepositoryIdentity::new(
                    "git.example.internal".to_owned(),
                    "group/subgroup".to_owned(),
                    "widget".to_owned(),
                )?,
                "rust/public-api",
            )?,
        ],
        aggregate_limits: RelationLimits {
            acquisition_objects: 150,
            acquisition_bytes: 1_572_864,
            projection_records: 150,
            projection_bytes: 1_572_864,
        },
        status_destinations: vec![RelationStatusDestination {
            subject_role: ArtifactId::new("documentation".to_owned())?,
            required_status_name: "Amiss cross-repository".parse().ok()?,
        }],
    });
    Some(registered)
}

fn report() -> Option<Vec<u8>> {
    let mut report: amiss_wire::report::model::ReportEnvelope =
        serde_json::from_slice(REPORT).ok()?;
    let amiss_wire::report::model::Evaluation::Resolved(evaluation) =
        &mut report.payload.evaluation
    else {
        return None;
    };
    evaluation.target_ref = Some("refs/heads/main".parse().ok()?);
    report.payload_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(amiss_wire::report::PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&report.payload).ok()?)
            .finalize()
            .0,
    );
    serde_json_canonicalizer::to_vec(&report).ok()
}

fn subject(
    role: &str,
    provider: &str,
    instance: &str,
    repository: RepositoryIdentity,
    set: &str,
) -> Option<RelationSubject> {
    Some(RelationSubject {
        role: ArtifactId::new(role.to_owned())?,
        scope: PlanScope {
            provider: ProviderIdentity {
                namespace: ProviderNamespace::new(provider.to_owned())?,
                instance: ProviderInstance::new(instance.to_owned())?,
            },
            integration: IntegrationId::new(format!("integration/{role}"))?,
            repository,
        },
        target: BranchRef::new("refs/heads/main".to_owned())?,
        object_format: ObjectFormat::Sha1,
        credential: OpaqueId::new(format!("credential/{role}"))?,
        source: ProjectionSource::RecordSet(RecordSetSelection {
            set: ArtifactId::new(set.to_owned())?,
        }),
        limits: RelationLimits {
            acquisition_objects: 100,
            acquisition_bytes: 1_048_576,
            projection_records: 100,
            projection_bytes: 1_048_576,
        },
    })
}

fn frozen(
    role: &str,
    base_commit: &str,
    base_tree: &str,
    candidate_commit: &str,
    candidate_tree: &str,
) -> Option<RelationSubjectTransition> {
    Some(RelationSubjectTransition {
        role: ArtifactId::new(role.to_owned())?,
        commits: OidPair {
            base: Oid::new(ObjectFormat::Sha1, base_commit.to_owned())?,
            candidate: Oid::new(ObjectFormat::Sha1, candidate_commit.to_owned())?,
        },
        trees: OidPair {
            base: Oid::new(ObjectFormat::Sha1, base_tree.to_owned())?,
            candidate: Oid::new(ObjectFormat::Sha1, candidate_tree.to_owned())?,
        },
    })
}

fn relation_evidence(plan: &amiss_wire::relation::RelationPlanEnvelope) -> Option<Vec<u8>> {
    let aligned = RelationProjectedValue {
        value_digest: amiss_wire::model::Digest::from([30; 32]),
        value_bytes: 12,
    };
    let changed = RelationProjectedValue {
        value_digest: amiss_wire::model::Digest::from([31; 32]),
        value_bytes: 13,
    };
    evidence(&RelationEvidence {
        schema: amiss_wire::relation::EvidencePayloadSchema::Current,
        plan_payload_digest: plan.payload_digest,
        subjects: [
            RelationEvidenceSubject {
                role: ArtifactId::new("documentation".to_owned())?,
                base: RelationProjectionSlot::Projected(aligned),
                candidate: RelationProjectionSlot::Projected(aligned),
            },
            RelationEvidenceSubject {
                role: ArtifactId::new("source".to_owned())?,
                base: RelationProjectionSlot::Projected(aligned),
                candidate: RelationProjectionSlot::Projected(changed),
            },
        ],
    })
    .ok()
}
