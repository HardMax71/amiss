use amiss_wire::envelope::Payload as _;
use amiss_wire::model::Digest;
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::publication::DocsCandidate;
use amiss_wire::report::model::ReportPayload;

pub(crate) const REPORT: &[u8] = include_bytes!("../../../spec/examples/scanner-report.json");

/// The accepted report every audit fixture binds, with the candidate the
/// controller derives from it rather than from the plan.
pub(crate) struct ReportBinding {
    pub(crate) report: Vec<u8>,
    pub(crate) payload_digest: Digest,
    pub(crate) docs: DocsCandidate,
}

pub(crate) fn report_binding() -> Option<ReportBinding> {
    let report = REPORT.to_vec();
    let payload_digest = <ReportPayload>::parse(&report).ok()?.payload_digest;
    Some(ReportBinding {
        report,
        payload_digest,
        docs: DocsCandidate {
            repository: RepositoryIdentity::new(
                "git.example.internal".to_owned(),
                "group/subgroup".to_owned(),
                "widget".to_owned(),
            )?,
            object_format: ObjectFormat::Sha1,
            commit: Oid::new(
                ObjectFormat::Sha1,
                "d1a175a1986230e4ba44b6f6ed67c8dbccb29aaf".to_owned(),
            )?,
            tree: Oid::new(
                ObjectFormat::Sha1,
                "7eed0bc378155f11543b2261997a1f363557e8cd".to_owned(),
            )?,
            candidate_identity_digest: Digest::from_wire(
                "sha256:8c8f4c8087edf216675ffbfc5a75a6c67dc48103be696b74174758a3e5db187a",
            )?,
        },
    })
}
