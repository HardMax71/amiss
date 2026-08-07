use amiss_controller::{
    AcquireError, BootstrapJobError, ControllerError, OpaqueId, PlanError, ProviderError,
    ProviderNamespace, RegistryError, SystemClock,
};

fn all_distinct_and_nonempty(messages: &[String]) {
    assert!(messages.iter().all(|message| !message.is_empty()));
    let unique: std::collections::BTreeSet<&str> = messages.iter().map(String::as_str).collect();
    assert_eq!(unique.len(), messages.len(), "{messages:?}");
}

/// Every controller-facing error names itself, distinct within its own
/// type, which is what a stubbed Display flattens. Types may share a
/// sentence across layers on purpose: the plan registry and the bootstrap
/// job both say "the check plan changed after validation".
#[test]
fn every_error_message_is_its_own_sentence() {
    let acquire = [
        AcquireError::PlanBinding,
        AcquireError::RepositoryObjects,
        AcquireError::RepositoryTree,
        AcquireError::ActionObjects,
        AcquireError::ActionTree,
    ];
    all_distinct_and_nonempty(&acquire.iter().map(ToString::to_string).collect::<Vec<_>>());
    let bootstrap = [
        BootstrapJobError::RunIdentity,
        BootstrapJobError::CheckPlan,
        BootstrapJobError::OrganizationFloor,
        BootstrapJobError::DebtSnapshot,
        BootstrapJobError::WaiverBundle,
        BootstrapJobError::ControlBinding,
        BootstrapJobError::ExecutionConstraint,
        BootstrapJobError::TrustedTime,
        BootstrapJobError::RequestEncoding,
    ];
    all_distinct_and_nonempty(
        &bootstrap
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    );
    let plan = [PlanError::Duplicate, PlanError::Missing, PlanError::Invalid];
    all_distinct_and_nonempty(&plan.iter().map(ToString::to_string).collect::<Vec<_>>());
    let provider = [
        ProviderError::Authentication,
        ProviderError::AuthorizationRevoked,
        ProviderError::Unavailable,
        ProviderError::InvalidResponse,
    ];
    all_distinct_and_nonempty(&provider.iter().map(ToString::to_string).collect::<Vec<_>>());
    assert!(!RegistryError::DuplicateNamespace.to_string().is_empty());
    let controller: [ControllerError<ProviderError>; 4] = [
        ControllerError::UnknownProvider,
        ControllerError::LeaseLost,
        ControllerError::Provider(ProviderError::Unavailable),
        ControllerError::Ledger(ProviderError::Unavailable),
    ];
    all_distinct_and_nonempty(
        &controller
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    );
}

/// Identity displays are their exact spellings.
#[test]
fn identities_display_their_spellings() {
    assert_eq!(
        ProviderNamespace::new("github".to_owned())
            .unwrap()
            .to_string(),
        "github"
    );
    assert_eq!(
        OpaqueId::new("delivery/1".to_owned()).unwrap().to_string(),
        "delivery/1"
    );
}

/// The system clock reads a present-day instant: after 2020, before 2100.
#[test]
fn the_system_clock_reads_the_present() {
    let now = amiss_controller::ControllerClock::now_unix_millis(&SystemClock).unwrap();
    assert!(now > 1_577_836_800_000, "{now}");
    assert!(now < 4_102_444_800_000, "{now}");
}
