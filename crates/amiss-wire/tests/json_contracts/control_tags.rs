use std::collections::BTreeMap;

use amiss_wire::controls::Profile;
use amiss_wire::report::model::{
    ControlStatus, ControlTrustSource, NoControlStatus, SandboxAssurance, SandboxEnforcementSource,
    TrustedTimeTrustSource, VerifiedControlStatus,
};
use amiss_wire::requests::RequestTrust;
use serde::{Serialize, de::DeserializeOwned};

#[test]
fn control_tags_keep_their_wire_spelling_and_require_strings() -> serde_json::Result<()> {
    strings(&[
        (Profile::Observe, "observe"),
        (Profile::EnforceIntroduced, "enforce-introduced"),
        (Profile::Enforce, "enforce"),
    ])?;
    strings(&[
        (
            RequestTrust::ExternalRequiredCheck,
            "external-required-check",
        ),
        (RequestTrust::OrganizationPolicy, "organization-policy"),
    ])?;
    strings(&[
        (ControlStatus::None, "none"),
        (ControlStatus::Verified, "verified"),
    ])?;
    strings(&[
        (ControlTrustSource::None(NoControlStatus::None), "none"),
        (
            ControlTrustSource::Verified(RequestTrust::ExternalRequiredCheck),
            "external-required-check",
        ),
        (
            ControlTrustSource::Verified(RequestTrust::OrganizationPolicy),
            "organization-policy",
        ),
    ])?;
    strings(&[(NoControlStatus::None, "none")])?;
    strings(&[(VerifiedControlStatus::Verified, "verified")])?;
    strings(&[(
        TrustedTimeTrustSource::ExternalRequiredCheck,
        "external-required-check",
    )])?;
    strings(&[
        (SandboxAssurance::ProviderVerified, "provider-verified"),
        (SandboxAssurance::SelfAsserted, "self-asserted"),
    ])?;
    strings(&[
        (
            SandboxEnforcementSource::ExternalRequiredCheck,
            "external-required-check",
        ),
        (SandboxEnforcementSource::LocalProcess, "local-process"),
    ])
}

fn strings<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
    cases: &[(T, &str)],
) -> serde_json::Result<()> {
    for (value, spelling) in cases {
        let encoded = serde_json::to_vec(value)?;
        assert_eq!(encoded, serde_json::to_vec(spelling)?);
        assert_eq!(&serde_json::from_slice::<T>(&encoded)?, value);
        for invalid in [
            serde_json::to_vec(&BTreeMap::from([(*spelling, None::<bool>)]))?,
            serde_json::to_vec(&[*spelling])?,
            serde_json::to_vec(&())?,
            serde_json::to_vec(&false)?,
            serde_json::to_vec(&1)?,
            serde_json::to_vec("unknown")?,
        ] {
            assert!(serde_json::from_slice::<T>(&invalid).is_err(), "{spelling}");
        }
    }
    Ok(())
}
