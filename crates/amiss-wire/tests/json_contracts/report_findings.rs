use amiss_wire::report::{
    Disposition, FixKind,
    model::{
        ByteSpan, DebtApplication, FindingFix, PolicySource, PolicyStep, ReportEnvelope,
        WaiverApplication,
    },
};
use sha2::Digest as _;

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
    let snapshot: amiss_wire::controls::DebtSnapshot = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/debt-snapshot.json"
    ))
    .unwrap();
    let bundle: amiss_wire::controls::WaiverBundle = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/waiver-bundle.json"
    ))
    .unwrap();
    let debt_snapshot_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/debt-snapshot")
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&snapshot).unwrap())
            .finalize()
            .0,
    );
    let waiver_bundle_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/waiver-bundle")
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&bundle).unwrap())
            .finalize()
            .0,
    );
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
