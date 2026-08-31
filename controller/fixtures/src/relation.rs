use std::sync::Arc;

use amiss_controller::{
    IntegrationId, OidPair, OpaqueId, PlanScope, ProviderIdentity, ProviderInstance,
    ProviderNamespace, RelationLimits, RelationPlan, RelationStatusDestination, RelationSubject,
    RelationSubjectTransition, RelationTransition, TriggeredRelation, relation_transition,
};
use amiss_wire::controls::{ProjectionKind, ProjectionSource, RecordSetSelection};
use amiss_wire::digest::{Digest, hj, sha256};
use amiss_wire::json::{self, Value};
use amiss_wire::model::{ArtifactId, BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::relation::{
    RelationEvidence, RelationEvidenceSubject, RelationIdentity, RelationProjectedValue,
    RelationSnapshot, RelationSubject as PlannedSubject, assess, evidence, parse_evidence,
    parse_plan, plan,
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
    let parsed = json::parse(&report).ok()?;
    let (_, report_payload_digest, _) = amiss_wire::report::validate_envelope(&parsed).ok()?;
    let report_payload_digest = Digest::from_wire(report_payload_digest)?;
    let transition = transition(coordination)?;
    let registered = transition.relation.plan.as_ref();
    let subjects = transition.subjects.clone().map(|frozen| {
        let configured = registered
            .subjects
            .iter()
            .find(|subject| subject.role == frozen.role)?;
        Some(PlannedSubject {
            role: frozen.role,
            repository: configured.scope.repository.clone(),
            target: configured.target.clone(),
            source: configured.source.clone(),
            base: RelationSnapshot {
                commit: frozen.commits.base,
                tree: frozen.trees.base,
            },
            candidate: RelationSnapshot {
                commit: frozen.commits.candidate,
                tree: frozen.trees.candidate,
            },
        })
    });
    let [Some(documentation), Some(source)] = subjects else {
        return None;
    };
    let plan = json::canonical(
        &plan(&amiss_wire::relation::RelationPlan {
            report_payload_digest,
            relation: RelationIdentity {
                identity: registered.identity.clone(),
                context_digest: registered.context_digest,
            },
            coordination: transition.coordination.clone(),
            trigger_role: transition.relation.trigger_role.clone(),
            projection: registered.projection,
            subjects: [documentation, source],
        })
        .ok()?,
    );
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
        sha256(b"relation evaluator fixture"),
    )
    .ok()?;
    Some(RelationAuditFixture {
        transition,
        report,
        plan,
        evidence,
        assessment: json::canonical(&assessment),
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
        context_digest: sha256(b"operator relation context"),
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
            required_status_name: "Amiss cross-repository".to_owned(),
        }],
    });
    Some(registered)
}

fn report() -> Option<Vec<u8>> {
    let mut report = json::parse(REPORT).ok()?;
    let Value::Object(envelope) = &mut report else {
        return None;
    };
    let payload = envelope
        .iter_mut()
        .find_map(|(key, value)| (key == "payload").then_some(value))?;
    let Value::Object(payload_members) = payload else {
        return None;
    };
    let evaluation = payload_members
        .iter_mut()
        .find_map(|(key, value)| (key == "evaluation").then_some(value))?;
    let Value::Object(evaluation_members) = evaluation else {
        return None;
    };
    *evaluation_members
        .iter_mut()
        .find_map(|(key, value)| (key == "target_ref").then_some(value))? =
        Value::string("refs/heads/main".to_owned());
    let payload_digest = hj(amiss_wire::report::PAYLOAD_SCHEMA, payload);
    *envelope
        .iter_mut()
        .find_map(|(key, value)| (key == "payload_digest").then_some(value))? =
        Value::string(payload_digest.to_string());
    Some(json::canonical(&report))
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
        value_digest: sha256(b"timeout: u64"),
        value_bytes: 12,
    };
    let changed = RelationProjectedValue {
        value_digest: sha256(b"timeout: u128"),
        value_bytes: 13,
    };
    evidence(&RelationEvidence {
        plan_payload_digest: plan.payload_digest,
        subjects: [
            RelationEvidenceSubject {
                role: ArtifactId::new("documentation".to_owned())?,
                base: Some(aligned),
                candidate: Some(aligned),
            },
            RelationEvidenceSubject {
                role: ArtifactId::new("source".to_owned())?,
                base: Some(aligned),
                candidate: Some(changed),
            },
        ],
    })
    .ok()
    .map(|value| json::canonical(&value))
}
