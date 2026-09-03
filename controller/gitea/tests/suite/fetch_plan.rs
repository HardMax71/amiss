#![expect(
    clippy::unwrap_used,
    reason = "fixed provider identities and constraints must fail loudly"
)]

use std::sync::Arc;

use amiss_controller::{
    ChangeId, ChangeLocator, ControllerEvaluationId, DeliveryId, DeliveryIdentity, IntegrationId,
    OidPair, PolicyControls, ProviderIdentity, ProviderInstance, ProviderNamespace,
    ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, RunIdentity, RunRefs, RunRequest,
    check_binding, check_plan,
};
use amiss_controller_gitea::{GiteaPlanError, gitea_fetch_plan};
use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile, parse_execution_constraint};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

const RUN_DOMAIN: &str = "amiss/controller-gitea-family-pull-request-v1";

#[test]
fn projects_exact_fetches_for_gitea_and_forgejo() {
    for namespace in ["gitea", "forgejo"] {
        let plan = gitea_fetch_plan(&request(namespace)).unwrap();
        assert_eq!(plan.integration_id, 77);
        assert_eq!(plan.repository_url, "https://forge.example/acme/widget.git");
        assert_eq!(plan.repository_oids, [oid('a'), oid('b')]);
        assert_eq!(
            plan.action_url,
            "https://forge.example/controller/amiss.git"
        );
        assert_eq!(plan.action_oid, oid('e'));
    }
}

#[test]
fn rejects_wrong_host_identity_change_and_object_format() {
    let mut wrong_host = request("gitea");
    wrong_host.run.change.repository = RepositoryIdentity::new(
        "forge.example@attacker.invalid".to_owned(),
        "acme".to_owned(),
        "widget".to_owned(),
    )
    .unwrap();
    assert_eq!(
        gitea_fetch_plan(&wrong_host),
        Err(GiteaPlanError::InvalidRequest)
    );

    let mut wrong_identity = request("gitea");
    wrong_identity.run.change.repository = repository("other", "widget");
    assert_eq!(
        gitea_fetch_plan(&wrong_identity),
        Err(GiteaPlanError::InvalidRequest)
    );

    let mut wrong_change = request("forgejo");
    wrong_change.run.change.change = ChangeId::new("pull/42".to_owned()).unwrap();
    assert_eq!(
        gitea_fetch_plan(&wrong_change),
        Err(GiteaPlanError::InvalidRequest)
    );

    let mut wrong_action_host = request("forgejo");
    replace_action_repository(
        &mut wrong_action_host,
        RepositoryIdentity::new(
            "other.example".to_owned(),
            "controller".to_owned(),
            "amiss".to_owned(),
        )
        .unwrap(),
    );
    assert_eq!(
        gitea_fetch_plan(&wrong_action_host),
        Err(GiteaPlanError::InvalidRequest)
    );

    let mut wrong_format = request("gitea");
    wrong_format.run.object_format = ObjectFormat::Sha256;
    assert_eq!(
        gitea_fetch_plan(&wrong_format),
        Err(GiteaPlanError::InvalidRequest)
    );
}

#[test]
fn tree_claims_do_not_change_the_provider_fetch_plan() {
    let exact = gitea_fetch_plan(&request("forgejo")).unwrap();
    let mut wrong_tree = request("forgejo");
    wrong_tree.run.trees.candidate = oid('f');

    assert_eq!(gitea_fetch_plan(&wrong_tree).unwrap(), exact);
}

fn request(namespace: &str) -> RunRequest {
    let provider = ProviderIdentity {
        namespace: ProviderNamespace::new(namespace.to_owned()).unwrap(),
        instance: ProviderInstance::new("forge.example".to_owned()).unwrap(),
    };
    let repository = repository("acme", "widget");
    let change = ChangeLocator {
        provider: provider.clone(),
        repository,
        change: ChangeId::new("repository/101/pull/4201/number/42".to_owned()).unwrap(),
    };
    let integration = IntegrationId::new("77".to_owned()).unwrap();
    let refs = RunRefs {
        forge: ForgeDialect::Gitea,
        candidate: branch("topic"),
        target: branch("main"),
        default_branch: branch("main"),
    };
    let candidate = oid('b');
    let provider_run = provider_run(
        &integration,
        &change,
        &candidate,
        &refs.candidate,
        &refs.target,
    );
    let plan =
        Arc::new(check_plan(Profile::Enforce, PolicyControls::default(), execution()).unwrap());
    RunRequest {
        delivery: DeliveryIdentity {
            provider,
            integration,
            delivery: DeliveryId::new("signed-body".to_owned()).unwrap(),
        },
        provider_run,
        evaluation_id: ControllerEvaluationId::new("evaluation/1".to_owned()).unwrap(),
        check: check_binding(&plan).unwrap(),
        plan,
        run: RunIdentity::new(
            change,
            refs,
            ObjectFormat::Sha1,
            OidPair {
                base: oid('a'),
                candidate,
            },
            OidPair {
                base: oid('c'),
                candidate: oid('d'),
            },
        )
        .unwrap(),
    }
}

