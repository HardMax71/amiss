use amiss_wire::uri::{http_destination_valid, site_route_valid};

#[test]
fn semantic_destinations_have_one_closed_http_grammar() {
    for valid in [
        "https://docs.example/guide/page.html",
        "http://docs.example/a%20b?q=x%20y#section%201",
        "HTTPS://docs.example/",
        "https://[::1]:8080/page",
    ] {
        assert!(http_destination_valid(valid), "valid: {valid}");
    }
    for invalid in [
        "relative/page.html",
        "ftp://docs.example/page.html",
        "https:docs.example/page.html",
        "https:///page.html",
        "https://docs.example/page#100%",
        "https://docs.example/page?q=%zz",
        "https://docs.example/résumé",
        "https://docs.example/page#%00",
    ] {
        assert!(!http_destination_valid(invalid), "invalid: {invalid}");
    }
}

#[test]
fn site_routes_are_exact_absolute_uri_paths() {
    for valid in ["/", "/guide/", "/a%20b.html", "/locale/en:v2"] {
        assert!(site_route_valid(valid), "valid: {valid}");
    }
    for invalid in [
        "guide/",
        "//other.example/guide",
        "/guide?mode=print",
        "/guide#intro",
        "/guide%",
        "/résumé",
    ] {
        assert!(!site_route_valid(invalid), "invalid: {invalid}");
    }
}

#[test]
fn component_refusals_keep_escape_precedence_and_the_accepted_prefix() {
    use amiss_wire::resolution::InvalidReference;
    use amiss_wire::uri::decode_component;

    for malformed in ["%", "%0", "%GG", "%é", "%%32"] {
        let mut bytes = b"kept:".to_vec();
        assert_eq!(
            decode_component(&format!("a%00b{malformed}tail"), &mut bytes, |byte| {
                (byte == 0).then_some(InvalidReference::DecodedPathControl)
            }),
            Err(InvalidReference::PercentEncoding),
            "{malformed}"
        );
        assert_eq!(bytes, b"kept:a\0b", "{malformed}");
    }
    let mut bytes = Vec::new();
    assert_eq!(
        decode_component("a%00b%2Fc", &mut bytes, |byte| match byte {
            0 => Some(InvalidReference::DecodedPathControl),
            b'/' => Some(InvalidReference::EncodedSlash),
            _ => None,
        }),
        Err(InvalidReference::DecodedPathControl)
    );
    assert_eq!(bytes, b"a\0b/c");
    bytes.clear();
    assert_eq!(
        decode_component("a%252Fb+é", &mut bytes, |_byte| None),
        Ok(())
    );
    assert_eq!(bytes, "a%2Fb+é".as_bytes());
}
