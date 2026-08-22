use std::collections::{BTreeMap, BTreeSet};

use amiss_git::{GitResources, ObjectKind, Repository, ValueCap, parse_tree};
use amiss_wire::controls::{
    DOCUMENT_SUFFIX_BYTES, GitMode, IncludeKind, ResourceName, SCANNER_POLICY_PATH, ScannerPolicy,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::Digest;
use amiss_wire::model::{Adapter, Oid, RepoPath};
use amiss_wire::report::{AnalysisErrorCode, ErrorDetail};

use crate::resources::ScanResources;
use crate::{Error, lfs};

/// One side's acquired repository policy: the digest is null exactly when the
/// path is absent, and absence has empty semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PolicySide {
    pub digest: Option<Digest>,
    pub policy: Option<ScannerPolicy>,
}

/// The union of both sides' exact, tree, and suffix includes, which fixes
/// classification row five and overrides built-in exclusion. Bindings are not
/// a union: one grammar per path per evaluation, taken from the candidate
/// policy so both sides extract comparably, or from the base policy when the
/// candidate carries none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Includes {
    pub documents: BTreeSet<RepoPath>,
    pub trees: BTreeSet<RepoPath>,
    pub suffix_roots: BTreeMap<String, BTreeSet<RepoPath>>,
    pub document_bindings: BTreeMap<RepoPath, Adapter>,
    pub tree_bindings: BTreeMap<RepoPath, Adapter>,
    pub suffix_bindings: BTreeMap<RepoPath, (String, Adapter)>,
}

impl Includes {
    #[must_use]
    pub fn union(base: &PolicySide, candidate: &PolicySide) -> Self {
        let mut merged = Self::default();
        for side in [base, candidate] {
            let Some(policy) = &side.policy else {
                continue;
            };
            for include in policy.document_includes() {
                let path = RepoPath::from(&include.path);
                if let Some(suffix) = &include.suffix {
                    merged
                        .suffix_roots
                        .entry(suffix.clone())
                        .or_default()
                        .insert(path);
                    continue;
                }
                match include.kind {
                    IncludeKind::Document => {
                        merged.documents.insert(path);
                    }
                    IncludeKind::Tree => {
                        merged.trees.insert(path);
                    }
                }
            }
        }
        if let Some(policy) = candidate.policy.as_ref().or(base.policy.as_ref()) {
            for include in policy.document_includes() {
                let Some(adapter) = include.adapter else {
                    continue;
                };
                let path = RepoPath::from(&include.path);
                if let Some(suffix) = &include.suffix {
                    merged
                        .suffix_bindings
                        .insert(path, (suffix.clone(), adapter));
                    continue;
                }
                match include.kind {
                    IncludeKind::Document => {
                        merged.document_bindings.insert(path, adapter);
                    }
                    IncludeKind::Tree => {
                        merged.tree_bindings.insert(path, adapter);
                    }
                }
            }
        }
        merged
    }

    /// The bound adapter for a policy-included path: an exact document wins,
    /// then the nearest matching suffixed or plain tree.
    #[must_use]
    pub fn binding(&self, path: &RepoPath) -> Option<Adapter> {
        if let Some(adapter) = self.document_bindings.get(path) {
            return Some(*adapter);
        }
        let raw = path.as_bytes();
        for ancestor in ancestors(raw) {
            if let Some((suffix, adapter)) = self.suffix_bindings.get(ancestor)
                && raw.ends_with(suffix.as_bytes())
            {
                return Some(*adapter);
            }
            if let Some(adapter) = self.tree_bindings.get(ancestor) {
                return Some(*adapter);
            }
        }
        None
    }

    /// A document include matches exactly its path; a plain tree matches its
    /// root and descendants, while a suffixed tree additionally requires the
    /// exact raw tail.
    #[must_use]
    pub fn matches(&self, path: &RepoPath) -> bool {
        if self.documents.contains(path) {
            return true;
        }
        let raw = path.as_bytes();
        let basename = raw.rsplit(|byte| *byte == b'/').next().unwrap_or(raw);
        covered(&self.trees, raw)
            || basename
                .iter()
                .enumerate()
                .filter(|(_, byte)| **byte == b'.')
                .filter_map(|(start, _)| basename.get(start..))
                .filter(|suffix| suffix.len() <= DOCUMENT_SUFFIX_BYTES)
                .filter_map(|suffix| std::str::from_utf8(suffix).ok())
                .any(|suffix| {
                    self.suffix_roots
                        .get(suffix)
                        .is_some_and(|roots| covered(roots, raw))
                })
    }
}

fn ancestors(path: &[u8]) -> impl Iterator<Item = &[u8]> {
    std::iter::once(path).chain(
        path.iter()
            .enumerate()
            .rev()
            .filter(|(_, byte)| **byte == b'/')
            .filter_map(|(separator, _)| path.get(..separator)),
    )
}

fn covered(roots: &BTreeSet<RepoPath>, path: &[u8]) -> bool {
    ancestors(path).any(|ancestor| roots.contains(ancestor))
}