fn execution() -> ExecutionConstraintDescriptor {
    let mut descriptor = parse_execution_constraint(include_bytes!(
        "../../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    descriptor.action_repository = repository("controller", "amiss");
    descriptor.action_object_format = ObjectFormat::Sha1;
    descriptor.action_commit_oid = oid('e');
    descriptor.action_tree_oid = oid('f');
    descriptor
}

fn provider_run(
    reviewer: &IntegrationId,
    change: &ChangeLocator,
    candidate: &Oid,
    candidate_ref: &BranchRef,
    target_ref: &BranchRef,
) -> ProviderRunIdentity {
    let fields = serde_json::to_vec(&[
        reviewer.as_str(),
        change.provider.namespace.as_str(),
        change.repository.host(),
        change.repository.owner(),
        change.repository.name(),
        change.change.as_str(),
        candidate.as_str(),
        candidate_ref.as_str(),
        target_ref.as_str(),
    ])
    .unwrap();
    ProviderRunIdentity::new(
        ProviderRunId::new(format!("pr:{}", hb(RUN_DOMAIN, &fields))).unwrap(),
        ProviderRunAttempt::new(1).unwrap(),
        ObjectFormat::Sha1,
        candidate.clone(),
    )
    .unwrap()
}

fn repository(owner: &str, name: &str) -> RepositoryIdentity {
    RepositoryIdentity::new(
        "forge.example".to_owned(),
        owner.to_owned(),
        name.to_owned(),
    )
    .unwrap()
}

fn branch(name: &str) -> BranchRef {
    BranchRef::new(format!("refs/heads/{name}")).unwrap()
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}

fn rebound(mut request: RunRequest) -> RunRequest {
    request.provider_run = provider_run(
        &request.delivery.integration,
        &request.run.change,
        &request.run.commits.candidate,
        &request.run.refs.candidate,
        &request.run.refs.target,
    );
    request
}

fn replace_action_repository(request: &mut RunRequest, repository: RepositoryIdentity) {
    Arc::make_mut(&mut request.plan).execution.action_repository = repository;
}

/// The repository identity has three separate demands, and a request that
/// breaks one of them breaks it alone.
#[test]
fn each_demand_on_a_repository_identity_refuses_by_itself() {
    let mut nested = request("gitea");
    nested.run.change.repository = repository("group/sub", "widget");
    assert_eq!(
        gitea_fetch_plan(&rebound(nested)),
        Err(GiteaPlanError::InvalidRequest),
        "a nested owner is a GitLab group path, not a Gitea owner"
    );

    assert!(
        RepositoryIdentity::new(
            "forge.example".to_owned(),
            "Acme".to_owned(),
            "widget".to_owned(),
        )
        .is_none(),
        "a non-canonical owner never becomes a repository identity"
    );

    let mut underscored = request("gitea");
    let instance = ProviderInstance::new("forge_example".to_owned()).unwrap();
    underscored.delivery.provider.instance = instance.clone();
    underscored.run.change.provider.instance = instance;
    underscored.run.change.repository = RepositoryIdentity::new(
        "forge_example".to_owned(),
        "acme".to_owned(),
        "widget".to_owned(),
    )
    .unwrap();
    replace_action_repository(
        &mut underscored,
        RepositoryIdentity::new(
            "forge_example".to_owned(),
            "controller".to_owned(),
            "amiss".to_owned(),
        )
        .unwrap(),
    );
    assert_eq!(
        gitea_fetch_plan(&rebound(underscored)),
        Err(GiteaPlanError::InvalidRequest),
        "a host no forge label may spell, agreed on by every party"
    );
}

/// Every object the plan carries or names is exactly a SHA-1, trees
/// included, though the plan itself never states them.
#[test]
fn an_object_outside_the_sha1_grammar_refuses_the_plan() {
    let mut wider = request("forgejo");
    wider.run.trees.candidate = Oid::new(ObjectFormat::Sha256, "d".repeat(64)).unwrap();
    assert_eq!(
        gitea_fetch_plan(&wider),
        Err(GiteaPlanError::InvalidRequest),
        "a tree the plan never states is still an object it answers for"
    );
}

/// The one refusal this crate can state says what it means.
#[test]
fn the_refusal_states_what_it_refused() {
    assert_eq!(
        GiteaPlanError::InvalidRequest.to_string(),
        "the Gitea-family acquisition request is inconsistent"
    );
}
