#![cfg(test)]

use std::net::IpAddr;

use url::Url;

use super::{get_retries, global, redirect_target, shown, vetted};

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

#[test]
fn the_deny_table_refuses_every_unroutable_family() {
    for private in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.1.1",
        "0.0.0.0",
        "255.255.255.255",
        "224.0.0.1",
        "100.64.0.1",
        "100.127.255.254",
        "192.0.0.1",
        "192.88.99.2",
        "192.0.2.1",
        "198.18.0.1",
        "198.19.255.254",
        "240.0.0.1",
        "::1",
        "::",
        "fc00::1",
        "fd12::1",
        "fe80::1",
        "ff02::1",
        "2001:db8::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "::ffff:1.1.1.1",
        "::1.1.1.1",
        "0.1.2.3",
        "::10.0.0.1",
        "100::1",
        "100:0:0:1::1",
        "2001:10::1",
        "2001:2::1",
        "64:ff9b::10.0.0.1",
        "64:ff9b:1::10.0.0.1",
        "64:ff9b:1::1.1.1.1",
        "64:ff9b:2::1",
        "2002:a00:1::1",
        "2002:101:101::1",
        "3fff::1",
        "3fff:fff::1",
        "5f00::1",
        "2001::1",
        "2001:0:4136:e378:8000:63bf:3fff:fdd2",
    ] {
        assert!(!global(ip(private)), "{private} must be refused");
    }
    for public in [
        "140.82.121.4",
        "1.1.1.1",
        "100.128.0.1",
        "192.0.0.9",
        "192.0.0.10",
        "2606:4700::1111",
        "64:ff9b::1.1.1.1",
        "100:0:0:2::1",
        "2001:1::1",
        "2001:1::2",
        "2001:1::3",
        "2001:3::1",
        "2001:4:112::1",
        "2001:20::1",
        "2001:30::1",
        "2001:200::1",
        "3fff:1000::1",
        "5f01::1",
        "2001:4860:4860::8888",
    ] {
        assert!(global(ip(public)), "{public} must be routable");
    }
}

#[test]
fn only_named_https_hosts_without_credentials_are_vetted() {
    for allowed in ["https://example.com/a", "https://sub.host.example/x?q=1#f"] {
        assert!(vetted(Url::parse(allowed).unwrap()).is_some(), "{allowed}");
    }
    for refused in [
        "http://example.com/a",
        "https://127.0.0.1/a",
        "https://[::1]/a",
        "https://user@example.com/a",
        "https://user:pw@example.com/a",
        "ftp://example.com/a",
    ] {
        assert!(vetted(Url::parse(refused).unwrap()).is_none(), "{refused}");
    }
}

#[test]
fn the_get_retry_covers_absence_and_head_refusals() {
    for status in [404, 405, 410, 501] {
        assert!(get_retries(status));
    }
    for status in [200, 301, 403, 429, 500] {
        assert!(!get_retries(status));
    }
}

#[test]
fn redirect_targets_join_relative_locations() {
    let current = Url::parse("https://example.com/docs/page").unwrap();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::LOCATION, "/moved".parse().unwrap());
    assert_eq!(
        redirect_target(&current, &headers).unwrap().as_str(),
        "https://example.com/moved"
    );
    headers.insert(
        reqwest::header::LOCATION,
        "https://other.example/x".parse().unwrap(),
    );
    assert_eq!(
        redirect_target(&current, &headers).unwrap().as_str(),
        "https://other.example/x"
    );
    assert!(redirect_target(&current, &reqwest::header::HeaderMap::new()).is_none());
}

#[test]
fn refused_destinations_are_named_without_their_credentials() {
    assert_eq!(
        shown("https://user:secret@example.com/a"),
        "https://example.com/a"
    );
    assert_eq!(shown("https://example.com/a"), "https://example.com/a");
    assert_eq!(shown("not a url"), "an unparsable destination");
}
