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
