#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed provider fixtures must fail loudly"
)]

use std::sync::atomic::{AtomicUsize, Ordering};

use amiss_controller::{IntegrationId, ProviderError, ProviderInstance, RelationSubject};
use amiss_controller_fixtures::relation::relation_audit;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};

use super::{GitHubRelationRest, RelationSubjectHead};
use crate::live::model::CommitRecord;
use crate::live::{Client, Config};

const APP_ID: u64 = 99;
const INSTALLATION_ID: u64 = 7;

#[test]
fn exact_subject_resolves_one_current_head() {
    let (config, subject) = fixture();
    let client = Client {
        config,
        rest: FakeRelationRest::new(Ok(CommitRecord {
            sha: "3".repeat(40),
            tree: "4".repeat(40),
        })),
    };

    assert_eq!(
        client.resolve_relation_head(&subject),
        Ok(RelationSubjectHead {
            subject,
            candidate_commit: oid(ObjectFormat::Sha1, '3'),
        })
    );
    assert_eq!(client.rest.calls.load(Ordering::Relaxed), 1);
}

#[test]
fn request_scope_is_checked_before_provider_io() {
    let (config, subject) = fixture();
    let defects: [fn(&mut RelationSubject); 6] = [
        |subject| {
            subject.scope.provider.instance =
                ProviderInstance::new("github.example".to_owned()).unwrap();
        },
        |subject| {
            subject.scope.integration = IntegrationId::new("8".to_owned()).unwrap();
        },
        |subject| {
            subject.scope.repository = RepositoryIdentity::new(
                "github.example".to_owned(),
                "acme".to_owned(),
                "handbook".to_owned(),
            )
            .unwrap();
        },
        |subject| {
            subject.scope.repository = RepositoryIdentity::new(
                "github.com".to_owned(),
                "group/acme".to_owned(),
                "handbook".to_owned(),
            )
            .unwrap();
        },
        |subject| subject.object_format = ObjectFormat::Sha256,
        |subject| {
            subject.scope.provider.namespace =
                amiss_controller::ProviderNamespace::new("gitlab".to_owned()).unwrap();
        },
    ];

    for defect in defects {
        let mut changed = subject.clone();
        defect(&mut changed);
        let client = Client {
            config: config.clone(),
            rest: FakeRelationRest::new(Ok(CommitRecord {
                sha: "3".repeat(40),
                tree: "4".repeat(40),
            })),
        };
        assert_eq!(
            client.resolve_relation_head(&changed),
            Err(ProviderError::InvalidResponse)
        );
        assert_eq!(client.rest.calls.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn malformed_head_or_provider_failure_is_not_a_finality_fact() {
    let (config, subject) = fixture();
    for head in [
        Ok(CommitRecord {
            sha: "not-an-oid".to_owned(),
            tree: "4".repeat(40),
        }),
        Ok(CommitRecord {
            sha: "3".repeat(40),
            tree: "not-an-oid".to_owned(),
        }),
        Err(ProviderError::Unavailable),
    ] {
        let expected = head
            .as_ref()
            .err()
            .copied()
            .unwrap_or(ProviderError::InvalidResponse);
        let client = Client {
            config: config.clone(),
            rest: FakeRelationRest::new(head),
        };
        assert_eq!(client.resolve_relation_head(&subject), Err(expected));
    }
}

fn fixture() -> (Config, RelationSubject) {
    let relation = relation_audit(true).unwrap();
    let mut subject = relation
        .transition
        .relation
        .plan
        .subjects
        .iter()
        .find(|subject| subject.scope.provider.namespace.as_str() == "github")
        .unwrap()
        .clone();
    subject.scope.integration = IntegrationId::new(INSTALLATION_ID.to_string()).unwrap();
    (
        Config {
            provider: subject.scope.provider.clone(),
            app_id: APP_ID,
            installation_id: INSTALLATION_ID,
            required_status_name: "amiss/provider".to_owned(),
        },
        subject,
    )
}

struct FakeRelationRest {
    head: Result<CommitRecord, ProviderError>,
    calls: AtomicUsize,
}

impl FakeRelationRest {
    fn new(head: Result<CommitRecord, ProviderError>) -> Self {
        Self {
            head,
            calls: AtomicUsize::new(0),
        }
    }
}

impl GitHubRelationRest for FakeRelationRest {
    fn relation_head(
        &self,
        _repository: &RepositoryIdentity,
        _target: &BranchRef,
    ) -> Result<CommitRecord, ProviderError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.head.clone()
    }
}

fn oid(format: ObjectFormat, value: char) -> Oid {
    let length = match format {
        ObjectFormat::Sha1 => 40,
        ObjectFormat::Sha256 => 64,
    };
    Oid::new(format, value.to_string().repeat(length)).unwrap()
}
