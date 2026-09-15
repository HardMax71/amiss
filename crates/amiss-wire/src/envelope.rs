use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use crate::de::{self, Error, ErrorKind};
use crate::model::Digest;

/// The domain-separated digest of one value's canonical bytes. Every digest in
/// the system starts with the label naming its purpose, so a digest computed
/// for one context cannot be replayed as a digest for another.
#[must_use]
pub fn document_digest<T: Serialize>(domain: &str, value: &T) -> Option<Digest> {
    let mut writer =
        digest_io::IoWrapper(sha2::Sha256::new_with_prefix(domain).chain_update([0_u8]));
    serde_json_canonicalizer::to_writer(value, &mut writer).ok()?;
    Some(Digest::from(writer.0.finalize().0))
}

/// The same digest over canonical bytes already spelled, so a payload spelled
/// once is hashed and written without a second pass.
#[must_use]
pub fn sealed_digest(domain: &str, canonical: &[u8]) -> Digest {
    Digest::from(
        sha2::Sha256::new_with_prefix(domain)
            .chain_update([0_u8])
            .chain_update(canonical)
            .finalize()
            .0,
    )
}

/// The same digest over a document as received, canonicalized while it is read
/// so a reordered or reformatted input cannot borrow another document's digest.
#[must_use]
pub fn transcoded_digest(domain: &str, json: &[u8]) -> Option<Digest> {
    let mut input = serde_json::Deserializer::from_slice(json);
    document_digest(domain, &serde_transcode::Transcoder::new(&mut input))
}

/// A document payload that is sealed under its own schema domain.
pub trait Payload: Serialize + Sized {
    /// The closed tag the carrying document announces itself under.
    type Schema;
    /// What a rejected document reports; `Error` unless a payload carries
    /// more than a path and a kind.
    type Defect: From<Error>;
    /// The domain the payload digest is taken under.
    const DOMAIN: &'static str;
    /// The ceiling on the encoded document that carries this payload.
    const DOCUMENT_BYTES: u64;
    /// What the payload digest covers.
    const SEALING: Sealing = Sealing::Typed;
    /// How the canonical bytes are produced.
    const SPELLING: Spelling = Spelling::Canonicalizer;

    /// Checks the payload's closed grammar.
    ///
    /// # Errors
    ///
    /// A public field violates the grammar the document's reader enforces.
    fn validate(&self) -> Result<(), Self::Defect> {
        Ok(())
    }

    /// The digest this payload is sealed under.
    ///
    /// # Errors
    ///
    /// Refuses a payload that violates its grammar or does not serialize.
    fn digest(&self) -> Result<Digest, Self::Defect> {
        self.validate()?;
        document_digest(Self::DOMAIN, self)
            .ok_or_else(|| Error::new("$.payload", ErrorKind::InvalidValue).into())
    }

    /// The canonical spelling of this payload, the bytes its digest covers.
    ///
    /// # Errors
    ///
    /// Refuses a payload that does not serialize.
    fn spell(&self) -> Result<Vec<u8>, Self::Defect> {
        match Self::SPELLING {
            Spelling::Canonicalizer => serde_json_canonicalizer::to_vec(self),
            Spelling::SortedKeys => {
                serde_json::to_value(self).and_then(|value| serde_json::to_vec(&value))
            }
        }
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue).into())
    }

    /// Seals this payload under its schema and encodes the canonical document,
    /// spelling the payload once for both the digest and the wire.
    ///
    /// # Errors
    ///
    /// Refuses a payload that violates its grammar, or a document over its ceiling.
    fn emit(&self) -> Result<Vec<u8>, Self::Defect>
    where
        Self::Schema: Default + Serialize,
    {
        self.validate()?;
        let payload = self.spell()?;
        let payload_digest = sealed_digest(Self::DOMAIN, &payload);
        let mut canonical = Vec::new();
        write_sealed(
            &Self::Schema::default(),
            &payload,
            payload_digest,
            &mut canonical,
        )
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
        if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > Self::DOCUMENT_BYTES {
            return Err(Error::new("$", ErrorKind::LimitExceeded).into());
        }
        Ok(canonical)
    }

    /// Reads one sealed document carrying this payload.
    ///
    /// # Errors
    ///
    /// Refuses an oversized or malformed document, an unknown or malformed
    /// field, a payload digest that does not recompute, and a payload that
    /// violates its grammar.
    fn parse(bytes: &[u8]) -> Result<Envelope<Self>, Self::Defect>
    where
        Self: DeserializeOwned,
        Self::Schema: DeserializeOwned + Serialize,
    {
        match Self::SEALING {
            Sealing::Typed => {
                let document: Envelope<Self> = de::read(bytes, Self::DOCUMENT_BYTES)?;
                document.validate()?;
                Ok(document)
            }
            Sealing::Exact => {
                let document: Envelope<Self> = de::read(bytes, Self::DOCUMENT_BYTES)?;
                let spelled = document_digest(Self::DOMAIN, &document)
                    .ok_or_else(|| Error::new("$", ErrorKind::InvalidValue))?;
                if transcoded_digest(Self::DOMAIN, bytes) != Some(spelled) {
                    return Err(Error::new("$", ErrorKind::Noncanonical).into());
                }
                document.sealed()?;
                document.payload.validate()?;
                Ok(document)
            }
            Sealing::Received => {
                let received: Sealed<'_, Self::Schema> = de::read(bytes, Self::DOCUMENT_BYTES)?;
                let sealed = transcoded_digest(Self::DOMAIN, received.payload.get().as_bytes())
                    .ok_or_else(|| Error::new("$.payload", ErrorKind::InvalidValue))?;
                let mut input = serde_json::Deserializer::from_str(received.payload.get());
                let payload: Self = serde_path_to_error::deserialize(&mut input)
                    .map_err(|defect| de::deserialize_error("$.payload", &defect))?;
                if sealed != received.payload_digest {
                    return Err(Error::new("$.payload_digest", ErrorKind::DigestMismatch).into());
                }
                payload.validate()?;
                Ok(Envelope {
                    payload,
                    payload_digest: received.payload_digest,
                    schema: received.schema,
                })
            }
        }
    }
}

