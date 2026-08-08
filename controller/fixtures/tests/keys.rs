use amiss_controller_fixtures::rsa_keys;
use aws_lc_rs::rsa::{KeyPair, PublicKey};

#[test]
fn generated_keys_are_pkcs8_and_spki() {
    let keys = rsa_keys().unwrap();
    let replacement = rsa_keys().unwrap();
    let private = pem::parse(&keys.private_pem).unwrap();
    let public = pem::parse(&keys.public_pem).unwrap();

    assert_ne!(keys.private_pem, replacement.private_pem);
    assert_ne!(keys.public_pem, replacement.public_pem);
    assert_eq!(private.tag(), "PRIVATE KEY");
    assert_eq!(public.tag(), "PUBLIC KEY");
    assert!(KeyPair::from_pkcs8(private.contents()).is_ok());
    assert!(PublicKey::from_der(public.contents()).is_ok());
}

/// The pinned pair is stable and parseable: two calls agree byte for byte,
/// and both PEMs carry the armor the consumers decode.
#[test]
fn the_pinned_pair_never_moves() {
    let first = amiss_controller_fixtures::pinned_rsa_keys();
    let second = amiss_controller_fixtures::pinned_rsa_keys();
    assert_eq!(first.private_pem, second.private_pem);
    assert_eq!(first.public_pem, second.public_pem);
    assert!(
        String::from_utf8(first.private_pem)
            .unwrap()
            .starts_with("-----BEGIN PRIVATE KEY-----")
    );
    assert!(
        String::from_utf8(first.public_pem)
            .unwrap()
            .starts_with("-----BEGIN PUBLIC KEY-----")
    );
}
