use std::fs;
use std::process::ExitCode;

use amiss_scan::report::Built;
use amiss_wire::controls::DebtSnapshot;
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::{BranchRef, RepositoryIdentity};
use serde::Serialize;

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
    let (items, ineligible, factless) = items(&built.envelope, adoption);
    let recorded = items.len();
    let Some(snapshot) = snapshot(&built.envelope, identity, adoption, built, items) else {
        println!("amiss adopt: the report carries no candidate tree; nothing recorded");
        return ExitCode::from(2);
    };
    let snapshot = match amiss_wire::codec::to_value(&snapshot).and_then(|value| {
        DebtSnapshot::from_value(&value)?;
        Ok(value)
    }) {
        Ok(snapshot) => snapshot,
        Err(_defect) => {
            println!("amiss adopt: the minted snapshot failed its own reader; nothing recorded");
            return ExitCode::from(2);
        }
    };
    let bytes = amiss_wire::json::canonical(&snapshot);
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
fn items<'a>(envelope: &'a Value, adoption: &'a Adoption) -> (Vec<DebtRow<'a>>, usize, usize) {
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
        rows.push(DebtRow {
            debt_id: format!("debt/{full}"),
            finding_key: key,
            accepted_fact: fact,
            accepted_fact_digest: fact_digest,
            owner: &adoption.owner,
            reason: &adoption.reason,
            created_at: &adoption.created_at,
            expires_at: &adoption.expires_at,
        });
    }
    (rows, ineligible, factless)
}

#[derive(Serialize)]
struct DebtRow<'a> {
    debt_id: String,
    finding_key: &'a str,
    accepted_fact: &'a Value,
    accepted_fact_digest: &'a str,
    owner: &'a str,
    reason: &'a str,
    created_at: &'a str,
    expires_at: &'a str,
}

#[derive(Serialize)]
struct Snapshot<'a> {
    schema: &'static str,
    repository: &'a RepositoryIdentity,
    #[serde(rename = "ref")]
    ref_name: &'a BranchRef,
    organization_floor_digest: &'a str,
    adoption_tree: AdoptionTree<'a>,
    adoption_report_payload_digest: Digest,
    created_at: &'a str,
    items: Vec<DebtRow<'a>>,
}

#[derive(Serialize)]
struct AdoptionTree<'a> {
    object_format: &'a str,
    tree_oid: &'a str,
}

fn snapshot<'a>(
    envelope: &'a Value,
    identity: &'a ProviderIdentity,
    adoption: &'a Adoption,
    built: &Built,
    items: Vec<DebtRow<'a>>,
) -> Option<Snapshot<'a>> {
    let candidate = member(envelope, "payload")
        .and_then(|payload| member(payload, "evaluation"))
        .and_then(|evaluation| member(evaluation, "candidate"))?;
    let tree = member(candidate, "tree_oid").and_then(text)?;
    let object_format = member(candidate, "object_format").and_then(text)?;
    Some(Snapshot {
        schema: "amiss/debt-snapshot",
        repository: &identity.repository,
        ref_name: &identity.ref_name,
        organization_floor_digest: &adoption.floor_digest,
        adoption_tree: AdoptionTree {
            object_format,
            tree_oid: tree,
        },
        adoption_report_payload_digest: built.payload_digest,
        created_at: &adoption.created_at,
        items,
    })
}
