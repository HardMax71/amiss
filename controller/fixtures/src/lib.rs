pub mod config;
pub mod lane;

use std::fmt;

use aws_lc_rs::encoding::{AsDer, Pkcs8V1Der, PublicKeyX509Der};
use aws_lc_rs::rsa::{KeyPair, KeySize};
use aws_lc_rs::signature::KeyPair as _;
use pem::{EncodeConfig, LineEnding, Pem, encode_config};

pub struct RsaKeys {
    pub private_pem: Vec<u8>,
    pub public_pem: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixtureError;

impl fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the RSA fixture could not be generated")
    }
}

impl std::error::Error for FixtureError {}

/// Generates one 2048-bit RSA pair for a provider test process.
///
/// # Errors
///
/// Returns an error when key generation or DER serialization fails.
pub fn rsa_keys() -> Result<RsaKeys, FixtureError> {
    let pair = KeyPair::generate(KeySize::Rsa2048).map_err(|_defect| FixtureError)?;
    let private = AsDer::<Pkcs8V1Der<'static>>::as_der(&pair).map_err(|_defect| FixtureError)?;
    let public = AsDer::<PublicKeyX509Der<'static>>::as_der(pair.public_key())
        .map_err(|_defect| FixtureError)?;
    let encoding = EncodeConfig::new().set_line_ending(LineEnding::LF);
    Ok(RsaKeys {
        private_pem: encode_config(&Pem::new("PRIVATE KEY", private.as_ref()), encoding)
            .into_bytes(),
        public_pem: encode_config(&Pem::new("PUBLIC KEY", public.as_ref()), encoding).into_bytes(),
    })
}
