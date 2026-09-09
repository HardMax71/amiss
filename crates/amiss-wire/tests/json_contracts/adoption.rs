use amiss_wire::controls::EligibleFindingKind;
use amiss_wire::report::FindingKind;
use strum::IntoEnumIterator;

#[test]
fn eligible_kind_conversion_matches_the_closed_wire_contract() {
    let mut eligible = 0;
    for kind in FindingKind::iter() {
        let encoded = serde_json::to_vec(&kind).unwrap();
        let wire = serde_json::from_slice::<EligibleFindingKind>(&encoded);
        let direct = EligibleFindingKind::try_from(&kind);
        assert_eq!(direct.ok(), wire.ok(), "{kind}");
        if let Ok(accepted) = direct {
            assert_eq!(serde_json::to_vec(&accepted).unwrap(), encoded);
            eligible += 1;
        }
    }
    assert_eq!(eligible, EligibleFindingKind::iter().count());
}
