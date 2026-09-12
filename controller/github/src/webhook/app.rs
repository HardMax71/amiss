use js_int::UInt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct WebhookApp {
    #[serde(deserialize_with = "Option::deserialize")]
    pub id: Option<UInt>,
}
