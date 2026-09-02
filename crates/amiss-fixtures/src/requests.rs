#![expect(
    clippy::expect_used,
    reason = "a fixture that cannot build its own inputs has no useful failure to return"
)]

use std::fs;
use std::path::{Path, PathBuf};

use amiss_wire::controls::ExecutionConstraintDescriptor;
use amiss_wire::model::Oid;
use amiss_wire::requests::{
    ControlsRequest, EvaluationRequest, RequestTrust, SnapshotRequest, SuppliedControl,
};

/// A candidate the published evaluation example omits, since the example is
/// written for the shape a controller sends before it resolves one.
const CANDIDATE: &str = "5cf1a0f2b34d7e9a1c6b8d0e2f4a6c8e0b2d4f60";

/// One request triple that clears every agreement check in the bootstrap's
/// input path, built from the published examples so it stays canonical.
///
/// A test varies exactly one field and writes again. Everything else keeps
/// agreeing, so the refusal a case observes names the field it broke.
pub struct SealedRequests {
    pub evaluation: EvaluationRequest,
    pub snapshot: SnapshotRequest,
    pub controls: ControlsRequest,
    pub constraint: ExecutionConstraintDescriptor,
}

/// Where [`SealedRequests::write`] put each document.
pub struct RequestPaths {
    pub evaluation: PathBuf,
    pub snapshot: PathBuf,
    pub controls: PathBuf,
    pub constraint: PathBuf,
}

impl SealedRequests {
    /// Takes the constraint the action tree already satisfies, since the
    /// wrapper validates that tree before it ever reads a request.
    ///
    /// # Panics
    ///
    /// A published example is unreadable or no longer parses.
    #[must_use]
    pub fn new(constraint: ExecutionConstraintDescriptor) -> Self {
        let mut evaluation = EvaluationRequest::parse(&example("scanner-evaluation-request.json"))
            .expect("the published evaluation request parses");
        evaluation.candidate_commit = Some(
            Oid::new(evaluation.object_format, CANDIDATE.to_owned()).expect("a valid candidate"),
        );
        let mut controls = ControlsRequest::parse(&example("scanner-controls-request.json"))
            .expect("the published controls request parses");
        controls.execution_constraint = Some(SuppliedControl {
            value: serde_json::from_slice(
                &constraint
                    .canonical_bytes()
                    .expect("the constraint serializes"),
            )
            .expect("canonical bytes parse"),
            expected_digest: constraint.digest(),
            trust_source: RequestTrust::ExternalRequiredCheck,
        });
        Self {
            evaluation,
            snapshot: SnapshotRequest::parse(&example("scanner-snapshot-request.json"))
                .expect("the published snapshot request parses"),
            controls,
            constraint,
        }
    }

    /// Writes all four documents under `root` in canonical form.
    ///
    /// # Panics
    ///
    /// A varied document no longer serializes, or `root` is not writable.
    pub fn write(&self, root: &Path) -> RequestPaths {
        let paths = RequestPaths {
            evaluation: root.join("evaluation.json"),
            snapshot: root.join("snapshot.json"),
            controls: root.join("controls.json"),
            constraint: root.join("constraint.json"),
        };
        put(
            &paths.evaluation,
            &self
                .evaluation
                .canonical_bytes()
                .expect("evaluation serializes"),
        );
        put(
            &paths.snapshot,
            &self
                .snapshot
                .canonical_bytes()
                .expect("snapshot serializes"),
        );
        put(
            &paths.controls,
            &self
                .controls
                .canonical_bytes()
                .expect("controls serializes"),
        );
        put(
            &paths.constraint,
            &self
                .constraint
                .canonical_bytes()
                .expect("constraint serializes"),
        );
        paths
    }
}

/// Rewrites one already-written document so it still parses and is no longer
/// canonical, which is the only way to reach the canonical-form refusal.
///
/// # Panics
///
/// The document was never written.
pub fn indent(path: &Path) {
    let canonical = fs::read(path).expect("the document was written");
    let mut indented = Vec::with_capacity(canonical.len().saturating_add(1));
    indented.push(b' ');
    indented.extend_from_slice(&canonical);
    put(path, &indented);
}

fn put(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).expect("the fixture root is writable");
}

fn example(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/examples")
            .join(name),
    )
    .expect("the published example is readable")
}
