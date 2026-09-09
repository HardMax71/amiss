use amiss_wire::model::Oid;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitCommitRecord {
    pub sha: Oid,
    pub node_id: String,
    pub url: String,
    pub html_url: String,
    pub author: CommitUser,
    pub committer: CommitUser,
    pub message: String,
    pub tree: GitObjectRecord,
    pub parents: Vec<CommitParent>,
    pub verification: CommitVerification,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitObjectRecord {
    pub sha: Oid,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitParent {
    pub sha: Oid,
    pub url: String,
    pub html_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitUser {
    pub name: String,
    pub email: String,
    pub date: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitVerification {
    pub verified: bool,
    pub reason: VerificationReason,
    #[serde(deserialize_with = "Option::deserialize")]
    pub signature: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub payload: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub verified_at: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationReason {
    ExpiredKey,
    NotSigningKey,
    GpgverifyError,
    GpgverifyUnavailable,
    Unsigned,
    UnknownSignatureType,
    NoUser,
    UnverifiedEmail,
    BadEmail,
    UnknownKey,
    MalformedSignature,
    Invalid,
    Valid,
}
