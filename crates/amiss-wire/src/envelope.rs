use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use crate::de::{self, Error, ErrorKind, fail};
use crate::model::Digest;

/// A document payload that is sealed under its own schema domain.
pub trait Payload: Serialize + Sized {
    /// The closed tag the carrying document announces itself under.
    type Schema;
    /// The domain the payload digest is taken under.
    const DOMAIN: &'static str;
    /// The ceiling on the encoded document that carries this payload.
    const DOCUMENT_BYTES: u64;

    /// Checks the payload's closed grammar.
    ///
    /// # Errors
    ///
    /// A public field violates the grammar the document's reader enforces.
    fn validate(&self) -> Result<(), Error> {
        Ok(())
    }

    /// The digest this payload is sealed under.
    ///
    /// # Errors
    ///
    /// Refuses a payload that violates its grammar or does not serialize.
    fn digest(&self) -> Result<Digest, Error> {
        self.validate()?;
        let mut writer =
            digest_io::IoWrapper(sha2::Sha256::new_with_prefix(Self::DOMAIN).chain_update([0_u8]));
        serde_json_canonicalizer::to_writer(self, &mut writer)
            .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))?;
        Ok(Digest::from(writer.0.finalize().0))
    }

    /// Seals this payload under its schema and encodes the canonical document.
    ///
    /// # Errors
    ///
    /// Refuses a payload that violates its grammar, or a document over its ceiling.
    fn emit(&self) -> Result<Vec<u8>, Error>
    where
        Self::Schema: Default + Serialize,
    {
        let sealed = Envelope {
            schema: Self::Schema::default(),
            payload_digest: self.digest()?,
            payload: self,
        };
        let canonical = serde_json_canonicalizer::to_vec(&sealed)
            .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
        if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > Self::DOCUMENT_BYTES {
            return fail("$", ErrorKind::LimitExceeded);
        }
        Ok(canonical)
    }

    /// Reads one sealed document carrying this payload.
    ///
    /// # Errors
    ///
    /// Refuses an oversized or malformed document, an unknown or malformed
    /// field, a payload that violates its grammar, and a payload digest that
    /// does not recompute.
    fn parse(bytes: &[u8]) -> Result<Envelope<Self>, Error>
    where
        Self: DeserializeOwned,
        Self::Schema: DeserializeOwned + Serialize,
    {
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > Self::DOCUMENT_BYTES {
            return fail("$", ErrorKind::LimitExceeded);
        }
        let mut input = serde_json::Deserializer::from_slice(bytes);
        let document: Envelope<Self> = serde_path_to_error::deserialize(&mut input)
            .map_err(|defect| de::deserialize_error("$", &defect))?;
        input
            .end()
            .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))?;
        document.validate()?;
        Ok(document)
    }
}

impl<T: Payload> Payload for &T {
    type Schema = T::Schema;
    const DOMAIN: &'static str = T::DOMAIN;
    const DOCUMENT_BYTES: u64 = T::DOCUMENT_BYTES;

    fn validate(&self) -> Result<(), Error> {
        T::validate(self)
    }
}

/// One sealed document: a payload, the schema that names it, and the digest
/// that binds the two.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T: Payload> {
    pub schema: T::Schema,
    pub payload: T,
    pub payload_digest: Digest,
}

impl<T: Payload> Envelope<T> {
    /// Checks the payload grammar, the payload binding, and the encoded ceiling.
    ///
    /// # Errors
    ///
    /// Refuses an invalid payload, an oversized document, or a payload digest
    /// that does not recompute.
    pub fn validate(&self) -> Result<(), Error>
    where
        T::Schema: Serialize,
    {
        let digest = self.payload.digest()?;
        let mut measured = countio::Counter::new(std::io::sink());
        serde_json_canonicalizer::to_writer(self, &mut measured)
            .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
        if u64::try_from(measured.writer_bytes()).unwrap_or(u64::MAX) > T::DOCUMENT_BYTES {
            return fail("$", ErrorKind::LimitExceeded);
        }
        if digest != self.payload_digest {
            return fail("$.payload_digest", ErrorKind::DigestMismatch);
        }
        Ok(())
    }
}
