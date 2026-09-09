use amiss_scan::policy::DebtContext;
use amiss_wire::{
    controls::{
        canonical_debt_snapshot, canonical_fact, canonical_trusted_time, canonical_waiver_bundle,
    },
    report::model::{DebtApplication, PolicySource, WaiverApplication},
    requests::RequestTrust,
};

use super::*;

#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "published exception fixtures"
)]
fn policy() -> (Vec<Comparison>, Effects) {
    let fixture = missing_spec("d.md", "absent.md");
    let comparisons = comparisons(Vec::new(), vec![observation(&fixture)]);
    let finding = only(
        evaluate(&[], &comparisons, Profile::Observe).unwrap(),
        FindingKind::ExplicitTargetMissing,
    );
    let mut fact = waived_fact();
    fact.key_input.scope.occurrence.source_projection_digest =
        hb("amiss/scanner-source-projection", fixture.block.as_bytes());
    let (_, fact_digest) = canonical_fact(&fact).unwrap();
    assert_eq!(fact_digest, finding.candidate_fact.unwrap().digest);
    let mut snapshot: amiss_wire::controls::DebtSnapshot = serde_json::from_slice(include_bytes!(
        "../../../../../spec/examples/debt-snapshot.json"
    ))
    .unwrap();
    let debt = &mut snapshot.items[0];
    debt.finding_key = finding.finding_key;
    debt.accepted_fact_digest = fact_digest;
    debt.accepted_fact = fact.clone();
    let mut bundle: amiss_wire::controls::WaiverBundle = serde_json::from_slice(include_bytes!(
        "../../../../../spec/examples/waiver-bundle.json"
    ))
    .unwrap();
    let waiver = &mut bundle.items[0];
    waiver.finding_key = finding.finding_key;
    waiver.authorized_fact_digest = debt.accepted_fact_digest;
    waiver.authorized_fact = fact;
    let issuer = waiver.issuer.clone();
    let candidate_tree = waiver.candidate_tree.clone();
    let (_, debt_digest) = canonical_debt_snapshot(&snapshot).unwrap();
    let (_, waiver_digest) = canonical_waiver_bundle(&bundle).unwrap();
    let mut statement: amiss_wire::controls::TrustedTimeStatement = serde_json::from_slice(
        include_bytes!("../../../../../spec/examples/scanner-trusted-time-statement.json"),
    )
    .unwrap();
    statement.evaluation_instant =
        amiss_wire::model::UtcInstant::try_from("2026-07-11T00:00:00Z".to_owned()).unwrap();
    statement.valid_until =
        amiss_wire::model::UtcInstant::try_from("2026-07-11T00:10:00Z".to_owned()).unwrap();
    let (_, digest) = canonical_trusted_time(&statement).unwrap();
    (
        comparisons,
        Effects {
            debt: Some(DebtContext {
                digest: debt_digest,
                trust_source: RequestTrust::OrganizationPolicy,
                adoption_tree: snapshot.adoption_tree,
                items: snapshot.items,
            }),
            waiver: Some(WaiverContext {
                digest: waiver_digest,
                trust_source: RequestTrust::OrganizationPolicy,
                candidate_tree,
                items: bundle.items,
                authorized_issuers: vec![issuer],
                waivable_kinds: vec![
                    amiss_wire::controls::EligibleFindingKind::ExplicitTargetMissing,
                ],
            }),
            time: Some(TimeContext { statement, digest }),
            ..Effects::default()
        },
    )
}

#[test]
fn applied_exceptions_carry_exact_provenance_and_disposition_steps() {
    let (comparisons, policy) = policy();
    let debt = policy.debt.as_ref().unwrap();
    let waiver = policy.waiver.as_ref().unwrap();
    let debt_item = &debt.items[0];
    let waiver_item = &waiver.items[0];
    let expected_debt = DebtApplication {
        accepted_fact_digest: debt_item.accepted_fact_digest,
        adoption_tree: debt.adoption_tree.clone(),
        created_at: debt_item.created_at.clone(),
        debt_id: debt_item.debt_id.clone(),
        debt_snapshot_digest: debt.digest,
        expires_at: debt_item.expires_at.clone(),
        owner: debt_item.owner.clone(),
        reason: debt_item.reason.clone(),
    };
    let expected_waiver = WaiverApplication {
        authorized_fact_digest: waiver_item.authorized_fact_digest,
        candidate_tree: waiver_item.candidate_tree.clone(),
        created_at: waiver_item.created_at.clone(),
        expires_at: waiver_item.expires_at.clone(),
        issuer: waiver_item.issuer.clone(),
        not_before: waiver_item.not_before.clone(),
        owner: waiver_item.owner.clone(),
        reason: waiver_item.reason.clone(),
        residual_disposition: waiver_item.residual_disposition,
        waiver_bundle_digest: waiver.digest,
        waiver_id: waiver_item.waiver_id.clone(),
    };
    for profile in [Profile::Observe, Profile::Enforce] {
        for (policy, expected_debt, expected_waiver, source) in [
            (
                Effects {
                    waiver: None,
                    ..policy.clone()
                },
                Some(expected_debt.clone()),
                None,
                PolicySource::DebtSnapshot,
            ),
            (
                Effects {
                    debt: None,
                    ..policy.clone()
                },
                None,
                (profile == Profile::Enforce).then_some(expected_waiver.clone()),
                PolicySource::WaiverBundle,
            ),
        ] {
            let (findings, errors) =
                evaluate_with_policy(&[], &comparisons, profile, &policy, &[], &[]).unwrap();
            assert!(errors.is_empty());
            let finding = only(findings, FindingKind::ExplicitTargetMissing);
            assert_eq!(finding.debt, expected_debt);
            assert_eq!(finding.waiver, expected_waiver);
            assert_eq!(finding.effective_disposition, Disposition::Warn);
            let steps: Vec<_> = finding
                .steps
                .iter()
                .filter(|step| step.source == source)
                .collect();
            if finding.debt.is_some() || finding.waiver.is_some() {
                assert_eq!(steps.len(), 1);
                assert_eq!(steps[0].before, finding.configured_disposition);
                assert_eq!(steps[0].after, Disposition::Warn);
            } else {
                assert!(steps.is_empty());
            }
        }
    }
}

#[test]
fn overlapping_exceptions_carry_neither_application() {
    let (comparisons, policy) = policy();
    let (findings, errors) =
        evaluate_with_policy(&[], &comparisons, Profile::Enforce, &policy, &[], &[]).unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].code,
        amiss_wire::report::AnalysisErrorCode::ExceptionOverlap
    );
    let finding = only(findings, FindingKind::ExplicitTargetMissing);
    assert_eq!((finding.debt, finding.waiver), (None, None));
    assert_eq!(finding.effective_disposition, Disposition::Fail);
    assert!(finding.steps.iter().all(|step| !matches!(
        step.source,
        PolicySource::DebtSnapshot | PolicySource::WaiverBundle
    )));
}
