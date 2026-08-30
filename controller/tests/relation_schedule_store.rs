#![cfg(not(miri))]

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::sync::{Arc, Barrier};

use amiss_controller::{
    FileRelationScheduleStore, RelationAdmission, RelationScheduleError, RelationScheduleStoreError,
};
use amiss_controller_fixtures::relation::relation_audit;
use amiss_wire::digest::sha256;
use amiss_wire::model::ArtifactId;

#[test]
fn restart_retains_current_work_and_historical_coordination_bindings() {
    let directory = tempfile::tempdir().unwrap();
    let first_transition = relation_audit(true).unwrap().transition;
    let store = FileRelationScheduleStore::open(directory.path(), 4).unwrap();
    let RelationAdmission::Scheduled(first) = store.schedule(first_transition.clone()).unwrap()
    else {
        panic!("the first exact transition schedules");
    };
    assert_eq!(first.fence.get(), 1);
    assert!(store.is_current(&first).unwrap());
    let journal = directory.path().join(".amiss-relation-schedules.journal");
    let bytes = fs::read(&journal).unwrap();
    assert!(
        first_transition
            .relation
            .plan
            .subjects
            .iter()
            .all(|subject| {
                !bytes
                    .windows(subject.credential.as_str().len())
                    .any(|window| window == subject.credential.as_str().as_bytes())
            })
    );

    drop(store);
    let store = FileRelationScheduleStore::open(directory.path(), 4).unwrap();
    let mut opposite_trigger = first_transition.clone();
    opposite_trigger.relation.trigger_role = ArtifactId::new("documentation".to_owned()).unwrap();
    let RelationAdmission::Duplicate(repeated) = store.schedule(opposite_trigger).unwrap() else {
        panic!("the other trigger repeats the first retained work");
    };
    assert_eq!(repeated, first);

    let mut second_transition = first_transition.clone();
    second_transition.coordination = ArtifactId::new("workflow/release-43".to_owned()).unwrap();
    let RelationAdmission::Scheduled(second) = store.schedule(second_transition).unwrap() else {
        panic!("a new coordination identity advances the durable fence");
    };
    assert_eq!(second.fence.get(), 2);
    assert!(!store.is_current(&first).unwrap());
    assert!(store.is_current(&second).unwrap());

    let RelationAdmission::Duplicate(delayed) = store.schedule(first_transition.clone()).unwrap()
    else {
        panic!("a delayed retry remains bound to its historical fence");
    };
    assert_eq!(delayed.fence.get(), 1);
    assert!(!store.is_current(&delayed).unwrap());
    assert!(store.is_current(&second).unwrap());

    let mut rebound = first_transition;
    rebound.subjects[1].trees.candidate = rebound.subjects[1].trees.base.clone();
    assert!(matches!(
        store.schedule(rebound),
        Err(RelationScheduleStoreError::Schedule(
            RelationScheduleError::CoordinationConflict
        ))
    ));
    drop(store);

    let committed_bytes = fs::metadata(&journal).unwrap().len();
    OpenOptions::new()
        .append(true)
        .open(&journal)
        .unwrap()
        .write_all(b"interrupted append")
        .unwrap();
    let reopened = FileRelationScheduleStore::open(directory.path(), 4).unwrap();
    assert_eq!(fs::metadata(&journal).unwrap().len(), committed_bytes);
    assert!(!reopened.is_current(&first).unwrap());
    assert!(reopened.is_current(&second).unwrap());
}

#[test]
fn capacity_refuses_only_new_bindings_and_is_immutable_on_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let first_transition = relation_audit(true).unwrap().transition;
    let store = FileRelationScheduleStore::open(directory.path(), 1).unwrap();
    let RelationAdmission::Scheduled(first) = store.schedule(first_transition.clone()).unwrap()
    else {
        panic!("the first binding fits");
    };
    assert!(matches!(
        store.schedule(first_transition.clone()).unwrap(),
        RelationAdmission::Duplicate(_)
    ));

    let mut next = first_transition;
    next.coordination = ArtifactId::new("workflow/release-43".to_owned()).unwrap();
    assert!(matches!(
        store.schedule(next),
        Err(RelationScheduleStoreError::Full)
    ));
    assert!(store.is_current(&first).unwrap());
    drop(store);

    assert!(matches!(
        FileRelationScheduleStore::open(directory.path(), 2),
        Err(RelationScheduleStoreError::Configuration)
    ));
    assert!(FileRelationScheduleStore::open(directory.path(), 1).is_ok());
}

