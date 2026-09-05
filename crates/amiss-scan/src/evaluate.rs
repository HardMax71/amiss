use amiss_wire::json::Value;
use amiss_wire::model::RepoPath;
use amiss_wire::report::FindingKind;
use amiss_wire::resolution::{
    BlobContent, BlobTarget, Missing, Resolution, Target, TargetTag, UnsupportedSemantics,
    UnsupportedTarget, VersionScope,
};
use strum::IntoDiscriminant;

use crate::correlate::Observation;

mod claims;
mod control;
mod debt;
mod documents;
mod finding;
mod model;
mod projections;
mod references;
mod run;
mod tests;
mod waiver;

pub(crate) use claims::source_multiplicities;
pub use claims::{ClaimGroup, claim_groups};
pub use control::GovernedSeed;
pub(crate) use finding::key_value;
use model::FindingKeyScope;
pub use model::{
    Attribution, DebtApplied, DocumentInput, DocumentSide, Finding, FindingFact, FindingFix,
    Location, LocationSide, PolicyStep, WaiverApplied,
};
pub use references::structural_facts;
pub(crate) use run::{GovernedInputs, evaluate_with_site};
use run::{candidate_digest_of, tree_value};
pub use run::{evaluate, evaluate_with_policy};

pub const FINDING_KEY_SCHEMA: &str = "amiss/scanner-finding-key-input";
pub const FINDING_KEY_DOMAIN: &str = "amiss/scanner-finding-key";
pub const FACT_SCHEMA: &str = "amiss/scanner-fact";
pub const FACT_DOMAIN: &str = "amiss/scanner-fact";

fn resolution_value(observation: &Observation) -> Value {
    resolution_row(&observation.resolution)
}

pub(crate) fn resolution_row(resolution: &crate::resolve::Resolution) -> Value {
    match resolution {
        Resolution::Resolved { target } | Resolution::TypeMismatch { target } => resolution_object(
            resolution.discriminant().as_ref(),
            vec![("target", target_value(target))],
        ),
        Resolution::Missing(missing) => match missing {
            Missing::LineFragmentOutOfRange { path } => reasoned_resolution(
                resolution.discriminant().as_ref(),
                missing.discriminant().as_ref(),
                vec![("path", path.to_value())],
            ),
            Missing::PathNotFound {
                path,
                near,
                same_object_at,
            } => reasoned_resolution(
                resolution.discriminant().as_ref(),
                missing.discriminant().as_ref(),
                vec![
                    ("path", path.to_value()),
                    (
                        "near",
                        near.as_ref().map_or(Value::Null, RepoPath::to_value),
                    ),
                    (
                        "same_object_at",
                        same_object_at
                            .as_ref()
                            .map_or(Value::Null, RepoPath::to_value),
                    ),
                ],
            ),
            Missing::HeadingAnchorNotFound { path, near } => reasoned_resolution(
                resolution.discriminant().as_ref(),
                missing.discriminant().as_ref(),
                vec![
                    ("path", path.to_value()),
                    (
                        "near",
                        near.as_ref()
                            .map_or(Value::Null, |anchor| Value::string(anchor.clone())),
                    ),
                ],
            ),
            Missing::LabelNotDeclared => reasoned_resolution(
                resolution.discriminant().as_ref(),
                missing.discriminant().as_ref(),
                Vec::new(),
            ),
        },
        Resolution::DeclaredUntracked(declared) => resolution_object(
            resolution.discriminant().as_ref(),
            vec![
                ("path", declared.path.to_value()),
                ("declared_by", declared.declared_by.to_value()),
            ],
        ),
        Resolution::UnsupportedTarget(target) => {
            unsupported_target_value(resolution.discriminant().as_ref(), target)
        }
        Resolution::UnsupportedSemantics(semantics) => {
            unsupported_semantics_value(resolution.discriminant().as_ref(), semantics)
        }
        Resolution::UnsupportedVersion { scope } => {
            let mut fields = vec![(
                "kind".to_owned(),
                Value::string(scope.discriminant().as_ref().to_owned()),
            )];
            match scope {
                VersionScope::KnownPath { path } => {
                    fields.push(("path".to_owned(), path.to_value()));
                }
                VersionScope::KnownCommit { commit_oid, path } => {
                    fields.push((
                        "commit_oid".to_owned(),
                        Value::string(commit_oid.as_str().to_owned()),
                    ));
                    fields.push(("path".to_owned(), path.to_value()));
                }
                VersionScope::UnknownPath => {}
            }
            resolution_object(
                resolution.discriminant().as_ref(),
                vec![("scope", Value::object(fields))],
            )
        }
        Resolution::Invalid { reason } => reasoned_resolution(
            resolution.discriminant().as_ref(),
            reason.as_ref(),
            Vec::new(),
        ),
        Resolution::External { reason } => reasoned_resolution(
            resolution.discriminant().as_ref(),
            reason.as_ref(),
            Vec::new(),
        ),
    }
}

