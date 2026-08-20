use std::io;
use std::path::{Path, PathBuf};

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_wire::action::host_platform;
use amiss_wire::digest::hb;
use serde_json::{Value, json};
use tempfile::TempDir;

const BOOTSTRAP_BYTES: &[u8] = b"trusted bootstrap fixture";

/// The local trust inputs every provider service loads before it binds anything:
/// a bootstrap executable and an execution constraint bound to its digest.
pub struct TrustFiles {
    root: TempDir,
    pub bootstrap: PathBuf,
    pub constraint: PathBuf,
}

impl TrustFiles {
    /// # Errors
    ///
    /// Any filesystem failure, or a host this build has no platform name for.
    pub fn new(host: &str, owner: &str, name: &str) -> io::Result<Self> {
        let root = TempDir::new()?;
        let bootstrap = root.path().join("amiss-bootstrap");
        std::fs::write(&bootstrap, BOOTSTRAP_BYTES)?;
        let platform = host_platform()
            .ok_or_else(|| io::Error::other("no execution platform for this host"))?;
        let constraint = root.path().join("execution.json");
        std::fs::write(
            &constraint,
            serde_json::to_vec_pretty(&json!({
                "schema": "amiss/scanner-execution-constraint",
                "action_repository": { "host": host, "owner": owner, "name": name },
                "action_object_format": "sha1",
                "action_commit_oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "action_tree_oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "manifest_path": "release/manifest.json",
                "release_manifest_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                "selected_platform": platform.as_ref(),
                "required_status_name": "amiss / documentation assurance",
                "bootstrap_contract": "amiss-action-bootstrap",
                "bootstrap_digest": hb(BOOTSTRAP_DOMAIN, BOOTSTRAP_BYTES).to_string()
            }))
            .map_err(io::Error::other)?,
        )?;
        Ok(Self {
            root,
            bootstrap,
            constraint,
        })
    }

    #[must_use]
    pub fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    /// # Errors
    ///
    /// The directory cannot be created.
    pub fn directory(&self, name: &str) -> io::Result<PathBuf> {
        let path = self.path(name);
        std::fs::create_dir(&path)?;
        Ok(path)
    }

    /// # Errors
    ///
    /// The file cannot be written.
    pub fn write(&self, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
        let path = self.path(name);
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    #[must_use]
    pub fn bootstrap_bytes() -> &'static [u8] {
        BOOTSTRAP_BYTES
    }
}

/// The `plan` object every provider service configuration carries.
#[must_use]
pub fn plan(constraint: &Path) -> Value {
    json!({
        "profile": "enforce",
        "execution_constraint_file": constraint,
        "organization_floor_file": null,
        "debt_snapshot_file": null,
        "waiver_bundle_file": null
    })
}

/// The `paths` object, with the inbox present only for the webhook lanes.
#[must_use]
pub fn paths(bootstrap: &Path, scratch: &Path, ledger: &Path, inbox: Option<&Path>) -> Value {
    let mut value = json!({ "bootstrap": bootstrap, "scratch": scratch, "ledger": ledger });
    if let (Some(inbox), Some(object)) = (inbox, value.as_object_mut()) {
        object.insert("inbox".to_owned(), json!(inbox));
    }
    value
}
