#![forbid(unsafe_code)]

mod delivery;
mod error;
mod frame;
mod hash;
mod inbox;
mod limits;
mod receiver;
mod record;
mod store;

pub use delivery::{Delivery, DeliveryHeader, IncomingDelivery, IncomingHeader};
pub use error::InboxError;
pub use inbox::{
    ClaimOutcome, ClaimedDelivery, CompleteOutcome, DeliveryLease, EnqueueOutcome, Inbox,
    InboxEntry, InboxState, RenewOutcome, RetryOutcome,
};
pub use limits::InboxLimits;
pub use receiver::{
    AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission, ReceiverConfig,
    ReceiverConfigError, router, serve,
};
