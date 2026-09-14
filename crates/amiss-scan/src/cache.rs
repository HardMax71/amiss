use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use amiss_wire::de;
use amiss_wire::envelope::{Payload, Sealing};
use amiss_wire::model::{Adapter, Digest, Oid};

use crate::scan::Scanned;

/// The ceiling on one stored row.
pub const ROW_BYTES: u64 = 16 * 1024 * 1024;

const ROW_SCHEMA: &str = "amiss/scan-cache-row";

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub enum ScanCacheRowSchema {
    #[default]
    #[strum(serialize = "amiss/scan-cache-row")]
    Current,
}

/// What one engine build made of one blob under one grammar. The row names
/// the build, so a different build never reads it, and it is sealed like
/// every other document, so a damaged file is refused rather than trusted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanCacheRow {
    pub adapter: Adapter,
    pub engine: Digest,
    pub oid: Oid,
    pub scanned: Scanned,
}

impl Payload for ScanCacheRow {
    type Schema = ScanCacheRowSchema;
    type Defect = de::Error;
    const DOMAIN: &'static str = ROW_SCHEMA;
    const DOCUMENT_BYTES: u64 = ROW_BYTES;
    const SEALING: Sealing = Sealing::Exact;
}

/// Scans an engine build already produced, kept on disk between runs. A
/// missing, damaged or foreign row costs one parse, never a wrong answer.
#[derive(Debug)]
pub struct ScanCache {
    root: PathBuf,
    engine: Digest,
}

impl ScanCache {
    #[must_use]
    pub fn open(root: PathBuf, engine: Digest) -> Arc<Self> {
        Arc::new(Self { root, engine })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn row_path(&self, adapter: Adapter, oid: &Oid) -> PathBuf {
        self.root
            .join(self.engine.to_string().replace(':', "-"))
            .join(adapter.to_string())
            .join(format!("{}.json", oid.as_str()))
    }

    pub(crate) fn read(&self, adapter: Adapter, oid: &Oid) -> Option<Arc<Scanned>> {
        let file = fs::File::open(self.row_path(adapter, oid)).ok()?;
        let mut bytes = Vec::new();
        file.take(ROW_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .ok()?;
        let row = ScanCacheRow::parse(&bytes).ok()?.payload;
        (row.engine == self.engine && row.adapter == adapter && row.oid == *oid)
            .then(|| Arc::new(row.scanned))
    }

    pub(crate) fn write(&self, adapter: Adapter, oid: &Oid, scanned: &Scanned) {
        let row = ScanCacheRow {
            adapter,
            engine: self.engine,
            oid: oid.clone(),
            scanned: scanned.clone(),
        };
        let Ok(bytes) = row.emit() else {
            return;
        };
        let path = self.row_path(adapter, oid);
        let Some(dir) = path.parent() else {
            return;
        };
        if fs::create_dir_all(dir).is_err() {
            return;
        }
        let staging = dir.join(format!(".{}.{}", oid.as_str(), std::process::id()));
        if fs::write(&staging, bytes).is_ok() && fs::rename(&staging, &path).is_ok() {
            return;
        }
        let _ignored = fs::remove_file(&staging);
    }
}
