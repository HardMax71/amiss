use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueFieldValue {
    pub issue_field_id: UInt,
    pub node_id: String,
    pub data_type: IssueFieldDataType,
    pub value: Nullable<IssueFieldContent>,
    pub issue_field_name: Option<String>,
    pub single_select_option: Option<Nullable<IssueFieldOption>>,
    pub multi_select_options: Option<Nullable<Vec<IssueFieldOption>>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum IssueFieldContent {
    Text(String),
    Number(serde_json::Number),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueFieldOption {
    pub id: UInt,
    pub name: String,
    pub color: String,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum IssueFieldDataType {
    Text,
    SingleSelect,
    MultiSelect,
    Number,
    Date,
}
