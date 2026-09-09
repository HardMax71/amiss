use std::fs;
use std::process::ExitCode;

use amiss_scan::report::Built;
use amiss_wire::controls::{
    DebtItem, DebtSnapshot, DebtSnapshotSchema, EligibleFindingKind, Fact, FactEvidence,
    FactEvidenceKind, FindingKeyInput, FindingOccurrence, FindingScope, ReferenceScopeKind,
    TargetIntent, canonical_debt_snapshot,
};
use amiss_wire::digest::Digest;
use amiss_wire::model::{ArtifactId, OwnerId, TreeIdentity, UtcInstant};
use amiss_wire::report::Disposition;
use amiss_wire::report::model::{
    Evaluation, FindingFactEvidence, FindingKeyScope, ReportPayload, RepositoryIntentPath,
    RepositoryTargetIntent, Snapshot,
};
use amiss_wire::requests::CandidateSnapshot;

use crate::invocation::{Adoption, Invocation, ProviderIdentity};

mod tests;

#[expect(
    clippy::print_stdout,
    reason = "the adoption rows are the command's output"
)]
pub(crate) fn run(invocation: &Invocation, adoption: &Adoption, built: &Built) -> ExitCode {
    let Some(identity) = &invocation.identity else {
        println!("amiss adopt: the grammar requires the repository identity; nothing recorded");
        return ExitCode::from(2);
    };

    if built.exit_code == 2 {
        println!("amiss adopt: the evaluation could not be trusted; nothing recorded");
        return ExitCode::from(2);
    }
    if adoption.output.exists() {
        println!("amiss adopt: the output path already exists; nothing recorded");
        return ExitCode::FAILURE;
    }
    let payload = &built.envelope.payload;
    let Ok((items, ineligible, factless)) = items(built, adoption) else {
        println!("amiss adopt: the minted snapshot failed its own reader; nothing recorded");
        return ExitCode::from(2);
    };
    let recorded = items.len();
    let Some(snapshot) = snapshot(payload, identity, adoption, built.payload_digest, items) else {
        println!("amiss adopt: the report carries no candidate tree; nothing recorded");
        return ExitCode::from(2);
    };
    let Ok((bytes, _digest)) = canonical_debt_snapshot(&snapshot) else {
        println!("amiss adopt: the minted snapshot failed its own reader; nothing recorded");
        return ExitCode::from(2);
    };
    // Exclusive creation closes the race the early existence check leaves.
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&adoption.output)
        .and_then(|mut file| std::io::Write::write_all(&mut file, &bytes));
    if written.is_err() {
        // A partial file must not survive to block the retry.
        drop(fs::remove_file(&adoption.output));
        println!("amiss adopt: the output path could not be written; nothing recorded");
        return ExitCode::FAILURE;
    }
    println!(
        "amiss adopt: {recorded} blocking findings recorded at {}; {ineligible} blocking \
         findings are not debt-eligible; {factless} eligible rows skipped for missing facts",
        adoption.output.display()
    );
    ExitCode::SUCCESS
}

/// Every blocking, debt-eligible finding becomes one item carrying the fact
/// the adoption accepts; blocking rows outside the eligible kinds are
/// counted and left to be fixed instead.
fn items(built: &Built, adoption: &Adoption) -> Result<(Vec<DebtItem>, usize, usize), ()> {
    let owner = OwnerId::new(adoption.owner.clone()).ok_or(())?;
    let created_at = UtcInstant::new(adoption.created_at.clone()).ok_or(())?;
    let expires_at = UtcInstant::new(adoption.expires_at.clone()).ok_or(())?;
    let mut rows = Vec::new();
    let mut ineligible = 0_usize;
    let mut factless = 0_usize;
    for row in built
        .envelope
        .payload
        .findings
        .iter()
        .filter(|row| row.effective_disposition == Disposition::Fail)
    {
        if EligibleFindingKind::try_from(&row.kind).is_err() {
            ineligible = ineligible.saturating_add(1);
            continue;
        }
        let Some((fact, fact_digest)) = row.candidate_fact.as_ref().zip(row.candidate_fact_digest)
        else {
            factless = factless.saturating_add(1);
            continue;
        };
        let (
            FindingKeyScope::Reference {
                document,
                normalized_target_intent:
                    RepositoryTargetIntent {
                        commit_oid,
                        fragment_digest,
                        kind: intent_kind,
                        path: RepositoryIntentPath::Path(path),
                        query_digest,
                        target_kind,
                    },
                occurrence,
                source_construct,
            },
            FindingFactEvidence::Reference {
                occurrence_multiplicity,
                resolution,
            },
        ) = (&fact.key_input.scope, &fact.evidence)
        else {
            return Err(());
        };
        let key = row.finding_key.to_string();
        let full = key.strip_prefix("sha256:").ok_or(())?;
        rows.push(DebtItem {
            debt_id: ArtifactId::new(format!("debt/{full}")).ok_or(())?,
            finding_key: row.finding_key,
            accepted_fact: Fact {
                schema: fact.schema,
                finding_kind: (&fact.finding_kind).try_into()?,
                key_input: FindingKeyInput {
                    finding_kind: (&fact.key_input.finding_kind).try_into()?,
                    schema: fact.key_input.schema,
                    scope: FindingScope {
                        document: document.try_into()?,
                        kind: ReferenceScopeKind::Reference,
                        normalized_target_intent: TargetIntent {
                            commit_oid: commit_oid.clone(),
                            fragment_digest: *fragment_digest,
                            kind: intent_kind.into(),
                            path: path.try_into()?,
                            query_digest: *query_digest,
                            target_kind: *target_kind,
                        },
                        occurrence: FindingOccurrence {
                            kind: (&occurrence.kind).into(),
                            source_projection_digest: occurrence.source_projection_digest,
                        },
                        source_construct: *source_construct,
                    },
                },
                evidence: FactEvidence {
                    kind: FactEvidenceKind::Reference,
                    resolution: resolution.try_into()?,
                    occurrence_multiplicity: *occurrence_multiplicity,
                },
            },
            accepted_fact_digest: fact_digest,
            owner: owner.clone(),
            reason: adoption.reason.clone(),
            created_at: created_at.clone(),
            expires_at: expires_at.clone(),
        });
    }
    Ok((rows, ineligible, factless))
}

fn snapshot<P, R, M, E>(
    payload: &ReportPayload<P, R, M, E>,
    identity: &ProviderIdentity,
    adoption: &Adoption,
    payload_digest: Digest,
    items: Vec<DebtItem>,
) -> Option<DebtSnapshot> {
    let Evaluation::Resolved(evaluation) = &payload.evaluation else {
        return None;
    };
    let Snapshot::Available(CandidateSnapshot::Git(candidate)) = &evaluation.candidate else {
        return None;
    };
    Some(DebtSnapshot {
        schema: DebtSnapshotSchema::Current,
        repository: identity.repository.clone(),
        ref_name: identity.ref_name.clone(),
        organization_floor_digest: Digest::from_wire(&adoption.floor_digest)?,
        adoption_tree: TreeIdentity {
            object_format: candidate.object_format,
            tree_oid: candidate.tree_oid.clone(),
        },
        adoption_report_payload_digest: payload_digest,
        created_at: UtcInstant::new(adoption.created_at.clone())?,
        items,
    })
}
