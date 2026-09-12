#![cfg(test)]

use super::SiteObservation;

#[test]
fn every_site_observation_requires_named_object_fields() {
    let cases: [(&[u8], &[u8]); 4] = [
        (
            br#"{"kind":"site-route","route":"/guide/","source":"docs/guide.md","anchors":[]}"#,
            br#"["site-route","/guide/","docs/guide.md",[]]"#,
        ),
        (
            br#"{"kind":"site-generated-route","route":"/guide/","source":null,"anchors":[]}"#,
            br#"["site-generated-route","/guide/",null,[]]"#,
        ),
        (
            br#"{"kind":"site-redirect","route":"/old/","source":"docs/old.md","destination":"/new/"}"#,
            br#"["site-redirect","/old/","docs/old.md","/new/"]"#,
        ),
        (
            br#"{"kind":"site-navigation","root":null,"manifest":"SUMMARY.md","entrypoints":["/"],"reachable":["guide.md"]}"#,
            br#"["site-navigation",null,"SUMMARY.md",["/"],["guide.md"]]"#,
        ),
    ];
    for (object, positional) in cases {
        assert!(amiss_wire::codec::decode::<SiteObservation>(object).is_ok());
        assert!(amiss_wire::codec::decode::<SiteObservation>(positional).is_err());
    }
}