fn resolution_object(kind: &str, fields: Vec<(&str, Value)>) -> Value {
    let mut members = Vec::with_capacity(fields.len().saturating_add(1));
    members.push(("kind".to_owned(), Value::string(kind.to_owned())));
    members.extend(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value)),
    );
    Value::object(members)
}

fn reasoned_resolution(kind: &str, reason: &str, fields: Vec<(&str, Value)>) -> Value {
    let mut fields = fields;
    fields.insert(0, ("reason", Value::string(reason.to_owned())));
    resolution_object(kind, fields)
}

fn unsupported_target_value(kind: &str, target: &UnsupportedTarget<RepoPath>) -> Value {
    let path = match target {
        UnsupportedTarget::Symlink { path } | UnsupportedTarget::Gitlink { path } => path,
    };
    reasoned_resolution(
        kind,
        target.discriminant().as_ref(),
        vec![("path", path.to_value())],
    )
}

fn unsupported_semantics_value(kind: &str, semantics: &UnsupportedSemantics<RepoPath>) -> Value {
    match semantics {
        UnsupportedSemantics::Query(target) | UnsupportedSemantics::CodeFragment(target) => {
            reasoned_resolution(
                kind,
                semantics.discriminant().as_ref(),
                vec![("target", target_value(target))],
            )
        }
        UnsupportedSemantics::Fragment(blob) => reasoned_resolution(
            kind,
            semantics.discriminant().as_ref(),
            vec![("target", blob_target_value(blob))],
        ),
        UnsupportedSemantics::SiteRoute
        | UnsupportedSemantics::NetworkPath
        | UnsupportedSemantics::AttributeDependent
        | UnsupportedSemantics::DuplicateLabel
        | UnsupportedSemantics::ExternalInventory => {
            reasoned_resolution(kind, semantics.discriminant().as_ref(), Vec::new())
        }
    }
}

fn target_value(target: &Target<RepoPath>) -> Value {
    match target {
        Target::Tree { path } => Value::object(vec![
            (
                "kind".to_owned(),
                Value::string(target.discriminant().as_ref().to_owned()),
            ),
            ("path".to_owned(), path.to_value()),
        ]),
        Target::Blob(blob) => blob_target_value(blob),
    }
}

fn blob_target_value(blob: &BlobTarget<RepoPath>) -> Value {
    Value::object(vec![
        (
            "kind".to_owned(),
            Value::string(TargetTag::Blob.as_ref().to_owned()),
        ),
        ("path".to_owned(), blob.path.to_value()),
        (
            "mode".to_owned(),
            Value::string(blob.mode.as_ref().to_owned()),
        ),
        ("content".to_owned(), blob_content_value(blob.content)),
    ])
}

fn blob_content_value(content: BlobContent) -> Value {
    match content {
        BlobContent::Available {
            raw_digest,
            projection_digest,
        } => Value::object(vec![
            (
                "kind".to_owned(),
                Value::string(content.discriminant().as_ref().to_owned()),
            ),
            (
                "raw_digest".to_owned(),
                Value::string(raw_digest.to_string()),
            ),
            (
                "projection_digest".to_owned(),
                Value::string(projection_digest.to_string()),
            ),
        ]),
        BlobContent::LfsPointer { raw_digest } => Value::object(vec![
            (
                "kind".to_owned(),
                Value::string(content.discriminant().as_ref().to_owned()),
            ),
            (
                "raw_digest".to_owned(),
                Value::string(raw_digest.to_string()),
            ),
        ]),
    }
}

pub(super) struct ResolutionKinds {
    pub(super) structural: Option<FindingKind>,
    pub(super) boundary: Option<FindingKind>,
}

pub(super) const fn resolution_kinds(resolution: &crate::resolve::Resolution) -> ResolutionKinds {
    match resolution {
        Resolution::Missing(_) => ResolutionKinds {
            structural: Some(FindingKind::ExplicitTargetMissing),
            boundary: None,
        },
        Resolution::TypeMismatch { .. } => ResolutionKinds {
            structural: Some(FindingKind::ExplicitTargetTypeMismatch),
            boundary: None,
        },
        Resolution::Invalid { .. } => ResolutionKinds {
            structural: None,
            boundary: Some(FindingKind::InvalidReference),
        },
        Resolution::UnsupportedSemantics(_) => ResolutionKinds {
            structural: None,
            boundary: Some(FindingKind::UnsupportedReferenceSemantics),
        },
        Resolution::UnsupportedVersion { .. } => ResolutionKinds {
            structural: None,
            boundary: Some(FindingKind::UnsupportedVersionScope),
        },
        Resolution::UnsupportedTarget(_) => ResolutionKinds {
            structural: None,
            boundary: Some(FindingKind::UnsupportedTargetKind),
        },
        Resolution::DeclaredUntracked(_) => ResolutionKinds {
            structural: None,
            boundary: Some(FindingKind::TargetDeclaredUntracked),
        },
        Resolution::Resolved { .. } | Resolution::External { .. } => ResolutionKinds {
            structural: None,
            boundary: None,
        },
    }
}
