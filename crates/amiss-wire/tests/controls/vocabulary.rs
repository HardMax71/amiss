use std::collections::BTreeSet;

use amiss_wire::controls::{ContentAvailability, GitMode, Profile, ResourceName};
use amiss_wire::json::Value;

#[test]
fn the_profile_vocabulary_is_the_closed_triple() {
    for (name, profile) in [
        ("observe", Profile::Observe),
        ("enforce-introduced", Profile::EnforceIntroduced),
        ("enforce", Profile::Enforce),
    ] {
        assert_eq!(
            Profile::decode("$.minimum_profile", Value::String(name.to_owned())),
            Ok(profile),
            "{name}"
        );
    }
    assert!(Profile::decode("$.minimum_profile", Value::String("enforced".to_owned())).is_err());
}

#[test]
fn git_modes_project_distinct_nonempty_spellings() {
    let spellings: BTreeSet<&str> = GitMode::all().map(GitMode::as_str).collect();
    assert!(spellings.iter().all(|mode| !mode.is_empty()));
    assert_eq!(spellings.len(), GitMode::all().len());
}

#[test]
fn availability_states_project_distinct_nonempty_spellings() {
    let spellings: BTreeSet<&str> = ContentAvailability::all()
        .map(ContentAvailability::as_str)
        .collect();
    assert!(spellings.iter().all(|state| !state.is_empty()));
    assert_eq!(spellings.len(), ContentAvailability::all().len());
}

#[test]
fn resource_phases_are_a_nonempty_partition_with_more_than_one_class() {
    let phases: BTreeSet<&str> = ResourceName::all().map(ResourceName::phase).collect();
    assert!(phases.iter().all(|phase| !phase.is_empty()));
    assert!(phases.len() > 1);
}
