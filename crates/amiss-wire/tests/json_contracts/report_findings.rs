use amiss_wire::{
    controls::{canonical_debt_snapshot, canonical_waiver_bundle},
    report::{
        Disposition, FixKind,
        model::{
            ByteSpan, DebtApplication, Finding, FindingFix, PolicySource, PolicyStep,
            ReportEnvelope, WaiverApplication,
        },
    },
};

#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "published fixtures"
)]
pub(super) fn reports() -> [ReportEnvelope; 2] {
    let mut debt: ReportEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-report.frozen-1.json"
    ))
    .unwrap();
    let snapshot = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/debt-snapshot.json"
    ))
    .unwrap();
    let bundle = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/waiver-bundle.json"
    ))
    .unwrap();
    let (_, debt_snapshot_digest) = canonical_debt_snapshot(&snapshot).unwrap();
    let (_, waiver_bundle_digest) = canonical_waiver_bundle(&bundle).unwrap();
    let mut waiver = debt.clone();
    let finding = &mut debt.payload.findings[0];
    let item = snapshot.items.into_iter().next().unwrap();
    finding.debt = Some(DebtApplication {
        accepted_fact_digest: finding.candidate_fact_digest.unwrap(),
        adoption_tree: snapshot.adoption_tree,
        created_at: item.created_at,
        debt_id: item.debt_id,
        debt_snapshot_digest,
        expires_at: item.expires_at,
        owner: item.owner,
        reason: item.reason,
    });
    finding.policy_trace.push(PolicyStep {
        after: Disposition::Warn,
        before: Disposition::Warn,
        rule_id: "debt/debt/readme-missing-example".to_owned(),
        source: PolicySource::DebtSnapshot,
    });
    debt.payload.summary.findings.debt_tolerated = 1;
    let finding = &mut waiver.payload.findings[0];
    let item = bundle.items.into_iter().next().unwrap();
    finding.waiver = Some(WaiverApplication {
        authorized_fact_digest: finding.candidate_fact_digest.unwrap(),
        candidate_tree: item.candidate_tree,
        created_at: item.created_at,
        expires_at: item.expires_at,
        issuer: item.issuer,
        not_before: item.not_before,
        owner: item.owner,
        reason: item.reason,
        residual_disposition: item.residual_disposition,
        waiver_bundle_digest,
        waiver_id: item.waiver_id,
    });
    finding.configured_disposition = Disposition::Fail;
    finding.policy_trace[0].after = Disposition::Fail;
    finding.policy_trace.push(PolicyStep {
        after: Disposition::Warn,
        before: Disposition::Fail,
        rule_id: "waiver/waiver/readme-missing-example".to_owned(),
        source: PolicySource::WaiverBundle,
    });
    waiver.payload.summary.findings.waived = 1;
    finding.fix = Some(FindingFix {
        description: FixKind::PathRespelling.meaning().to_owned(),
        path: amiss_wire::model::RepoPathText::new("README.md".to_owned()).unwrap(),
        replacement: "docs/Example.md".to_owned(),
        span: ByteSpan {
            end_byte: 39,
            start_byte: 24,
        },
    });
    [debt, waiver]
}

#[test]
fn finding_metadata_requires_object_fields() {
    let [debt_report, waiver_report] = reports();
    let debt = debt_report.payload.findings[0].debt.as_ref().unwrap();
    let finding = &waiver_report.payload.findings[0];
    let waiver = finding.waiver.as_ref().unwrap();
    let fix = finding.fix.as_ref().unwrap();
    let aggregation = &finding.aggregation;
    let location = &finding.location;
    let encoded = serde_json::to_string(&[&debt_report.payload.findings[0], finding]).unwrap();
    let rejected: Vec<_> = [
        (
            serde_json::to_string(aggregation).unwrap(),
            serde_json::to_string(&(
                aggregation.locations_omitted,
                aggregation.member_count,
                aggregation.representative_rule,
                aggregation.strategy,
            ))
            .unwrap(),
        ),
        (
            serde_json::to_string(location).unwrap(),
            serde_json::to_string(&(&location.path, location.side, location.span)).unwrap(),
        ),
        (
            serde_json::to_string(&fix.span).unwrap(),
            serde_json::to_string(&(fix.span.end_byte, fix.span.start_byte)).unwrap(),
        ),
    ]
    .into_iter()
    .chain([&debt.adoption_tree, &waiver.candidate_tree].map(|tree| {
        (
            serde_json::to_string(tree).unwrap(),
            serde_json::to_string(&(tree.object_format, &tree.tree_oid)).unwrap(),
        )
    }))
    .map(|(object, sequence)| {
        let altered = encoded.replace(&object, &sequence);
        assert_ne!(altered, encoded);
        serde_json::from_str::<[Finding; 2]>(&altered).is_err()
    })
    .collect();
    assert_eq!(rejected, [true; 5]);
}

#[test]
fn nullable_finding_fields_remain_required() {
    let [report, _waiver_report] = reports();
    let mut finding = report.payload.findings.into_iter().next().unwrap();
    finding.base_fact = None;
    finding.base_fact_digest = None;
    finding.candidate_fact = None;
    finding.candidate_fact_digest = None;
    finding.debt = None;
    finding.waiver = None;
    finding.fix = None;
    finding.location.path = None;
    finding.location.span = None;
    let encoded = serde_json::to_string(&finding).unwrap();
    assert_eq!(serde_json::from_str::<Finding>(&encoded).unwrap(), finding);
    for field in [
        "base_fact",
        "base_fact_digest",
        "candidate_fact",
        "candidate_fact_digest",
        "debt",
        "waiver",
        "fix",
        "path",
        "span",
    ] {
        let member = format!("\"{field}\":null");
        for invalid in [
            encoded.replace(&member, &format!("\"{field}\":{{}}")),
            encoded
                .replace(&format!("{member},"), "")
                .replace(&format!(",{member}"), ""),
        ] {
            assert_ne!(invalid, encoded, "{field}");
            assert!(
                serde_json::from_str::<Finding>(&invalid).is_err(),
                "{field}: {invalid}"
            );
        }
    }
}