/// How a payload spells its canonical bytes. The sorted value tree is RFC 8785
/// whenever every object key is a model field name and every number an
/// integer; a payload carrying open JSON objects keeps the canonicalizer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spelling {
    Canonicalizer,
    SortedKeys,
}

/// What a payload digest covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sealing {
    /// The canonical spelling of the typed payload. A closed document is
    /// exactly what its model spells, and its consumers recompute the digest
    /// from the typed value, so no other spelling is that document.
    Typed,
    /// The typed spelling, and the document has to arrive in it: a value the
    /// model would read and write back differently, such as a struct spelled
    /// as a sequence or an omitted null, is refused before the seal is checked.
    Exact,
    /// The payload bytes as received, so additive fields the model does not
    /// carry stay under the seal.
    Received,
}

/// A sealed document with its payload as bytes: as they arrived, before the
/// seal is checked, or as spelled once for the wire. The members sit in key
/// order, so a plain serialization is already the canonical one.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sealed<'a, S> {
    #[serde(borrow)]
    payload: &'a serde_json::value::RawValue,
    payload_digest: Digest,
    schema: S,
}

/// Writes one sealed document around a payload already spelled canonically.
///
/// # Errors
///
/// Refuses payload bytes that are not one JSON value, and returns the first
/// output error.
pub fn write_sealed<S: Serialize>(
    schema: &S,
    payload: &[u8],
    payload_digest: Digest,
    output: &mut impl std::io::Write,
) -> std::io::Result<()> {
    let text = std::str::from_utf8(payload)
        .map_err(|defect| std::io::Error::new(std::io::ErrorKind::InvalidData, defect))?;
    let payload: &serde_json::value::RawValue = serde_json::from_str(text)?;
    serde_json::to_writer(
        output,
        &Sealed {
            payload,
            payload_digest,
            schema,
        },
    )?;
    Ok(())
}

impl<T: Payload> Payload for &T {
    type Schema = T::Schema;
    type Defect = T::Defect;
    const DOMAIN: &'static str = T::DOMAIN;
    const DOCUMENT_BYTES: u64 = T::DOCUMENT_BYTES;

    fn validate(&self) -> Result<(), Self::Defect> {
        T::validate(self)
    }
}

/// One sealed document: a payload, the schema that names it, and the digest
/// that binds the two. The members sit in key order, so a plain serialization
/// is already the canonical one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T: Payload> {
    pub payload: T,
    pub payload_digest: Digest,
    pub schema: T::Schema,
}

impl<T: Payload> Envelope<T> {
    /// Checks the payload grammar, the encoded ceiling, and the payload binding.
    ///
    /// # Errors
    ///
    /// Refuses an invalid payload, an oversized document, or a payload digest
    /// that does not recompute.
    pub fn validate(&self) -> Result<(), T::Defect>
    where
        T::Schema: Serialize,
    {
        self.payload.validate()?;
        let payload = self.payload.spell()?;
        if sealed_digest(T::DOMAIN, &payload) != self.payload_digest {
            return Err(Error::new("$.payload_digest", ErrorKind::DigestMismatch).into());
        }
        let mut measured = countio::Counter::new(std::io::sink());
        write_sealed(&self.schema, &payload, self.payload_digest, &mut measured)
            .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
        if u64::try_from(measured.writer_bytes()).unwrap_or(u64::MAX) > T::DOCUMENT_BYTES {
            return Err(Error::new("$", ErrorKind::LimitExceeded).into());
        }
        Ok(())
    }

    fn sealed(&self) -> Result<(), T::Defect> {
        let digest = document_digest(T::DOMAIN, &self.payload)
            .ok_or_else(|| Error::new("$.payload", ErrorKind::InvalidValue))?;
        if digest != self.payload_digest {
            return Err(Error::new("$.payload_digest", ErrorKind::DigestMismatch).into());
        }
        Ok(())
    }
}

/// The shell of a `T` document with the payload elided, for measuring what
/// the seal itself costs.
impl<T: Payload> Payload for PhantomData<T> {
    type Schema = T::Schema;
    type Defect = T::Defect;
    const DOMAIN: &'static str = T::DOMAIN;
    const DOCUMENT_BYTES: u64 = T::DOCUMENT_BYTES;
}
