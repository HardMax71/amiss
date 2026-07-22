mod malformed;
mod vectors;

pub(super) const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";
pub(super) const NEW_SECRET: &[u8] = b"abcdef0123456789abcdef0123456789";
pub(super) const ID: &[u8] = b"f5e5f430-f57b-4e6e-9fac-d9128cd7232f";
pub(super) const TIMESTAMP: &[u8] = b"1744578123";
pub(super) const BODY: &[u8] = b"{\"object_kind\":\"pipeline\",\"status\":\"success\"}";
pub(super) const SIGNATURE: &[u8] = b"v1,eoSaLtOFqb9PT8wdg5hLQ8m9BxoPEp7HLufb1Anqlzg=";
pub(super) const NEW_SIGNATURE: &[u8] = b"v1,vYa4GH3weRilYqjBhH9AHAlcoKLqbsoIS9Fyn9XU3GQ=";

pub(super) fn headers(signature: &[u8]) -> [amiss_controller::DeliveryHeader<'_>; 3] {
    [
        super::support::header("webhook-id", ID),
        super::support::header("webhook-timestamp", TIMESTAMP),
        super::support::header("webhook-signature", signature),
    ]
}
