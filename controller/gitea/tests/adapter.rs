#![expect(
    clippy::unwrap_used,
    reason = "fixed provider payloads and protocol identities must fail loudly"
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, ChangeState, CheckBinding, CheckConclusion,
    ControllerClock, ControllerEvaluationId, DeliveryHeader, DeliveryRoute, GiteaWebhook,
    IngressLimits, IngressPolicy, OidPair, ProviderAdapter, ProviderError, ProviderIdentity,
    ProviderInstance, ProviderNamespace, Publication, ReplayIdentity, ReplayWindow, RunIdentity,
    RunRefs, SignedTimePolicy, UntrustedDelivery, WebhookKey, WebhookKeyring,
};
use amiss_controller_gitea::{
    DedicatedReviewer, GiteaApi, GiteaPullRequest, GiteaPullRequestAdapter,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid};
use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::Sha256;

const NOW: i64 = 1_800_000_000_000;
const SECRET: &[u8] = b"gitea-family-webhook-secret";
const BODY: &[u8] = br#"{
  "action":"opened",
  "repository":{
    "id":101,
    "name":"widget",
    "full_name":"Acme/widget",
    "owner":{"id":12,"login":"Acme"}
  },
  "number":42,
  "pull_request":{
    "id":4201,
    "number":42,
    "head":{
      "sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "ref":"topic",
      "repo_id":202,
      "repo":{"id":202,"name":"widget","full_name":"contributor/widget","owner":{"id":13,"login":"contributor"}}
    },
    "base":{
      "sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "ref":"main",
      "repo_id":101,
      "repo":{"id":101,"name":"widget","full_name":"Acme/widget","owner":{"id":12,"login":"Acme"}}
    }
  }
}"#;

type HmacSha256 = Hmac<Sha256>;

struct FixedClock;

impl ControllerClock for FixedClock {
    fn now_unix_millis(&self) -> Option<i64> {
        Some(NOW)
    }
}

#[derive(Clone)]
struct FakeApi {
    snapshot: ChangeSnapshot,
    publications: Arc<Mutex<Vec<Publication>>>,
}

impl GiteaApi for FakeApi {
    fn refresh(
        &self,
        _pull_request: GiteaPullRequest<'_>,
    ) -> Result<ChangeSnapshot, ProviderError> {
        Ok(self.snapshot.clone())
    }

    fn publish(
        &self,
        _pull_request: GiteaPullRequest<'_>,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        self.publications.lock().unwrap().push(publication.clone());
        Ok(())
    }
}