fn specific_code(kind: &ErrorKind) -> AnalysisErrorCode {
    match kind {
        ErrorKind::Json(_) => AnalysisErrorCode::InvalidJson,
        ErrorKind::UnknownField => AnalysisErrorCode::UnknownField,
        ErrorKind::DigestMismatch => AnalysisErrorCode::DigestMismatch,
        ErrorKind::UnsortedSet | ErrorKind::DuplicateMember => AnalysisErrorCode::NoncanonicalArray,
        ErrorKind::MissingField
        | ErrorKind::WrongType
        | ErrorKind::InvalidValue
        | ErrorKind::LimitExceeded
        | ErrorKind::Inconsistent => AnalysisErrorCode::ConfigurationInvalid,
    }
}

fn invalid(details: Vec<AnalysisErrorCode>) -> Vec<ErrorDetail> {
    let mut rows = vec![ErrorDetail {
        code: AnalysisErrorCode::ConfigurationInvalid,
        path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
        path_bytes: None,
        resource: None,
    }];
    for code in details {
        if code != AnalysisErrorCode::ConfigurationInvalid {
            rows.push(ErrorDetail {
                code,
                path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
                path_bytes: None,
                resource: None,
            });
        }
    }
    rows
}

/// Finds the exact policy path in a snapshot tree without discovering the
/// snapshot, so policy validation can precede discovery as the fatal order
/// requires.
fn locate(
    repo: &Repository,
    git: &mut GitResources,
    root_tree: &Oid,
) -> Result<Option<(GitMode, Oid)>, Error> {
    let mut components = SCANNER_POLICY_PATH.split('/').peekable();
    let mut tree_oid = root_tree.clone();
    while let Some(component) = components.next() {
        let object = repo
            .read_expected(git, &tree_oid, ObjectKind::Tree)
            .map_err(Error::from)?;
        let entries = parse_tree(repo.object_format(), &object.body).map_err(Error::from)?;
        let Some(entry) = entries
            .iter()
            .find(|entry| entry.name == component.as_bytes())
        else {
            return Ok(None);
        };
        if components.peek().is_none() {
            return Ok(Some((entry.mode, entry.oid.clone())));
        }
        if entry.mode != GitMode::Tree {
            return Ok(None);
        }
        tree_oid = entry.oid.clone();
    }
    Ok(None)
}

/// Acquires one side's policy under the exact object-form law: an ordinary
/// non-LFS regular blob with mode `100644`, read under the configuration
/// `control-input-bytes` cap, strictly parsed. Every other present form is
/// configuration-invalid at the policy path.
///
/// # Errors
///
/// The complete typed error rows for an invalid policy; acquisition defects
/// below the policy itself propagate as their own codes.
pub fn acquire(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    root_tree: &Oid,
) -> Result<PolicySide, Vec<ErrorDetail>> {
    let located = locate(repo, git, root_tree).map_err(|defect| {
        vec![ErrorDetail {
            code: defect.code(),
            path: None,
            path_bytes: None,
            resource: None,
        }]
    })?;
    let Some((mode, oid)) = located else {
        return Ok(PolicySide::default());
    };
    acquire_entry(repo, git, scan, mode, &oid)
}

/// Acquires a located policy entry under the same object-form law, for a
/// snapshot whose entries are already enumerated.
///
/// # Errors
///
/// Exactly as `acquire`.
pub fn acquire_entry(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    mode: GitMode,
    oid: &Oid,
) -> Result<PolicySide, Vec<ErrorDetail>> {
    if mode != GitMode::RegularFile {
        return Err(invalid(Vec::new()));
    }
    let cap = ValueCap {
        resource: ResourceName::ControlInputBytes,
        limit: scan.limits().control_input_bytes,
    };
    let object = repo
        .read_expected_capped(git, oid, ObjectKind::Blob, cap)
        .map_err(|defect| {
            let defect = Error::from(defect);
            vec![match defect {
                Error::ResourceLimit {
                    resource,
                    configured_limit,
                    observed_lower_bound,
                } => ErrorDetail {
                    code: AnalysisErrorCode::ResourceLimitExceeded,
                    path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
                    path_bytes: None,
                    resource: Some((resource, configured_limit, observed_lower_bound)),
                },
                Error::Parse(_) | Error::Git(_) | Error::UnrepresentablePath | Error::Internal => {
                    ErrorDetail {
                        code: defect.code(),
                        path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
                        path_bytes: None,
                        resource: None,
                    }
                }
            }]
        })?;
    if lfs::is_pointer(&object.body) {
        return Err(invalid(Vec::new()));
    }
    match ScannerPolicy::parse(&object.body) {
        Ok(policy) => {
            let entries = [
                policy.document_includes().len(),
                policy.protected_inventory().len(),
                policy.finding_dispositions().len(),
            ]
            .iter()
            .map(|&len| u64::try_from(len).unwrap_or(u64::MAX))
            .fold(0_u64, u64::saturating_add);
            let limit = scan.limits().repository_policy_entries;
            if entries > limit {
                return Err(vec![ErrorDetail {
                    code: AnalysisErrorCode::ResourceLimitExceeded,
                    path: RepoPath::new(SCANNER_POLICY_PATH.to_owned()),
                    path_bytes: None,
                    resource: Some((
                        ResourceName::RepositoryPolicyEntries,
                        limit,
                        limit.saturating_add(1),
                    )),
                }]);
            }
            Ok(PolicySide {
                digest: Some(policy.digest()),
                policy: Some(policy),
            })
        }
        Err(defect) => Err(invalid(vec![specific_code(&defect.kind)])),
    }
}
