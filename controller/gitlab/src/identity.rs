use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use url::Url;

pub(crate) fn parse_change_id(raw: &str) -> Option<(u64, u64)> {
    parse_pair(raw, "project", "merge-request")
}

pub(crate) fn parse_run_id(raw: &str) -> Option<(u64, u64)> {
    parse_pair(raw, "pipeline", "job")
}

pub(crate) fn parse_delivery_id(raw: &str) -> Option<u64> {
    let mut fields = raw.split('/');
    (fields.next()? == "oidc").then_some(())?;
    (fields.next()? == "runner").then_some(())?;
    let runner_id = positive(fields.next()?.parse().ok()?)?;
    (fields.next()? == "jti").then_some(())?;
    let digest = fields.next()?;
    (!digest.is_empty() && fields.next().is_none()).then_some(runner_id)
}

pub(crate) fn exact_sha1(raw: &str) -> Option<Oid> {
    Oid::new(ObjectFormat::Sha1, raw.to_owned())
}

pub(crate) fn branch_ref(branch: &str) -> Option<BranchRef> {
    BranchRef::new(format!("refs/heads/{branch}"))
}

pub(crate) fn train_ref(merge_request_iid: u64) -> String {
    format!("refs/merge-requests/{merge_request_iid}/train")
}

pub(crate) fn repository_identity(host: &str, project_path: &str) -> Option<RepositoryIdentity> {
    let (owner, name) = project_path.rsplit_once('/')?;
    RepositoryIdentity::new(host.to_owned(), owner.to_owned(), name.to_owned())
}

pub(crate) fn canonical_project_path(raw: &str) -> Option<String> {
    let canonical = raw.to_ascii_lowercase();
    repository_identity("gitlab.invalid", &canonical)
        .filter(|identity| !identity.owner.is_empty() && !identity.name.is_empty())
        .map(|_identity| canonical)
}

pub(crate) fn repository_url(host: &str, project_path: &str) -> Option<String> {
    let raw = format!("https://{host}/{project_path}.git");
    let parsed = Url::parse(&raw).ok()?;
    (parsed.scheme() == "https"
        && parsed.host_str() == Some(host)
        && parsed.port().is_none()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none())
    .then_some(raw)
}

pub(crate) fn canonical_repository(repository: &RepositoryIdentity) -> bool {
    let path = format!("{}/{}", repository.owner, repository.name);
    canonical_host(&repository.host)
        && canonical_project_path(&path).as_deref() == Some(path.as_str())
        && repository_identity(&repository.host, &path).as_ref() == Some(repository)
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

fn positive(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

fn parse_pair(raw: &str, first_label: &str, second_label: &str) -> Option<(u64, u64)> {
    let mut fields = raw.split('/');
    (fields.next()? == first_label).then_some(())?;
    let first = positive(fields.next()?.parse().ok()?)?;
    (fields.next()? == second_label).then_some(())?;
    let second = positive(fields.next()?.parse().ok()?)?;
    fields.next().is_none().then_some((first, second))
}
