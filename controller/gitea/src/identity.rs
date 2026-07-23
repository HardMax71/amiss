use amiss_controller::{
    ChangeId, ChangeLocator, IntegrationId, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid};

const RUN_DOMAIN: &str = "amiss/controller-gitea-family-pull-request-v1";

pub(crate) fn provider_run(
    reviewer: &IntegrationId,
    change: &ChangeLocator,
    candidate: &Oid,
    candidate_ref: &BranchRef,
    target_ref: &BranchRef,
) -> Option<ProviderRunIdentity> {
    let fields = serde_json::to_vec(&[
        reviewer.as_str(),
        change.provider.namespace.as_str(),
        change.repository.host.as_str(),
        change.repository.owner.as_str(),
        change.repository.name.as_str(),
        change.change.as_str(),
        candidate.as_str(),
        candidate_ref.as_str(),
        target_ref.as_str(),
    ])
    .ok()?;
    ProviderRunIdentity::new(
        ProviderRunId::new(format!("pr:{}", hb(RUN_DOMAIN, &fields)))?,
        ProviderRunAttempt::new(1)?,
        ObjectFormat::Sha1,
        candidate.clone(),
    )
}

pub(crate) fn positive(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

pub(crate) fn change_id(repository_id: u64, pull_request_id: u64, number: u64) -> Option<ChangeId> {
    ChangeId::new(format!(
        "repository/{repository_id}/pull/{pull_request_id}/number/{number}"
    ))
}

pub(crate) fn parse_change_id(raw: &str) -> Option<(u64, u64, u64)> {
    let mut fields = raw.split('/');
    (fields.next()? == "repository").then_some(())?;
    let repository_id = fields.next()?.parse().ok().and_then(positive)?;
    (fields.next()? == "pull").then_some(())?;
    let pull_request_id = fields.next()?.parse().ok().and_then(positive)?;
    (fields.next()? == "number").then_some(())?;
    let number = fields.next()?.parse().ok().and_then(positive)?;
    fields
        .next()
        .is_none()
        .then_some((repository_id, pull_request_id, number))
}

pub(crate) fn branch_ref(branch: &str) -> Option<BranchRef> {
    BranchRef::new(format!("refs/heads/{branch}"))
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
