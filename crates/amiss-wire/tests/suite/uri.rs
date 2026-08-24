use amiss_wire::uri::http_destination_valid;

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