#[test]
fn configuration_rebinding_and_missing_state_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let transition = relation_audit(true).unwrap().transition;
    let store = FileRelationScheduleStore::open(directory.path(), 2).unwrap();
    let RelationAdmission::Scheduled(first) = store.schedule(transition.clone()).unwrap() else {
        panic!("the first binding schedules");
    };

    let mut rebound = transition;
    let mut plan = rebound.relation.plan.as_ref().clone();
    plan.context_digest = sha256(b"different complete operator configuration");
    rebound.relation.plan = Arc::new(plan);
    assert!(matches!(
        store.schedule(rebound),
        Err(RelationScheduleStoreError::Schedule(
            RelationScheduleError::BindingConflict
        ))
    ));
    assert!(store.is_current(&first).unwrap());
    drop(store);

    fs::remove_file(directory.path().join(".amiss-relation-schedules.journal")).unwrap();
    assert!(matches!(
        FileRelationScheduleStore::open(directory.path(), 2),
        Err(RelationScheduleStoreError::Corrupt)
    ));
}

#[test]
fn concurrent_retries_admit_one_exact_binding() {
    let directory = tempfile::tempdir().unwrap();
    let stores = [
        FileRelationScheduleStore::open(directory.path(), 2).unwrap(),
        FileRelationScheduleStore::open(directory.path(), 2).unwrap(),
    ];
    let transition = relation_audit(true).unwrap().transition;
    let barrier = Arc::new(Barrier::new(3));
    let workers = stores
        .into_iter()
        .map(|store| {
            let transition = transition.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                store.schedule(transition)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let outcomes = workers
        .into_iter()
        .map(|worker| worker.join().unwrap().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RelationAdmission::Scheduled(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RelationAdmission::Duplicate(_)))
            .count(),
        1
    );
    let Some(outcome) = outcomes.into_iter().next() else {
        panic!("two workers returned outcomes");
    };
    let pending = match outcome {
        RelationAdmission::Scheduled(pending) | RelationAdmission::Duplicate(pending) => pending,
    };
    assert!(
        FileRelationScheduleStore::open(directory.path(), 2)
            .unwrap()
            .is_current(&pending)
            .unwrap()
    );
}

#[test]
fn concurrent_new_work_advances_one_shared_fence() {
    let directory = tempfile::tempdir().unwrap();
    let stores = [
        FileRelationScheduleStore::open(directory.path(), 2).unwrap(),
        FileRelationScheduleStore::open(directory.path(), 2).unwrap(),
    ];
    let first = relation_audit(true).unwrap().transition;
    let mut second = first.clone();
    second.coordination = ArtifactId::new("workflow/release-43".to_owned()).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let workers = stores
        .into_iter()
        .zip([first, second])
        .map(|(store, transition)| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                store.schedule(transition).unwrap()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let pending = workers
        .into_iter()
        .map(|worker| match worker.join().unwrap() {
            RelationAdmission::Scheduled(pending) => pending,
            RelationAdmission::Duplicate(_) => panic!("distinct work schedules"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        pending
            .iter()
            .map(|pending| pending.fence.get())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([1, 2])
    );
    let reopened = FileRelationScheduleStore::open(directory.path(), 2).unwrap();
    assert_eq!(
        pending
            .iter()
            .filter(|pending| reopened.is_current(pending).unwrap())
            .count(),
        1
    );
}

#[test]
fn committed_journal_truncation_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let store = FileRelationScheduleStore::open(directory.path(), 1).unwrap();
    store
        .schedule(relation_audit(true).unwrap().transition)
        .unwrap();
    drop(store);
    let journal = OpenOptions::new()
        .write(true)
        .open(directory.path().join(".amiss-relation-schedules.journal"))
        .unwrap();
    journal
        .set_len(journal.metadata().unwrap().len() - 1)
        .unwrap();
    assert!(matches!(
        FileRelationScheduleStore::open(directory.path(), 1),
        Err(RelationScheduleStoreError::Corrupt)
    ));
}

#[test]
fn committed_journal_mutation_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let store = FileRelationScheduleStore::open(directory.path(), 1).unwrap();
    store
        .schedule(relation_audit(true).unwrap().transition)
        .unwrap();
    drop(store);
    let journal = directory.path().join(".amiss-relation-schedules.journal");
    let mut bytes = fs::read(&journal).unwrap();
    *bytes.last_mut().unwrap() = b'!';
    fs::write(journal, bytes).unwrap();
    assert!(matches!(
        FileRelationScheduleStore::open(directory.path(), 1),
        Err(RelationScheduleStoreError::Corrupt)
    ));
}
