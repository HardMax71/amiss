#![forbid(unsafe_code)]

mod acquiring_runner;
mod acquisition;
mod artifacts;
#[doc(hidden)]
pub mod atomic_write_recovery;
mod audit_report;
mod bootstrap_job;
mod bootstrap_result;
mod bootstrap_runner;
mod clock;
mod external;
pub mod feedback;
mod file_ledger;
#[doc(hidden)]
pub mod frame;
mod identity;
mod ingress;
mod intersphinx;
mod locale_audit;
mod mdbook;
mod model;
mod orchestration;
mod plans;
mod provider;
mod publication_audit;
mod relation_audit;
mod relation_plan;
mod relations;
mod response_body;
mod semantic_artifact;
mod spelling;
mod webhook;

pub use acquiring_runner::{AcquiringRunner, Acquisition, AcquisitionTarget};
pub use acquisition::{AcquireError, AcquiredRoots, verify_acquired};
pub use amiss_bootstrap::BOOTSTRAP_EXECUTABLE_BYTES;
pub use artifacts::{
    ArtifactAuditBundle, ArtifactAuditDigests, ArtifactAuditReference, ArtifactBundle,
    ArtifactCleanup, ArtifactComponent, ArtifactReference, ArtifactStoreConfig, ExternalTally,
    FileArtifactStore, MAX_ARTIFACT_BYTES, MAX_ARTIFACT_RECORD_BYTES, MAX_ARTIFACT_RECORDS,
    MAX_ARTIFACT_RETENTION, artifact_route,
};
pub use audit_report::ArtifactError;
pub use bootstrap_job::{
    AcquiredSemanticTemplate, BootstrapJob, BootstrapJobError, BootstrapJobInput,
    BoundSemanticEvidence, CheckBinding, CheckPlan, ExternalPolicy,
    MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES, MAX_WORKFLOW_ARTIFACT_FILE_BYTES, PolicyControls,
    RunRequest, SEMANTIC_INPUT_ARTIFACT_BYTES, SemanticEvidenceExpectation,
    SemanticEvidenceTemplate, WorkflowArtifactExpectation, bind_semantic_evidence, bootstrap_job,
    check_binding, check_plan,
};
pub use bootstrap_result::{BootstrapTermination, classify_bootstrap_result};
pub use bootstrap_runner::{BootstrapRun, run_bootstrap};
pub use clock::{ControllerClock, SystemClock};
pub use external::{
    ForgeEvidence, ForgePresence, ForgeProducer, ForgeRefFamily, ForgeRepository, ForgeTail,
    forge_evidence, forge_repository_evidence,
};
pub use file_ledger::{
    FileLedger, FileLedgerCleanup, FileLedgerConfig, FileLedgerError, FileLedgerRoot,
};
pub use identity::{
    AuthenticatedDelivery, Change, ChangeLocator, ChangeSnapshot, ChangeState, Delivery,
    DeliveryIdentity, MergeRequestChange, OidPair, OidcToken, OpaqueId, PipelineJob, ProviderFacts,
    ProviderIdentity, ProviderNamespace, ProviderRun, ProviderRunAttempt, ProviderRunIdentity,
    PullRequestChange, RunIdentity, RunRefs,
};
pub use ingress::{
    AcceptedDelivery, DeliveryHeader, DeliveryRoute, IngressCheck, IngressError, IngressLimits,
    IngressPolicy, ReplayIdentity, ReplayWindow, SignedTimePolicy, UntrustedDelivery,
    VerifiedDelivery,
};
pub use intersphinx::{
    INTERSPHINX_INVENTORY_BYTES, IntersphinxError, IntersphinxInventory, intersphinx_evidence,
};
pub use locale_audit::{LocaleAuditBundle, LocaleAuditDigests, validate_locale_audit};
pub use mdbook::{
    MDBOOK_HTML_BYTES, MDBOOK_RENDER_CONTEXT_BYTES, MdBookEvidenceError, SiteBuildContext,
    mdbook_site_evidence, mdbook_site_expectation,
};
pub use model::AcquiredCommit;
pub use orchestration::{
    Controller, ControllerError, DeliveryClaim, DeliveryLease, DeliveryLedger, Evaluation,
    ExternalSink, HandleOutcome, HeartbeatOutcome, LeaseCompletion, LeaseFence, LeaseRenewal,
    RunHeartbeat, Runner, RunnerOutcome, StageOutcome, StagedPublication,
};
pub use plans::{PlanError, PlanRegistry, PlanScope, ResolvedPlan, register_plan, resolve_plan};
pub use provider::{
    AdapterRegistry, CheckConclusion, ForgeFact, ForgeNegative, OperationDeadline, ProviderAdapter,
    ProviderError, Publication, RegistryError, RunFailure, provider_api_url,
};
pub use publication_audit::{
    PublicationAuditBundle, PublicationAuditDigests, validate_publication_audit,
};
pub use relation_audit::{
    RelationAuditBundle, RelationAuditDigests, relation_audit_plan, validate_relation_audit,
};
pub use relation_plan::{
    RelationAcquisitionError, RelationLimits, RelationPlan, RelationRegistryError,
    RelationStatusDestination, RelationSubject, RelationSubjectTransition, RelationTransition,
    TriggeredRelation, relation_transition,
};
pub use relations::{
    FileRelationScheduleStore, PendingRelation, RELATION_REGISTRY_LIMIT,
    RELATION_SCHEDULE_BINDING_LIMIT, RelationAcquiredRoot, RelationAdmission,
    RelationCredentialError, RelationCredentialRoute, RelationCredentialRouter,
    RelationLookupError, RelationRegistry, RelationScheduleError, RelationScheduleStoreError,
    RelationStatusDeliveryClaim, RelationStatusError, RelationStatusPublication,
    RelationStatusRecord, RelationStatusTarget, RelationStatusTargets, RelationSubjectHead,
    complete_relation_status, relation_authority, relation_credential_router, relation_registry,
    relation_status_publication, relation_status_targets, relations_for_delivery,
    schedule_relation, stage_relation_status, verify_relation_acquired, verify_relation_plan,
};
pub use response_body::read_response_body;
pub use spelling::{ref_span, spelled_segments};
pub use webhook::{
    GitHubWebhook, GitLabWebhook, GiteaWebhook, SignedRequestProof, WebhookError, WebhookKey,
    WebhookKeyring, WebhookKeyringError,
};
