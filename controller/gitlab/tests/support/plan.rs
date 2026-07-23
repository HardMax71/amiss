use std::sync::Arc;

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, CheckPlan, ControllerEvaluationId, PlanScope,
    PolicyControls, RunRequest, check_binding, check_plan,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, ExecutionConstraintInput, Profile};
use amiss_wire::model::{ObjectFormat, RepositoryIdentity};

use super::identity::{HOST, oid};

pub fn run_request(delivery: &AuthenticatedDelivery, snapshot: &ChangeSnapshot) -> RunRequest {
    let plan = plan();
    RunRequest {
        delivery: delivery.identity.clone(),
        provider_run: delivery.provider_run.clone(),
        evaluation_id: ControllerEvaluationId::new("evaluation/1".to_owned()).unwrap(),
        check: check_binding(&plan).unwrap(),
        plan,
        run: snapshot.run.clone(),
    }
}

pub fn plan() -> Arc<CheckPlan> {
    Arc::new(check_plan(Profile::Enforce, PolicyControls::default(), execution()).unwrap())
}

pub fn scope(delivery: &AuthenticatedDelivery) -> PlanScope {
    PlanScope {
        provider: delivery.identity.provider.clone(),
        integration: delivery.identity.integration.clone(),
        repository: delivery.change.repository.clone(),
    }
}

fn execution() -> ExecutionConstraintDescriptor {
    let template = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    let mut input = ExecutionConstraintInput::from(&template);
    input.action_repository =
        RepositoryIdentity::new(HOST.to_owned(), "hardmax71".to_owned(), "amiss".to_owned())
            .unwrap();
    input.action_object_format = ObjectFormat::Sha1;
    input.action_commit_oid = oid('e');
    input.action_tree_oid = oid('f');
    ExecutionConstraintDescriptor::new(input).unwrap()
}