#[test]
fn both_supported_namespaces_bind_the_same_signed_facts() {
    for namespace in ["gitea", "forgejo"] {
        let adapter = adapter(namespace, dummy_snapshot(namespace));
        let verified = authenticated(&adapter, BODY, provider(namespace)).unwrap();
        let delivery = verified.delivery();
        assert_eq!(adapter.namespace().as_str(), namespace);
        assert_eq!(delivery.identity.integration.as_str(), "77");
        assert_eq!(delivery.change.repository.owner, "acme");
        assert_eq!(delivery.change.repository.name, "widget");
        assert_eq!(
            delivery.change.change.as_str(),
            "repository/101/pull/4201/number/42"
        );
        assert_eq!(
            delivery.provider_run.candidate_commit.as_str(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        assert_eq!(verified.replay(), &ReplayIdentity::ExactBody);
    }
    assert!(
        GiteaPullRequestAdapter::new(
            provider("compatible-fork"),
            reviewer(),
            webhook(),
            FakeApi {
                snapshot: dummy_snapshot("compatible-fork"),
                publications: Arc::new(Mutex::new(Vec::new())),
            },
        )
        .is_some()
    );
}

#[test]
fn only_run_defining_actions_are_accepted() {
    let adapter = adapter("gitea", dummy_snapshot("gitea"));
    let original = authenticated(&adapter, BODY, provider("gitea"))
        .unwrap()
        .delivery()
        .clone();
    for action in ["reopened", "synchronized"] {
        let body = replaced_once(
            BODY,
            r#""action":"opened""#,
            &format!(r#""action":"{action}""#),
        );
        let delivery = authenticated(&adapter, &body, provider("gitea"))
            .unwrap()
            .delivery()
            .clone();
        assert_eq!(delivery.provider_run, original.provider_run);
        assert_ne!(delivery.identity.delivery, original.identity.delivery);
    }
    let edited = replaced_once(
        BODY,
        r#""action":"opened","#,
        r#""action":"edited","changes":{"ref":{"from":"develop"}},"#,
    );
    assert!(authenticated(&adapter, &edited, provider("gitea")).is_ok());
    for action in ["closed", "labeled", "edited"] {
        let body = replaced_once(
            BODY,
            r#""action":"opened""#,
            &format!(r#""action":"{action}""#),
        );
        assert_eq!(
            authenticated(&adapter, &body, provider("gitea")),
            Err(ProviderError::Authentication)
        );
    }
}

#[test]
fn rejects_wrong_identity_treeish_facts_and_body_tampering() {
    let adapter = adapter("gitea", dummy_snapshot("gitea"));
    let cases = [
        replaced_once(BODY, r#""id":101"#, r#""id":0"#),
        replaced_once(BODY, r#""id":4201"#, r#""id":0"#),
        replaced_once(BODY, r#""number":42"#, r#""number":0"#),
        replaced_once(BODY, r#""repo_id":101"#, r#""repo_id":102"#),
        replaced_once(BODY, "Acme/widget", "Acme/other"),
        replaced_once(
            BODY,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
        ),
        replaced_once(BODY, r#""ref":"topic""#, r#""ref":"bad ref""#),
        replaced_once(BODY, r#""ref":"main""#, r#""ref":"bad..ref""#),
    ];
    for body in cases {
        assert_eq!(
            authenticated(&adapter, &body, provider("gitea")),
            Err(ProviderError::Authentication)
        );
    }

    let tampered = replaced_once(BODY, r#""number":42"#, r#""number":43"#);
    assert_eq!(
        authenticate_with_signature(
            &adapter,
            &tampered,
            &signature(BODY),
            provider("gitea"),
            SignedTimePolicy::ReplayOnly,
        ),
        Err(ProviderError::Authentication)
    );
    assert_eq!(
        authenticated(&adapter, BODY, provider("forgejo")),
        Err(ProviderError::Authentication)
    );
    assert_eq!(
        authenticate_with_signature(
            &adapter,
            BODY,
            &signature(BODY),
            provider("gitea"),
            SignedTimePolicy::Required(Duration::from_mins(5)),
        ),
        Err(ProviderError::Authentication)
    );
}

#[test]
fn refresh_and_publication_remain_event_bound() {
    let seed = adapter("gitea", dummy_snapshot("gitea"));
    let delivery = authenticated(&seed, BODY, provider("gitea"))
        .unwrap()
        .delivery()
        .clone();
    let exact = snapshot(&delivery, "topic", "main");
    let publications = Arc::new(Mutex::new(Vec::new()));
    let exact_adapter = adapter_with("gitea", exact.clone(), Arc::clone(&publications));
    assert_eq!(exact_adapter.refresh(&delivery), Ok(exact.clone()));

    let drifted = snapshot(&delivery, "changed", "main");
    let drifted_adapter = adapter("gitea", drifted.clone());
    assert_eq!(
        drifted_adapter.refresh(&delivery),
        Ok(ChangeSnapshot {
            state: ChangeState::Superseded,
            run: drifted.run,
            gate_commit: drifted.gate_commit,
        })
    );

    let valid = publication(&delivery, exact.run);
    assert_eq!(exact_adapter.publish(&delivery, &valid), Ok(()));
    assert_eq!(publications.lock().unwrap().len(), 1);

    let invalid = publication(&delivery, snapshot(&delivery, "changed", "main").run);
    assert_eq!(
        exact_adapter.publish(&delivery, &invalid),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(publications.lock().unwrap().len(), 1);
}

fn adapter(namespace: &str, snapshot: ChangeSnapshot) -> GiteaPullRequestAdapter<FakeApi> {
    adapter_with(namespace, snapshot, Arc::new(Mutex::new(Vec::new())))
}

fn adapter_with(
    namespace: &str,
    snapshot: ChangeSnapshot,
    publications: Arc<Mutex<Vec<Publication>>>,
) -> GiteaPullRequestAdapter<FakeApi> {
    GiteaPullRequestAdapter::new(
        provider(namespace),
        reviewer(),
        webhook(),
        FakeApi {
            snapshot,
            publications,
        },
    )
    .unwrap()
}

fn reviewer() -> DedicatedReviewer {
    DedicatedReviewer::new(77, "amiss-controller".to_owned()).unwrap()
}

fn webhook() -> GiteaWebhook {
    let key = WebhookKey::new(
        amiss_controller::OpaqueId::new("current".to_owned()).unwrap(),
        SECRET.to_vec(),
        0,
        None,
    )
    .unwrap();
    GiteaWebhook::new(
        WebhookKeyring::new(
            amiss_controller::OpaqueId::new("gitea-webhooks".to_owned()).unwrap(),
            vec![key],
        )
        .unwrap(),
    )
}

fn authenticated(
    adapter: &GiteaPullRequestAdapter<FakeApi>,
    body: &[u8],
    route_provider: ProviderIdentity,
) -> Result<amiss_controller::VerifiedDelivery, ProviderError> {
    authenticate_with_signature(
        adapter,
        body,
        &signature(body),
        route_provider,
        SignedTimePolicy::ReplayOnly,
    )
}

fn authenticate_with_signature(
    adapter: &GiteaPullRequestAdapter<FakeApi>,
    body: &[u8],
    signature: &[u8],
    route_provider: ProviderIdentity,
    signed_time: SignedTimePolicy,
) -> Result<amiss_controller::VerifiedDelivery, ProviderError> {
    let signature_header = if route_provider.namespace.as_str() == "forgejo" {
        "x-forgejo-signature"
    } else {
        "x-gitea-signature"
    };
    let route = DeliveryRoute {
        provider: route_provider,
        trust_set: amiss_controller::OpaqueId::new("gitea-webhooks".to_owned()).unwrap(),
        signed_time,
    };
    let headers = [DeliveryHeader {
        name: signature_header,
        value: signature,
    }];
    let check = policy()
        .pre_auth(
            UntrustedDelivery {
                route: &route,
                received_at_unix_millis: NOW,
                headers: &headers,
                body,
            },
            &FixedClock,
        )
        .unwrap();
    adapter.authenticate(check)
}

fn policy() -> IngressPolicy {
    IngressPolicy::new(
        IngressLimits::new(1_000_000, 16, 4_096).unwrap(),
        ReplayWindow::new(Duration::from_mins(5), Duration::from_mins(1)).unwrap(),
        Duration::ZERO,
    )
    .unwrap()
}

fn signature(body: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(SECRET).unwrap();
    mac.update(body);
    hex::encode(mac.finalize().into_bytes()).into_bytes()
}

fn provider(namespace: &str) -> ProviderIdentity {
    ProviderIdentity {
        namespace: self::namespace(namespace),
        instance: ProviderInstance::new("forge.example".to_owned()).unwrap(),
    }
}

fn namespace(raw: &str) -> ProviderNamespace {
    ProviderNamespace::new(raw.to_owned()).unwrap()
}

fn snapshot(
    delivery: &AuthenticatedDelivery,
    candidate_ref: &str,
    target_ref: &str,
) -> ChangeSnapshot {
    let candidate = delivery.provider_run.candidate_commit.clone();
    ChangeSnapshot {
        state: ChangeState::Active,
        run: RunIdentity::new(
            delivery.change.clone(),
            RunRefs {
                forge: ForgeDialect::Gitea,
                candidate: BranchRef::new(format!("refs/heads/{candidate_ref}")).unwrap(),
                target: BranchRef::new(format!("refs/heads/{target_ref}")).unwrap(),
                default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            },
            ObjectFormat::Sha1,
            OidPair {
                base: oid('a'),
                candidate: candidate.clone(),
            },
            OidPair {
                base: oid('c'),
                candidate: oid('d'),
            },
        )
        .unwrap(),
        gate_commit: candidate,
    }
}

fn dummy_snapshot(namespace: &str) -> ChangeSnapshot {
    let provider = provider(namespace);
    let repository = amiss_wire::model::RepositoryIdentity::new(
        "forge.example".to_owned(),
        "acme".to_owned(),
        "widget".to_owned(),
    )
    .unwrap();
    let change = amiss_controller::ChangeLocator {
        provider,
        repository,
        change: amiss_controller::OpaqueId::new("42".to_owned()).unwrap(),
    };
    ChangeSnapshot {
        state: ChangeState::Active,
        run: RunIdentity::new(
            change,
            RunRefs {
                forge: ForgeDialect::Gitea,
                candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
                target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
                default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
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
        .unwrap(),
        gate_commit: oid('b'),
    }
}

fn publication(delivery: &AuthenticatedDelivery, run: RunIdentity) -> Publication {
    let digest = hb("amiss/controller-gitea-test", b"fixture");
    Publication {
        provider_run: delivery.provider_run.clone(),
        evaluation_id: ControllerEvaluationId::new("evaluation-1".to_owned()).unwrap(),
        check: CheckBinding {
            plan_digest: digest,
            required_status_name: "amiss".to_owned(),
            execution_constraint_digest: digest,
        },
        gate_commit: delivery.provider_run.candidate_commit.clone(),
        run,
        conclusion: CheckConclusion::Pass,
        report: Some(br#"{"schema":"amiss/report"}"#.to_vec()),
    }
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}

fn replaced_once(source: &[u8], from: &str, to: &str) -> Vec<u8> {
    String::from_utf8(source.to_vec())
        .unwrap()
        .replacen(from, to, 1)
        .into_bytes()
}
