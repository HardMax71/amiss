use amiss_wire::controls::Profile;
use amiss_wire::report::model::{
    ControlStatus, ControlTrustSource, NoControlStatus, SandboxAssurance, SandboxEnforcementSource,
    TrustedTimeTrustSource, VerifiedControlStatus,
};
use amiss_wire::requests::RequestTrust;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

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
        (ControlTrustSource::None, "none"),
        (
            ControlTrustSource::ExternalRequiredCheck,
            "external-required-check",
        ),
        (
            ControlTrustSource::OrganizationPolicy,
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
        let encoded = serde_json::to_value(value)?;
        assert_eq!(encoded, json!(spelling));
        assert_eq!(&serde_json::from_value::<T>(encoded)?, value);
        for invalid in [
            json!({*spelling: null}),
            json!([spelling]),
            Value::Null,
            json!(false),
            json!(1),
            json!("unknown"),
        ] {
            assert!(serde_json::from_value::<T>(invalid).is_err(), "{spelling}");
        }
    }
    Ok(())
}
