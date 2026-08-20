use std::collections::BTreeSet;

use amiss_wire::controls::{
    ConstraintPlatform, ContentAvailability, GitMode, Profile, ResourceName,
};
use strum::IntoEnumIterator;

#[test]
fn the_profile_vocabulary_is_the_closed_triple() {
    for (name, profile) in [
        ("observe", Profile::Observe),
        ("enforce-introduced", Profile::EnforceIntroduced),
        ("enforce", Profile::Enforce),
    ] {
        assert_eq!(name.parse(), Ok(profile), "{name}");
    }
    assert!("enforced".parse::<Profile>().is_err());
}

/// Every platform in the closed table answers to its own spelling, since a
/// constraint that named one and decoded to another would bind the run to a
/// host it was never written for.
#[test]
fn the_platform_vocabulary_is_the_closed_six() {
    let table = [
        ConstraintPlatform::LinuxX8664,
        ConstraintPlatform::LinuxAarch64,
        ConstraintPlatform::MacosX8664,
        ConstraintPlatform::MacosAarch64,
        ConstraintPlatform::WindowsX8664,
        ConstraintPlatform::WindowsAarch64,
    ];
    for platform in table {
        let name = platform.as_ref();
        assert_eq!(name.parse(), Ok(platform), "{name}");
    }
    let spellings: BTreeSet<&str> = table.iter().map(AsRef::as_ref).collect();
    assert_eq!(spellings.len(), table.len(), "no two share a spelling");
    assert!("linux-x86".parse::<ConstraintPlatform>().is_err());
}

#[test]
fn git_modes_project_distinct_nonempty_spellings() {
    let spellings: BTreeSet<&str> = GitMode::iter().map(Into::into).collect();
    assert!(spellings.iter().all(|mode| !mode.is_empty()));
    assert_eq!(spellings.len(), GitMode::iter().len());
}

#[test]
fn availability_states_project_distinct_nonempty_spellings() {
    let spellings: BTreeSet<&str> = ContentAvailability::iter().map(Into::into).collect();
    assert!(spellings.iter().all(|state| !state.is_empty()));
    assert_eq!(spellings.len(), ContentAvailability::iter().len());
}

#[test]
fn resource_phases_are_a_nonempty_partition_with_more_than_one_class() {
    let phases: BTreeSet<&str> = ResourceName::iter().map(ResourceName::phase).collect();
    assert!(phases.iter().all(|phase| !phase.is_empty()));
    assert!(phases.len() > 1);
}
