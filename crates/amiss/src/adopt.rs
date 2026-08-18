use std::fs;
use std::process::ExitCode;

use amiss_scan::report::Built;
use amiss_wire::controls::DebtSnapshot;
use amiss_wire::json::Value;

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
    let bytes = amiss_wire::json::canonical(&snapshot);
    // The engine's own reader is the gate: a file it would refuse is never
    // written.
    if DebtSnapshot::parse(&bytes).is_err() {
        println!("amiss adopt: the minted snapshot failed its own reader; nothing recorded");
        return ExitCode::from(2);
    }
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
fn items(envelope: &Value, adoption: &Adoption) -> (Vec<Value>, usize, usize) {
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
        rows.push(Value::object(vec![
            ("debt_id".to_owned(), Value::string(format!("debt/{full}"))),
            ("finding_key".to_owned(), Value::string(key.to_owned())),
            ("accepted_fact".to_owned(), fact.clone()),
            (
                "accepted_fact_digest".to_owned(),
                Value::string(fact_digest.to_owned()),
            ),
            ("owner".to_owned(), Value::string(adoption.owner.clone())),
            ("reason".to_owned(), Value::string(adoption.reason.clone())),
            (
                "created_at".to_owned(),
                Value::string(adoption.created_at.clone()),
            ),
            (
                "expires_at".to_owned(),
                Value::string(adoption.expires_at.clone()),
            ),
        ]));
    }
    (rows, ineligible, factless)
}

fn snapshot(
    envelope: &Value,
    identity: &ProviderIdentity,
    adoption: &Adoption,
    built: &Built,
    items: Vec<Value>,
) -> Option<Value> {
    let candidate = member(envelope, "payload")
        .and_then(|payload| member(payload, "evaluation"))
        .and_then(|evaluation| member(evaluation, "candidate"))?;
    let tree = member(candidate, "tree_oid").and_then(text)?;
    let object_format = member(candidate, "object_format").and_then(text)?;
    Some(Value::object(vec![
        (
            "schema".to_owned(),
            Value::string("amiss/debt-snapshot".to_owned()),
        ),
        (
            "repository".to_owned(),
            Value::object(vec![
                (
                    "host".to_owned(),
                    Value::string(identity.repository.host().to_owned()),
                ),
                (
                    "owner".to_owned(),
                    Value::string(identity.repository.owner().to_owned()),
                ),
                (
                    "name".to_owned(),
                    Value::string(identity.repository.name().to_owned()),
                ),
            ]),
        ),
        (
            "ref".to_owned(),
            Value::string(identity.ref_name.as_str().to_owned()),
        ),
        (
            "organization_floor_digest".to_owned(),
            Value::string(adoption.floor_digest.clone()),
        ),
        (
            "adoption_tree".to_owned(),
            Value::object(vec![
                (
                    "object_format".to_owned(),
                    Value::string(object_format.to_owned()),
                ),
                ("tree_oid".to_owned(), Value::string(tree.to_owned())),
            ]),
        ),
        (
            "adoption_report_payload_digest".to_owned(),
            Value::string(built.payload_digest.to_string()),
        ),
        (
            "created_at".to_owned(),
            Value::string(adoption.created_at.clone()),
        ),
        ("items".to_owned(), Value::array(items)),
    ]))
}
