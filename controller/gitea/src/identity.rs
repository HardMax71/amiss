use amiss_controller::{
    ChangeLocator, IntegrationId, ProviderRun, ProviderRunAttempt, ProviderRunIdentity,
};
use amiss_wire::model::{BranchRef, ObjectFormat, Oid};
use sha2::Digest as _;

const RUN_DOMAIN: &str = "amiss/controller-gitea-family-pull-request-v2";

pub(crate) fn provider_run(
    reviewer: &IntegrationId,
    change: &ChangeLocator,
    candidate: &Oid,
    candidate_ref: &BranchRef,
    target_ref: &BranchRef,
) -> Option<ProviderRunIdentity> {
    let fields = serde_json::to_vec(&(
        reviewer.as_str(),
        change.provider.namespace.as_str(),
        change.repository.host(),
        change.repository.owner(),
        change.repository.name(),
        change.change,
        candidate.as_str(),
        candidate_ref.as_str(),
        target_ref.as_str(),
    ))
    .ok()?;
    ProviderRunIdentity::new(
        ProviderRun::PullRequest(amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(RUN_DOMAIN)
                .chain_update([0_u8])
                .chain_update(&fields)
                .finalize()
                .0,
        )),
        ProviderRunAttempt::FIRST,
        ObjectFormat::Sha1,
        candidate.clone(),
    )
}

pub(crate) fn positive(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

pub(crate) fn branch_ref(branch: &str) -> Option<BranchRef> {
    BranchRef::try_from(format!("refs/heads/{branch}")).ok()
}

pub(crate) fn canonical_segment(raw: &str) -> Option<String> {
    let canonical = raw.to_ascii_lowercase();
    (!canonical.is_empty() && canonical.len() <= 100 && !canonical.contains('/'))
        .then_some(canonical)
}

pub(crate) fn canonical_host(host: &str) -> bool {
    host.len() <= 253
        && host.as_bytes().split(|byte| *byte == b'.').all(|label| {
            (1..=63).contains(&label.len())
                && label.first().is_some_and(u8::is_ascii_alphanumeric)
                && label.last().is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .iter()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        })
}
