use edtf_core::{DateTime, Edtf, Time, TimeShift};
use serde::Serialize;

validated_newtype::validated_newtype! {
    /// Whole-second UTC instant; fixed-width text orders chronologically.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
    #[serde(transparent)]
    String => pub UtcInstant
    if |raw: &str| raw.len() == 20 && matches!(Edtf::parse(raw), Ok(Edtf::DateTime(DateTime {
        time: Time { second: 0..=59, shift: Some(TimeShift::Utc), .. }, ..
    })));
    error "invalid UTC instant"
}
