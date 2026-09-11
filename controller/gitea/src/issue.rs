use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
}
