use std::collections::BTreeSet;

use amiss_scan::{claim::ClaimCarrier, evaluate::ClaimGroup};
use amiss_wire::report::model::ClaimObserved;

use super::*;

#[test]
fn explicit_claim_and_governed_inputs_receive_the_supplied_policy() {
    let governed = [GovernedSeed {
        document: repo_path("governed.md"),
        member_count: 1,
        sources: vec![ControlStateSource {
            digest: hb("test", b"governed"),
            multiplicity: 1,
        }],
        representative_span: None,
        representative_display: None,
    }];
    let claims = [ClaimGroup {
        kind: FindingKind::ClaimTargetMissing,
        carrier: ClaimCarrier::Definition,
        document: repo_path("README.md"),
        name: "version".to_owned(),
        member_count: 1,
        sources: vec![ControlStateSource {
            digest: hb("test", b"claim"),
            multiplicity: 1,
        }],
        representative_span: None,
        representative_display: None,
        target_path: repo_path("version.txt"),
        line: 1,
        expected_digest: hb("test", b"version"),
        observed: ClaimObserved::TargetAbsent,
        observed_digest: None,
        observed_line: None,
    }];
    let policy = Effects {
        raised: vec![
            (FindingKind::ClaimTargetMissing, Disposition::Fail),
            (FindingKind::UnsupportedCapability, Disposition::Fail),
        ],
        ..Effects::default()
    };
    for (governed, claims, expected) in [
        (&[][..], &[][..], BTreeSet::new()),
        (
            &governed[..],
            &[][..],
            BTreeSet::from([FindingKind::UnsupportedCapability]),
        ),
        (
            &[][..],
            &claims[..],
            BTreeSet::from([FindingKind::ClaimTargetMissing]),
        ),
        (
            &governed[..],
            &claims[..],
            BTreeSet::from([
                FindingKind::UnsupportedCapability,
                FindingKind::ClaimTargetMissing,
            ]),
        ),
    ] {
        let (findings, errors) = evaluate(
            &[],
            &[],
            Profile::Observe,
            &policy,
            GovernedInputs {
                site: &SiteEvaluation::default(),
                governed,
                claims,
                projections: &[],
            },
        )
        .unwrap();
        assert!(errors.is_empty());
        assert_eq!(
            findings
                .iter()
                .map(|finding| finding.key_input.finding_kind)
                .collect::<BTreeSet<_>>(),
            expected
        );
        assert!(
            findings
                .iter()
                .all(|finding| finding.effective_disposition == Disposition::Fail)
        );
    }
}
