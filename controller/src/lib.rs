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
mod bounded_json;
mod clock;
mod external;
pub mod feedback;
mod file_ledger;
mod identity;
mod ingress;
mod intersphinx;
mod mdbook;
mod orchestration;
mod plans;
mod provider;
mod publication_audit;
mod relation_audit;
mod relations;
mod spelling;
mod webhook;

pub use acquiring_runner::{AcquiringRunner, Acquisition, AcquisitionTarget};
pub use acquisition::{AcquireError, AcquiredRoots, verify_acquired};
pub use amiss_bootstrap::BOOTSTRAP_EXECUTABLE_BYTES;
pub use artifacts::{
    ArtifactAuditBundle, ArtifactAuditDigests, ArtifactAuditReference, ArtifactBundle,
    ArtifactCleanup, ArtifactComponent, ArtifactError, ArtifactReference, ArtifactStoreConfig,
    FileArtifactStore, MAX_ARTIFACT_BYTES, MAX_ARTIFACT_RECORD_BYTES, MAX_ARTIFACT_RECORDS,
    MAX_ARTIFACT_RETENTION, artifact_route,
};
pub use bootstrap_job::{
    AcquiredControl, AcquiredSemanticTemplate, BootstrapJob, BootstrapJobError, BootstrapJobInput,
    BoundSemanticEvidence, CheckBinding, CheckPlan, ExternalPolicy,
    MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES, MAX_WORKFLOW_ARTIFACT_FILE_BYTES, PolicyControls,
    SEMANTIC_INPUT_ARTIFACT_BYTES, SemanticEvidenceExpectation, SemanticEvidenceTemplate,
    WorkflowArtifactExpectation, bind_semantic_evidence, bootstrap_job, check_binding, check_plan,
};
pub use bootstrap_result::{BootstrapTermination, classify_bootstrap_result};
pub use bootstrap_runner::{BootstrapRun, run_bootstrap};
pub use bounded_json::decode_bounded_json;
pub use clock::{ControllerClock, SystemClock};
pub use external::{
    ForgeEvidence, ForgePresence, ForgeProducer, ForgeRefFamily, ForgeTail, ForgeTarget,
    ForgeVisibility, forge_evidence, forge_repository_evidence,
};
pub use file_ledger::{
    FileLedger, FileLedgerCleanup, FileLedgerConfig, FileLedgerError, FileLedgerRoot,
};
pub use identity::{
    ChangeId, ChangeLocator, ControllerEvaluationId, DeliveryId, DeliveryIdentity, IntegrationId,
    OpaqueId, ProviderIdentity, ProviderInstance, ProviderNamespace, ProviderRunAttempt,
    ProviderRunId, ProviderRunIdentity,
};
pub use ingress::{
    AcceptedDelivery, DeliveryHeader, DeliveryRoute, IngressCheck, IngressError, IngressLimits,
    IngressPolicy, ReplayIdentity, ReplayWindow, SignedTimePolicy, TrustAnchorId, TrustSetId,
    UntrustedDelivery, VerifiedDelivery,
};
pub use intersphinx::{
    INTERSPHINX_INVENTORY_BYTES, IntersphinxError, IntersphinxInventory, intersphinx_evidence,
};
pub use mdbook::{
    MDBOOK_HTML_BYTES, MDBOOK_RENDER_CONTEXT_BYTES, MdBookEvidenceError, SiteBuildContext,
    mdbook_site_evidence, mdbook_site_expectation,
};
pub use orchestration::{
    ChangeSnapshot, ChangeState, CheckConclusion, Controller, ControllerError, DeliveryClaim,
    DeliveryLease, DeliveryLedger, Evaluation, ExternalSink, ExternalTally, HandleOutcome,
    HeartbeatOutcome, LeaseCompletion, LeaseFence, LeaseRenewal, OidPair, Publication, RunFailure,
    RunHeartbeat, RunIdentity, RunRefs, RunRequest, Runner, RunnerOutcome, StageOutcome,
    StagedPublication,
};
pub use plans::{PlanError, PlanRegistry, PlanScope, ResolvedPlan, register_plan, resolve_plan};
pub use provider::{
    AdapterRegistry, AuthenticatedDelivery, ForgeFact, ForgeNegative, OperationDeadline,
    ProviderAdapter, ProviderError, RegistryError,
};
pub use publication_audit::{
    PublicationAuditBundle, PublicationAuditDigests, validate_publication_audit,
};
pub use relation_audit::{RelationAuditBundle, RelationAuditDigests, validate_relation_audit};
pub use relations::{
    FileRelationScheduleStore, PendingRelation, RELATION_REGISTRY_LIMIT,
    RELATION_SCHEDULE_BINDING_LIMIT, RelationAcquiredRoot, RelationAcquisitionError,
    RelationAdmission, RelationCredentialError, RelationCredentialRoute, RelationCredentialRouter,
    RelationLimits, RelationLookupError, RelationPlan, RelationRegistry, RelationRegistryError,
    RelationScheduleError, RelationScheduleStoreError, RelationStatusDeliveryClaim,
    RelationStatusDestination, RelationStatusError, RelationStatusPublication,
    RelationStatusRecord, RelationStatusTarget, RelationStatusTargets, RelationSubject,
    RelationSubjectHead, RelationSubjectTransition, RelationTransition, TriggeredRelation,
    complete_relation_status, relation_authority, relation_credential_router, relation_registry,
    relation_status_publication, relation_status_targets, relation_transition,
    relations_for_delivery, schedule_relation, stage_relation_status, verify_relation_acquired,
    verify_relation_plan,
};
pub use spelling::{ref_span, spelled_segments};
pub use webhook::{
    GitHubWebhook, GitLabWebhook, GiteaWebhook, SignedRequestProof, WebhookError, WebhookKey,
    WebhookKeyring, WebhookKeyringError, WebhookProof,
};
