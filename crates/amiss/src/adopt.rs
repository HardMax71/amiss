use std::fs;
use std::process::ExitCode;

use amiss_scan::report::Built;
use amiss_wire::controls::{
    DebtItem, DebtSnapshot, DebtSnapshotSchema, canonical_debt_snapshot, parse_fact,
};
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::{ArtifactId, ObjectFormat, Oid, OwnerId, TreeIdentity, UtcInstant};

use crate::invocation::{Adoption, Invocation, ProviderIdentity};
use crate::payload::{member, text};

const ELIGIBLE: [&str; 2] = ["explicit-target-missing", "explicit-target-type-mismatch"];

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
    let Ok((items, ineligible, factless)) = items(&built.envelope, adoption) else {
        println!("amiss adopt: the minted snapshot failed its own reader; nothing recorded");
        return ExitCode::from(2);
    };
    let recorded = items.len();
    let Some(snapshot) = snapshot(&built.envelope, identity, adoption, built, items) else {
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
fn items(envelope: &Value, adoption: &Adoption) -> Result<(Vec<DebtItem>, usize, usize), ()> {
    let owner = OwnerId::new(adoption.owner.clone()).ok_or(())?;
    let created_at = UtcInstant::new(adoption.created_at.clone()).ok_or(())?;
    let expires_at = UtcInstant::new(adoption.expires_at.clone()).ok_or(())?;
    let mut rows = Vec::new();
    let mut ineligible = 0_usize;
    let mut factless = 0_usize;
    let findings = member(envelope, "payload")
        .and_then(|payload| member(payload, "findings"))
        .and_then(|findings| {
            if let Value::Array(rows) = findings {
                Some(rows)
            } else {
                None
            }
        });
    for row in findings.into_iter().flatten() {
        if member(row, "effective_disposition").and_then(text) != Some("fail") {
            continue;
        }
        let kind = member(row, "kind").and_then(text);
        if !kind.is_some_and(|kind| ELIGIBLE.contains(&kind)) {
            ineligible = ineligible.saturating_add(1);
            continue;
        }
        let parts = member(row, "finding_key").and_then(text).zip(
            member(row, "candidate_fact")
                .filter(|fact| !matches!(fact, Value::Null))
                .zip(member(row, "candidate_fact_digest").and_then(text)),
        );
        let Some((key, (fact, fact_digest))) = parts else {
            factless = factless.saturating_add(1);
            continue;
        };
        let full = key.strip_prefix("sha256:").unwrap_or(key);
        rows.push(DebtItem {
            debt_id: ArtifactId::new(format!("debt/{full}")).ok_or(())?,
            finding_key: Digest::from_wire(key).ok_or(())?,
            accepted_fact: parse_fact(&amiss_wire::json::canonical(fact)).map_err(|_defect| ())?,
            accepted_fact_digest: Digest::from_wire(fact_digest).ok_or(())?,
            owner: owner.clone(),
            reason: adoption.reason.clone(),
            created_at: created_at.clone(),
            expires_at: expires_at.clone(),
        });
    }
    Ok((rows, ineligible, factless))
}

fn snapshot(
    envelope: &Value,
    identity: &ProviderIdentity,
    adoption: &Adoption,
    built: &Built,
    items: Vec<DebtItem>,
) -> Option<DebtSnapshot> {
    let candidate = member(envelope, "payload")
        .and_then(|payload| member(payload, "evaluation"))
        .and_then(|evaluation| member(evaluation, "candidate"))?;
    let object_format = member(candidate, "object_format")
        .and_then(text)?
        .parse::<ObjectFormat>()
        .ok()?;
    let tree_oid = Oid::new(
        object_format,
        member(candidate, "tree_oid").and_then(text)?.to_owned(),
    )?;
    Some(DebtSnapshot {
        schema: DebtSnapshotSchema::Current,
        repository: identity.repository.clone(),
        ref_name: identity.ref_name.clone(),
        organization_floor_digest: Digest::from_wire(&adoption.floor_digest)?,
        adoption_tree: TreeIdentity {
            object_format,
            tree_oid,
        },
        adoption_report_payload_digest: built.payload_digest,
        created_at: UtcInstant::new(adoption.created_at.clone())?,
        items,
    })
}
