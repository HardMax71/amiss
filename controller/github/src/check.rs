use std::collections::BTreeMap;

use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, MapPreventDuplicates, Same, TryFromInto};

use crate::owner::OwnerRecord;

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRunApp {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub node_id: String,
    pub owner: AppOwner,
    pub name: String,
    pub description: Nullable<String>,
    pub external_url: String,
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(with = "As::<MapPreventDuplicates<Same, Same>>")]
    pub permissions: BTreeMap<String, String>,
    pub events: Vec<String>,
    pub slug: Option<String>,
    pub client_id: Option<String>,
    pub installations_count: Option<UInt>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum AppOwner {
    Account(Box<OwnerRecord>),
    Enterprise(Box<EnterpriseRecord>),
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnterpriseRecord {
    pub id: UInt,
    pub node_id: String,
    pub name: String,
    pub slug: String,
    pub html_url: String,
    pub created_at: Nullable<String>,
    pub updated_at: Nullable<String>,
    pub avatar_url: String,
    pub description: Option<Nullable<String>>,
    pub website_url: Option<Nullable<String>>,
}
