#![cfg(test)]

use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, DeliveryId, DeliveryIdentity, IntegrationId,
    OidPair, ProviderError, ProviderIdentity, ProviderInstance, ProviderNamespace, RunIdentity,
    RunRefs,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use super::{event_bound_run, validate_delivery};
use crate::DedicatedReviewer;
use crate::identity::{canonical_host, canonical_segment};

fn oid(fill: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, fill.to_string().repeat(40)).expect("an object id")
}

fn branch(name: &str) -> BranchRef {
    BranchRef::new(format!("refs/heads/{name}")).expect("a branch ref")
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("gitea".to_owned()).expect("a namespace"),
        instance: ProviderInstance::new("forge.example".to_owned()).expect("an instance"),
    }
}

fn reviewer() -> DedicatedReviewer {
    DedicatedReviewer::new(77, "amiss-controller".to_owned()).expect("a reviewer")
}

fn delivery() -> AuthenticatedDelivery {
    let provider = provider();
    let change = ChangeLocator {
        provider: provider.clone(),
        repository: RepositoryIdentity::new(
            "forge.example".to_owned(),
            "acme".to_owned(),
            "widget".to_owned(),
        )
        .expect("an identity"),
        change: ChangeId::new("repository/101/pull/4201/number/42".to_owned()).expect("a change"),
    };
    let integration = IntegrationId::new("77".to_owned()).expect("an integration");
    let provider_run = crate::identity::provider_run(
        &integration,
        &change,
        &oid('b'),
        &branch("topic"),
        &branch("main"),
    )
    .expect("a provider run");
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider,
            integration,
            delivery: DeliveryId::new("signed-body".to_owned()).expect("a delivery id"),
        },
        change,
        provider_run,
    }
}

fn run(delivery: &AuthenticatedDelivery) -> RunIdentity {
    RunIdentity::new(
        delivery.change.clone(),
        RunRefs {
            forge: ForgeDialect::Gitea,
            candidate: branch("topic"),
            target: branch("main"),
            default_branch: branch("main"),
        },
        ObjectFormat::Sha1,
        OidPair {
            base: oid('a'),
            candidate: oid('b'),
        },
        OidPair {
            base: oid('c'),
            candidate: oid('d'),
        },
    )
    .expect("a run identity")
}

type Deviation = fn(&mut AuthenticatedDelivery);

/// The delivery is bound in every field, and one wrong field is enough.
#[test]
fn a_delivery_answers_for_every_field_alone() {
    let sound = delivery();
    assert!(validate_delivery(&sound, &provider(), &reviewer()).is_ok());

    let elsewhere = ProviderIdentity {
        namespace: ProviderNamespace::new("gitea".to_owned()).expect("a namespace"),
        instance: ProviderInstance::new("other.example".to_owned()).expect("an instance"),
    };
    let rows: [(&str, Deviation); 6] = [
        ("another delivery provider", |delivery| {
            delivery.identity.provider = ProviderIdentity {
                namespace: ProviderNamespace::new("forgejo".to_owned()).expect("a namespace"),
                instance: ProviderInstance::new("forge.example".to_owned()).expect("an instance"),
            };
        }),
        ("another change provider", |delivery| {
            delivery.change.provider = ProviderIdentity {
                namespace: ProviderNamespace::new("forgejo".to_owned()).expect("a namespace"),
                instance: ProviderInstance::new("forge.example".to_owned()).expect("an instance"),
            };
        }),
        ("a nested owner", |delivery| {
            delivery.change.repository = RepositoryIdentity::new(
                "forge.example".to_owned(),
                "group/acme".to_owned(),
                "widget".to_owned(),
            )
            .expect("an identity");
        }),
        ("a second attempt", |delivery| {
            delivery.provider_run.attempt =
                amiss_controller::ProviderRunAttempt::new(2).expect("an attempt");
        }),
        ("another object format", |delivery| {
            delivery.provider_run.object_format = ObjectFormat::Sha256;
        }),
        ("a run nobody minted", |delivery| {
            delivery.provider_run.run_id =
                amiss_controller::ProviderRunId::new("pr:not-a-digest".to_owned())
                    .expect("a run id");
        }),
    ];
    for (reason, deviate) in rows {
        let mut wrong = delivery();
        deviate(&mut wrong);
        assert_eq!(
            validate_delivery(&wrong, &provider(), &reviewer()).err(),
            Some(ProviderError::InvalidResponse),
            "{reason}"
        );
    }

    assert_eq!(
        validate_delivery(&sound, &elsewhere, &reviewer()).err(),
        Some(ProviderError::InvalidResponse),
        "a host that is not the provider instance"
    );
}

/// A run is event bound in every field it echoes.
#[test]
fn a_run_answers_for_every_echoed_field() {
    let delivery = delivery();
    assert_eq!(event_bound_run(&delivery, &run(&delivery)), Ok(true));

    let mut forge = run(&delivery);
    forge.refs.forge = ForgeDialect::Github;
    assert_eq!(
        event_bound_run(&delivery, &forge),
        Err(ProviderError::InvalidResponse),
        "another forge dialect"
    );

    let mut format = run(&delivery);
    format.object_format = ObjectFormat::Sha256;
    assert_eq!(
        event_bound_run(&delivery, &format),
        Err(ProviderError::InvalidResponse),
        "another object format"
    );

    let mut candidate = run(&delivery);
    candidate.commits.candidate = oid('9');
    assert_eq!(
        event_bound_run(&delivery, &candidate),
        Err(ProviderError::InvalidResponse),
        "another candidate commit"
    );
}

/// A segment is one lowercase word within its length, and a host is
/// lowercase dns with bounded labels.
#[test]
fn identity_grammars_answer_from_both_sides() {
    assert_eq!(canonical_segment("Acme").as_deref(), Some("acme"));
    assert_eq!(
        canonical_segment(&"a".repeat(100)).as_deref(),
        Some("a".repeat(100).as_str()),
        "a segment exactly at its ceiling"
    );
    assert_eq!(canonical_segment(""), None, "a segment naming nothing");
    assert_eq!(
        canonical_segment(&"a".repeat(101)),
        None,
        "one character past the ceiling"
    );
    assert_eq!(
        canonical_segment("group/acme"),
        None,
        "a segment is not a path"
    );

    for valid in ["forge.example", "my-host.example", "a1.io", "x"] {
        assert!(canonical_host(valid), "{valid}");
    }
    let overlong = format!("{}a", "a.".repeat(127));
    let long_label = format!("{}.com", "a".repeat(64));
    for invalid in [
        "UPPER.example",
        overlong.as_str(),
        long_label.as_str(),
        "-ab.example",
        "ab-.example",
        "a..b",
        "",
    ] {
        assert!(!canonical_host(invalid), "{invalid}");
    }
}
