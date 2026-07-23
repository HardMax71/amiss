#![expect(
    clippy::unwrap_used,
    reason = "fixed provider identities and constraints must fail loudly"
)]

mod support;

use std::sync::Arc;

use amiss_controller::{ChangeId, DeliveryId, ProviderInstance, ProviderRunId};
use amiss_controller_gitlab::{GitLabMergeTrainAdapter, gitlab_fetch_plan};
use amiss_wire::model::{ForgeDialect, ObjectFormat};

use support::identity::now_seconds;
use support::oidc::{accept, claims, oidc};
use support::plan::run_request;
use support::refresh::valid_refresh;

use crate::adapter_api::StaticApi;

const BODY: &[u8] = br#"{"merge_request_iid":42}"#;

mod adapter_api {
    use amiss_controller::ProviderError;
    use amiss_controller_gitlab::{GitLabApi, GitLabRefresh, GitLabRefreshQuery};

    #[derive(Clone)]
    pub(crate) struct StaticApi(pub GitLabRefresh);

    impl GitLabApi for StaticApi {
        fn refresh(&self, _query: &GitLabRefreshQuery) -> Result<GitLabRefresh, ProviderError> {
            Ok(self.0.clone())
        }
    }
}

#[test]
fn exact_fetch_plan_contains_no_credential_or_moving_ref() {
    let (delivery, snapshot) = fixture();
    let request = run_request(&delivery, &snapshot);
    let plan = gitlab_fetch_plan(&request).unwrap();

    assert_eq!(plan.project_id, 101);
    assert_eq!(plan.pipeline_id, 202);
    assert_eq!(plan.job_id, 303);
    assert_eq!(
        plan.repository_url,
        "https://gitlab.example/acme/widget.git"
    );
    assert_eq!(
        plan.action_url,
        "https://gitlab.example/hardmax71/amiss.git"
    );
    assert_eq!(
        plan.repository_oids,
        [snapshot.run.commits.base, snapshot.run.commits.candidate]
    );
    assert_eq!(plan.action_oid, request.plan.execution.action_commit_oid);
}

#[test]
fn host_change_run_delivery_and_format_substitutions_are_rejected() {
    let (delivery, snapshot) = fixture();
    let request = run_request(&delivery, &snapshot);

    let mut wrong_host = request.clone();
    wrong_host.run.change.repository.host = "gitlab.example@attacker.invalid".to_owned();
    let mut wrong_owner = request.clone();
    wrong_owner.run.change.repository.owner = "Acme".to_owned();
    let mut wrong_change = request.clone();
    wrong_change.run.change.change = ChangeId::new("merge-request/42".to_owned()).unwrap();
    let mut wrong_run = request.clone();
    wrong_run.provider_run.run_id = ProviderRunId::new("pipeline/0/job/303".to_owned()).unwrap();
    let mut wrong_delivery = request.clone();
    wrong_delivery.delivery.delivery = DeliveryId::new("signed-body".to_owned()).unwrap();
    let mut wrong_forge = request.clone();
    wrong_forge.run.refs.forge = ForgeDialect::Github;
    let mut wrong_format = request.clone();
    wrong_format.run.object_format = ObjectFormat::Sha256;
    let mut wrong_instance = request.clone();
    wrong_instance.delivery.provider.instance =
        ProviderInstance::new("other.example".to_owned()).unwrap();
    let mut wrong_action = request;
    Arc::make_mut(&mut wrong_action.plan)
        .execution
        .action_repository
        .host = "other.example".to_owned();

    for changed in [
        wrong_host,
        wrong_owner,
        wrong_change,
        wrong_run,
        wrong_delivery,
        wrong_forge,
        wrong_format,
        wrong_instance,
        wrong_action,
    ] {
        assert!(gitlab_fetch_plan(&changed).is_err());
    }
}

fn fixture() -> (
    amiss_controller::AuthenticatedDelivery,
    amiss_controller::ChangeSnapshot,
) {
    use amiss_controller::ProviderAdapter as _;

    let now = now_seconds();
    let source = oidc();
    let delivery = accept(&source, &claims(now), BODY, now)
        .unwrap()
        .delivery()
        .clone();
    let refresh = valid_refresh(&delivery);
    let adapter = GitLabMergeTrainAdapter::new(source, StaticApi(refresh));
    let snapshot = adapter.refresh(&delivery).unwrap();
    (delivery, snapshot)
}
