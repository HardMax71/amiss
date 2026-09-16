use amiss_wire::de::Document as _;
use sha2::Digest as _;
use std::collections::BTreeSet;
use strum::IntoEnumIterator as _;

use amiss_wire::controls::{DocumentInclude, IncludeKind, ScannerPolicy, ScannerPolicySchema};
use amiss_wire::human::atom;
use amiss_wire::model::{Adapter, RepoPathText};

use super::super::arguments::{Gathered, required};
use super::super::{
    AuthorInvocation, Code, PolicyIncludeInvocation, PolicyIncludePreview, Refusal, Verb,
};
use super::{Validation, classify_object_format, classify_repo, invalid, record, refuse_foreign};

/// The path refuses the bytes the claim url and the extractor cannot carry.
pub(super) fn classify_claim(
    mut refusals: BTreeSet<Refusal>,
    gathered: &Gathered,
) -> Result<AuthorInvocation, BTreeSet<Refusal>> {
    refuse_foreign(
        &mut refusals,
        gathered,
        Verb::Claim,
        &["--repo", "--path", "--line", "--name"],
    );
    let authored = claim_values(&mut refusals, gathered);
    if !refusals.is_empty() {
        return Err(refusals);
    }
    authored.ok_or_else(|| BTreeSet::from([invalid(Code::InvalidInvocation.meaning().to_owned())]))
}

/// The claim itself: the checkout it reads, the path and line it pins, and
/// the reserved label it will be spelled under.
fn claim_values(refusals: &mut BTreeSet<Refusal>, gathered: &Gathered) -> Option<AuthorInvocation> {
    let repo = record(refusals, classify_repo(gathered));
    let name = record(
        refusals,
        required(&gathered.claim_name, "--name").and_then(|value| {
            amiss_wire::extraction::governed_name_valid(value)
                .then(|| value.to_owned())
                .ok_or_else(|| {
                    invalid(format!(
                        "--name must be 1 to 120 ASCII bytes, a letter or digit first, then letters, digits, dots, underscores, or hyphens, got {}",
                        atom(value)
                    ))
                })
        }),
    );
    let line = record(
        refusals,
        required(&gathered.claim_line, "--line").and_then(claim_line),
    );
    let path = record(
        refusals,
        required(&gathered.claim_path, "--path").and_then(|value| {
            (!value.contains(['&', '<', '>', '"', ' ', '%', '?', '#', '\\']))
                .then(|| RepoPathText::try_from(value.to_owned()).ok())
                .flatten()
                .ok_or_else(|| {
                    invalid(format!(
                        "--path must be a repository path without spaces, quotes, backslashes, or any of & < > % ? #, got {}",
                        atom(value)
                    ))
                })
        }),
    );
    Some(AuthorInvocation {
        repo: repo?,
        path: path?,
        line: line?,
        name: name?,
    })
}

fn claim_line(value: &str) -> Validation<u64> {
    let lawful = !value.is_empty()
        && value.len() <= 16
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit());
    let ceiling = u64::try_from(js_int::MAX_SAFE_INT).ok();
    lawful
        .then(|| value.parse::<u64>().ok())
        .flatten()
        .filter(|line| Some(*line) <= ceiling)
        .ok_or_else(|| {
            invalid(format!(
                "--line must be a one-based line number without leading zeros, got {}",
                atom(value)
            ))
        })
}

pub(super) fn classify_policy_include(
    mut refusals: BTreeSet<Refusal>,
    gathered: &Gathered,
) -> Result<PolicyIncludeInvocation, BTreeSet<Refusal>> {
    refuse_foreign(
        &mut refusals,
        gathered,
        Verb::PolicyInclude,
        &[
            "--path",
            "--suffix",
            "--adapter",
            "--repo",
            "--object-format",
            "--index",
        ],
    );

    let policy = include_row(&mut refusals, gathered);

    let preview_presence = [
        gathered.repo.occurrences > 0,
        gathered.object_format.occurrences > 0,
        gathered.index > 0,
    ];
    let preview = if preview_presence == [false, false, false] {
        Some(None)
    } else if preview_presence == [true, true, true] {
        let repo = record(&mut refusals, classify_repo(gathered));
        let object_format = record(&mut refusals, classify_object_format(gathered));
        repo.zip(object_format).map(|(repo, object_format)| {
            Some(PolicyIncludePreview {
                repo,
                object_format,
            })
        })
    } else {
        refusals.insert(invalid(
            "--repo, --object-format, and --index form the preview group together".to_owned(),
        ));
        None
    };

    if !refusals.is_empty() {
        return Err(refusals);
    }
    policy
        .zip(preview)
        .map(
            |((policy, policy_digest), preview)| PolicyIncludeInvocation {
                policy,
                policy_digest,
                preview,
            },
        )
        .ok_or_else(|| BTreeSet::from([invalid(Code::InvalidInvocation.meaning().to_owned())]))
}

/// The one include row the selector spells, built and accepted by the
/// policy reader before anything is printed.
fn include_row(
    refusals: &mut BTreeSet<Refusal>,
    gathered: &Gathered,
) -> Option<(ScannerPolicy, amiss_wire::model::Digest)> {
    let path = record(
        refusals,
        required(&gathered.claim_path, "--path").and_then(|value| {
            RepoPathText::try_from(value.to_owned()).map_err(|_invalid| {
                invalid(format!(
                    "--path must be a repository path, got {}",
                    atom(value)
                ))
            })
        }),
    );
    let suffix = record(
        refusals,
        required(&gathered.suffix, "--suffix").map(str::to_owned),
    );
    let adapter = record(
        refusals,
        required(&gathered.adapter, "--adapter").and_then(|value| {
            value.parse::<Adapter>().map_err(|_unknown| {
                let adapters: Vec<&'static str> = Adapter::iter().map(Into::into).collect();
                invalid(format!(
                    "--adapter must be one of {}, got {}",
                    adapters.join(", "),
                    atom(value)
                ))
            })
        }),
    );
    let policy = ScannerPolicy {
        schema: ScannerPolicySchema::Current,
        document_includes: vec![DocumentInclude {
            path: path?,
            kind: IncludeKind::Tree,
            suffix: Some(suffix?),
            adapter: Some(adapter?),
        }],
        projection_assertions: Some(Vec::new()),
        protected_inventory: Vec::new(),
        finding_dispositions: Vec::new(),
    };
    if let Err(error) = policy.validate() {
        refusals.insert(invalid(format!(
            "--path and --suffix do not form a valid include: {error}"
        )));
        return None;
    }
    let mut writer = digest_io::IoWrapper(
        sha2::Sha256::new_with_prefix(amiss_wire::controls::SCANNER_POLICY_SCHEMA)
            .chain_update([0_u8]),
    );
    serde_json_canonicalizer::to_writer(&policy, &mut writer).ok()?;
    Some((
        policy,
        amiss_wire::model::Digest::from(writer.0.finalize().0),
    ))
}
