use amiss_wire::model::ObjectFormat;
use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::user::UserRecord;

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct RepositoryRecord {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: UserRecord,
    pub default_branch: String,
    pub object_format_name: ObjectFormat,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_manual_merge: Option<bool>,
}
