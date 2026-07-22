use std::fmt;

use sha2::{Digest as _, Sha256};

use super::{SignedTimePolicy, UntrustedDelivery};

const DOMAIN: &[u8] = b"amiss/controller-ingress-request-v1";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct RequestBinding([u8; 32]);

impl RequestBinding {
    pub(crate) fn new(delivery: &UntrustedDelivery<'_>) -> Option<Self> {
        let mut digest = Sha256::new();
        digest.update(DOMAIN);
        digest.update([0]);
        frame(
            &mut digest,
            delivery.route.provider.namespace.as_str().as_bytes(),
        )?;
        frame(
            &mut digest,
            delivery.route.provider.instance.as_str().as_bytes(),
        )?;
        frame(&mut digest, delivery.route.trust_set.as_str().as_bytes())?;
        match delivery.route.signed_time {
            SignedTimePolicy::ReplayOnly => digest.update([0]),
            SignedTimePolicy::Required(max_age) => {
                digest.update([1]);
                digest.update(max_age.as_secs().to_be_bytes());
                digest.update(max_age.subsec_nanos().to_be_bytes());
            }
        }
        digest.update(delivery.received_at_unix_millis.to_be_bytes());
        frame_len(&mut digest, delivery.headers.len())?;
        for header in delivery.headers {
            frame(&mut digest, header.name.as_bytes())?;
            frame(&mut digest, header.value)?;
        }
        frame(&mut digest, delivery.body)?;
        Some(Self(digest.finalize().into()))
    }
}

impl fmt::Debug for RequestBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RequestBinding([REDACTED])")
    }
}

fn frame(digest: &mut Sha256, value: &[u8]) -> Option<()> {
    frame_len(digest, value.len())?;
    digest.update(value);
    Some(())
}

fn frame_len(digest: &mut Sha256, len: usize) -> Option<()> {
    digest.update(u64::try_from(len).ok()?.to_be_bytes());
    Some(())
}
