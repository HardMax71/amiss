#![expect(
    clippy::unwrap_used,
    reason = "shared tests build known-valid relation contract values"
)]

use amiss_wire::controls::{ProjectionKind, ProjectionSource, RecordSetSelection};
use amiss_wire::digest::Digest;
use amiss_wire::model::{ArtifactId, BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::relation::{
    RelationEvidence, RelationEvidenceSubject, RelationIdentity, RelationPlan,
    RelationProjectedValue, RelationProjectionSlot, RelationSnapshot, RelationSubject, parse_plan,
    plan as build_plan,
};

pub(crate) fn digest(digit: char) -> Digest {
    Digest::from_wire(&format!("sha256:{}", digit.to_string().repeat(64))).unwrap()
}

pub(crate) fn identity(value: &str) -> ArtifactId {
    ArtifactId::new(value.to_owned()).unwrap()
}

pub(crate) fn oid(digit: char, object_format: ObjectFormat) -> Oid {
    let width = match object_format {
        ObjectFormat::Sha1 => 40,
        ObjectFormat::Sha256 => 64,
    };
    Oid::new(object_format, digit.to_string().repeat(width)).unwrap()
}

fn subject(
    role: &str,
    repository: &str,
    set: &str,
    object_format: ObjectFormat,
    digits: [char; 4],
) -> RelationSubject {
    RelationSubject {
        role: identity(role),
        repository: RepositoryIdentity::github("acme".to_owned(), repository.to_owned()).unwrap(),
        target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        object_format,
        source: ProjectionSource::RecordSet(RecordSetSelection { set: identity(set) }),
        base: RelationSnapshot {
            commit: oid(digits[0], object_format),
            tree: oid(digits[1], object_format),
        },
        candidate: RelationSnapshot {
            commit: oid(digits[2], object_format),
            tree: oid(digits[3], object_format),
        },
    }
}

pub(crate) struct RelationContract {
    pub plan: RelationPlan,
    pub evidence: RelationEvidence,
}

pub(crate) fn relation_contract() -> RelationContract {
    let plan = RelationPlan {
        report_payload_digest: digest('1'),
        relation: RelationIdentity {
            identity: identity("relation/public-api"),
            context_digest: digest('2'),
        },
        coordination: identity("workflow/release-42"),
        trigger_role: identity("source"),
        projection: ProjectionKind::SortedRowsV1,
        subjects: [
            subject(
                "documentation",
                "handbook",
                "docs/public-api",
                ObjectFormat::Sha1,
                ['a', 'b', 'a', 'b'],
            ),
            subject(
                "source",
                "service",
                "rust/public-api",
                ObjectFormat::Sha256,
                ['c', 'd', 'e', 'f'],
            ),
        ],
    };
    let plan_bytes = build_plan(&plan).unwrap();
    let evidence = RelationEvidence {
        plan_payload_digest: parse_plan(&plan_bytes).unwrap().payload_digest,
        subjects: [
            RelationEvidenceSubject {
                role: identity("documentation"),
                base: RelationProjectionSlot::Projected(projected('a', 1_024)),
                candidate: RelationProjectionSlot::Projected(projected('a', 1_024)),
            },
            RelationEvidenceSubject {
                role: identity("source"),
                base: RelationProjectionSlot::Projected(projected('a', 1_024)),
                candidate: RelationProjectionSlot::Projected(projected('b', 1_031)),
            },
        ],
    };
    RelationContract { plan, evidence }
}

pub(crate) fn projected(digit: char, value_bytes: u64) -> RelationProjectedValue {
    RelationProjectedValue {
        value_digest: digest(digit),
        value_bytes,
    }
}
