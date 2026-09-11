use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct UserRecord {
    pub id: u64,
    pub login: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserVisibility {
    Public,
    Limited,
    Private,
}
