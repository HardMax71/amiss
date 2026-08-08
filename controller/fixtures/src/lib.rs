pub mod clock;
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

const PINNED_PRIVATE_DER_HEX: &str = concat!(
    "308204be020100300d06092a864886f70d0101010500048204a8308204a40201000282010100a2b43a34b3e98ae60190",
    "9fb92b6dbc87b77e5535fbac5e6871e32d2e11293789c1138c1509e80cd416960d260ee949619408c9a11a5085dbbbcc",
    "cd69d92762fde4495cab5467d4bd0dda4be7d954fc966841a4e172e34c447384e0f6aeba892eb242b00cc1051744ca64",
    "bd846e46d58fcedfb06fafae943afaedb7354f2c8930573091f302f539c9c370c7ffc1f563941325c7a618d304b058de",
    "d59dac116412dae680970b8cefbfe010f500ad8035bbc1a9a4b9e0720bc568801f4b87e07bacf68f84a39da5c77c3167",
    "75dbea4567b133adfd2eaaab20c246411de00a240b850329b4cc524fd340f1db1225b7e3f8519246cba8a7381e33ee7e",
    "01246fb83b8d02030100010282010006bd70780313c3ac1bccf12775833c47e32b2f89a6e776bcd15921faf3c8f2ba2a",
    "8e710efc0e3580bec5249b1dae617d982f29eabd115ef68d408e2a05f0d586779de804ccbd73d5f19dd3b8dff9c7cd70",
    "878e87db1105de326ab186d96f51959537dbba5212a498fbe07a31e316ae606678cd19f9de9eaa892dba3c3ac9b931a2",
    "6f116e3a933757009220b7fe2324427d85324cf6480c8fb433b1db7b39fa691e45a59a47edd615b08198cc66716bc16b",
    "d74317a494d15e24e4dfebcaa920ed834c2a38ce46904d3aea7f13d9694275265c50b56639a8c60e4708dd7a3e7628fb",
    "91be4332152338bd662c6ff4242e6925a95ccc0fd8ef238724c16dd96d5e5702818100df07d398594352d5d4138f0fc7",
    "c35d557e8eb01462880a6c3544598ed48b517070263437c780576b958dd6459b5fe7cdfa4cc49f82b356eba4ab2fe296",
    "ed96ebffd993d7de2c773f12b20735a36202038da1d64e196904b61c19d0e52b8babf5a664bf3dae33dfb9fce08b4735",
    "2370d354753af7d65f963a0b0fa9dd2355e8c302818100bac1749526c1f63e129e54ff04a9946317f7103bc443c646dc",
    "975887cc5965e25972d8a185d04df2ee44ad1669794786fddb646adb5f3956f7096fcfe8a0d7fb01d79e970625e151bc",
    "c1c91b44d9c40337baa3b9d6eca4b9dc7200f59654f4e41f04c76b06557a2e1e76cb43b20bb88d39304b1dbc332113ce",
    "532d4a3599856f028181008604b8f462271f5e984a8c7bea090e4bb279e17ace5e7b0cdcd14e93924a894c6c47b8b70f",
    "eda21a66cacd48147e83d77521ae413f93ae9678e3d9296a92284f75f5736e92f5db4e0e58e61628305b8f710b1fb0dc",
    "7a0bb7b69918baacf90802dbd2cc4c2f22c2bc8b250eec621502d62a792b4f04057a4b349c5bf1232b9b6b0281807673",
    "fdde1c9729f87516b812888286fbd3578194670815db1c4f6277bfc57439fd423ae5385ac7162ecaa07e76a7d616692d",
    "9ea3a840ddbdab32f1188e1476e95e61c4d545b10119370032ee78dd26d663a29df661bbf73f6bf3636861d1c102702a",
    "37d24a522d0cd385c5a74a66e4c7ae5e5346a8f84522aadb56fe9ac0a54102818100a5fc41705dfc4940016581d1939d",
    "733f0d6e9bf8516d6e46f3355e5ef06c3ed054e4f70e8d6339681e347c98367ef209b5e96882f351311c3be5ffd77f8a",
    "2f1a3472bc6bde62d8985030cb21c8b1cdbcaa6a82704a4fd70116e00efa86c0221d75a2419f75de319ea69cb4234a8f",
    "4f60acad56fa8d18ca16dbe94285178a4587",
);

const PINNED_PUBLIC_DER_HEX: &str = concat!(
    "30820122300d06092a864886f70d01010105000382010f003082010a0282010100a2b43a34b3e98ae601909fb92b6dbc",
    "87b77e5535fbac5e6871e32d2e11293789c1138c1509e80cd416960d260ee949619408c9a11a5085dbbbcccd69d92762",
    "fde4495cab5467d4bd0dda4be7d954fc966841a4e172e34c447384e0f6aeba892eb242b00cc1051744ca64bd846e46d5",
    "8fcedfb06fafae943afaedb7354f2c8930573091f302f539c9c370c7ffc1f563941325c7a618d304b058ded59dac1164",
    "12dae680970b8cefbfe010f500ad8035bbc1a9a4b9e0720bc568801f4b87e07bacf68f84a39da5c77c316775dbea4567",
    "b133adfd2eaaab20c246411de00a240b850329b4cc524fd340f1db1225b7e3f8519246cba8a7381e33ee7e01246fb83b",
    "8d0203010001",
);

fn hex_bytes(text: &str) -> Vec<u8> {
    let digit = |byte: u8| match byte {
        b'0'..=b'9' => byte.saturating_sub(b'0'),
        b'a'..=b'f' => byte.saturating_sub(b'a').saturating_add(10),
        _ => 0,
    };
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = pair.first().copied().unwrap_or(b'0');
            let low = pair.get(1).copied().unwrap_or(b'0');
            (digit(high) << 4) | digit(low)
        })
        .collect()
}

/// One fixed RSA pair for tests that need a key rather than key generation:
/// the bytes never vary between runs or platforms, keeping signatures and
/// keygen time out of every test's variance. Test material only.
#[must_use]
pub fn pinned_rsa_keys() -> RsaKeys {
    let encoding = EncodeConfig::new().set_line_ending(LineEnding::LF);
    RsaKeys {
        private_pem: encode_config(
            &Pem::new("PRIVATE KEY", hex_bytes(PINNED_PRIVATE_DER_HEX)),
            encoding,
        )
        .into_bytes(),
        public_pem: encode_config(
            &Pem::new("PUBLIC KEY", hex_bytes(PINNED_PUBLIC_DER_HEX)),
            encoding,
        )
        .into_bytes(),
    }
}

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
