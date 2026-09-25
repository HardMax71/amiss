mod tests;

use std::io::Read as _;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs as _};
use std::time::{Duration, Instant};

use reqwest::blocking::{Client, Response};
use reqwest::redirect::Policy;
use url::Url;

use amiss_wire::external::{ProbeFailure, ProbeMethod};

const CONNECT: Duration = Duration::from_secs(5);
const OPERATION: Duration = Duration::from_secs(10);
const MAX_HOPS: usize = 5;
/// How much of an HTML page is read for an instant refresh, which a
/// redirect stub states in its head.
const REFRESH_BYTES: u64 = 16 * 1024;

pub(crate) enum Observation {
    Answered {
        method: ProbeMethod,
        status: u16,
        redirect: Option<Redirect>,
    },
    Failed {
        method: ProbeMethod,
        failure: ProbeFailure,
    },
    Refused,
}

pub(crate) struct Redirect {
    pub destination: String,
    pub permanent: bool,
}

enum Attempted {
    Answered {
        status: u16,
        redirect: Option<Redirect>,
    },
    Failed(ProbeFailure),
    Refused,
}

/// One destination probed: HEAD first, and a GET retry for the statuses
/// servers answer differently by method, since a 404 refutes only under
/// GET. Every URL and every redirect hop is vetted and address-pinned
/// before a byte leaves the process.
pub(crate) fn probe(destination: &str, deadline: Instant) -> Observation {
    let Some(url) = Url::parse(destination).ok().and_then(vetted) else {
        return Observation::Refused;
    };
    match attempt(&url, false, deadline) {
        Attempted::Answered { status, redirect } if get_retries(status) => {
            match attempt(&url, true, deadline) {
                Attempted::Answered { status, redirect } => Observation::Answered {
                    method: ProbeMethod::Get,
                    status,
                    redirect,
                },
                // A failed retry does not erase the HEAD answer; the judge
                // treats an unconfirmed absence as unproven anyway.
                Attempted::Failed(_) | Attempted::Refused => Observation::Answered {
                    method: ProbeMethod::Head,
                    status,
                    redirect,
                },
            }
        }
        Attempted::Answered { status, redirect } => Observation::Answered {
            method: ProbeMethod::Head,
            status,
            redirect,
        },
        Attempted::Failed(failure) => Observation::Failed {
            method: ProbeMethod::Head,
            failure,
        },
        Attempted::Refused => Observation::Refused,
    }
}

/// The statuses whose HEAD answer is not evidence: absence needs the GET
/// confirmation, and 405 or 501 mean HEAD itself was the problem.
const fn get_retries(status: u16) -> bool {
    matches!(status, 404 | 405 | 410 | 501)
}

fn attempt(start: &Url, get: bool, deadline: Instant) -> Attempted {
    let mut url = start.clone();
    let mut standing_redirect: Option<(u16, bool)> = None;
    for _hop in 0..=MAX_HOPS {
        // The ceiling binds before the resolver and again before the send,
        // so only one lookup or one in-flight request can overhang it.
        if remaining(deadline).is_none() {
            return spent(standing_redirect, &url);
        }
        // A hop whose name resolves nowhere usable keeps the redirect that
        // pointed at it: the redirect is the observation, the hop is not.
        let address = match (resolved_global(&url), standing_redirect) {
            (Ok(address), _) => address,
            (Err(_defect), Some((status, permanent))) => {
                return Attempted::Answered {
                    status,
                    redirect: Some(Redirect {
                        destination: url.to_string(),
                        permanent,
                    }),
                };
            }
            (Err(Resolution::NoRecords), None) => return Attempted::Failed(ProbeFailure::Dns),
            (Err(Resolution::NothingGlobal), None) => return Attempted::Refused,
        };
        let Ok(client) = pinned(&url, address) else {
            return Attempted::Refused;
        };
        let Some(left) = remaining(deadline) else {
            return spent(standing_redirect, &url);
        };
        let request = if get {
            client.get(url.clone())
        } else {
            client.head(url.clone())
        }
        .timeout(OPERATION.min(left));
        let response = match request.send() {
            Ok(response) => response,
            Err(error) if error.is_timeout() => {
                return Attempted::Failed(ProbeFailure::Timeout);
            }
            // Connect and protocol failures are indistinguishable from a
            // refused socket without sniffing error text; the judge treats
            // every transport failure alike, so one honest word suffices.
            Err(_error) => return Attempted::Failed(ProbeFailure::Refused),
        };
        let status = response.status().as_u16();
        if !response.status().is_redirection() {
            let refreshed = response
                .status()
                .is_success()
                .then(|| page_head(&client, &url, response, get, deadline))
                .flatten()
                .and_then(|head| refresh_target(&url, &head))
                .and_then(vetted)
                .filter(|next| *next != url);
            if let Some(next) = refreshed {
                standing_redirect = Some((
                    status,
                    standing_redirect.is_none_or(|(_status, permanent)| permanent),
                ));
                url = next;
                continue;
            }
            return Attempted::Answered {
                status,
                redirect: moved(
                    start,
                    &url,
                    standing_redirect.is_some_and(|(_status, permanent)| permanent),
                ),
            };
        }
        let observed_redirect = advance_redirect(standing_redirect, status);
        let Some(next) = redirect_target(&url, response.headers()) else {
            return Attempted::Answered {
                status,
                redirect: moved(start, &url, observed_redirect.1),
            };
        };
        // A hop the policy refuses is still worth recording: the standing
        // redirect and where it pointed, never a request to it.
        match vetted(next.clone()) {
            Some(vetted) => {
                standing_redirect = Some(observed_redirect);
                url = vetted;
            }
            None => {
                return Attempted::Answered {
                    status,
                    redirect: Some(Redirect {
                        destination: next.to_string(),
                        permanent: observed_redirect.1,
                    }),
                };
            }
        }
    }
    // Hops exhausted: the last redirect the server actually sent is the
    // record, never an invented status.
    match standing_redirect {
        Some((status, permanent)) => Attempted::Answered {
            status,
            redirect: moved(start, &url, permanent),
        },
        None => Attempted::Failed(ProbeFailure::Refused),
    }
}

fn remaining(deadline: Instant) -> Option<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    (!left.is_zero()).then_some(left)
}

/// The budget ran out mid-walk: the standing redirect is still the record
/// when one was observed, and plain exhaustion otherwise.
fn spent(standing_redirect: Option<(u16, bool)>, url: &Url) -> Attempted {
    match standing_redirect {
        Some((status, permanent)) => Attempted::Answered {
            status,
            redirect: Some(Redirect {
                destination: url.to_string(),
                permanent,
            }),
        },
        None => Attempted::Failed(ProbeFailure::Timeout),
    }
}

fn moved(start: &Url, current: &Url, permanent: bool) -> Option<Redirect> {
    (current != start).then(|| Redirect {
        destination: current.to_string(),
        permanent,
    })
}

fn advance_redirect(standing: Option<(u16, bool)>, status: u16) -> (u16, bool) {
    let permanent = matches!(status, 301 | 308)
        && standing.is_none_or(|(_previous_status, permanent)| permanent);
    (status, permanent)
}

/// The opening bytes of an HTML page that answered: the body already in
/// hand under GET, or one more GET after HEAD, since HEAD carries none.
fn page_head(
    client: &Client,
    url: &Url,
    response: Response,
    get: bool,
    deadline: Instant,
) -> Option<Vec<u8>> {
    let html = |response: &Response| {
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.trim_start().starts_with("text/html"))
    };
    if !html(&response) {
        return None;
    }
    let response = if get {
        response
    } else {
        let left = remaining(deadline)?;
        let fetched = client
            .get(url.clone())
            .timeout(OPERATION.min(left))
            .send()
            .ok()?;
        (fetched.status().is_success() && html(&fetched)).then_some(fetched)?
    };
    let mut head = Vec::new();
    response.take(REFRESH_BYTES).read_to_end(&mut head).ok()?;
    Some(head)
}

/// Where an instant refresh sends a browser: the first `meta` element whose
/// `http-equiv` is `refresh` and whose `content` is a zero delay and a URL,
/// read against the page's own URL. A later delay is a page a reader sees
/// first, so it moves nothing.
fn refresh_target(current: &Url, head: &[u8]) -> Option<Url> {
    let text = String::from_utf8_lossy(head);
    let folded = text.to_ascii_lowercase();
    folded.match_indices("<meta").find_map(|(start, _)| {
        let end = folded.get(start..)?.find('>')?.checked_add(start)?;
        let tag = text.get(start..end)?;
        let refresh = attribute(tag, "http-equiv")?.eq_ignore_ascii_case("refresh");
        let (delay, target) = attribute(tag, "content")?.split_once([';', ','])?;
        let instant = !delay.trim().is_empty()
            && delay.trim().bytes().all(|byte| matches!(byte, b'0' | b'.'));
        let target = target.trim_start();
        let target = match target.get(..3) {
            Some(key) if key.eq_ignore_ascii_case("url") => {
                target.get(3..)?.trim_start().strip_prefix('=')?.trim()
            }
            Some(_) | None => target.trim(),
        };
        let target = target.trim_matches(['"', '\'']);
        (refresh && instant && !target.is_empty())
            .then(|| current.join(target).ok())
            .flatten()
    })
}

/// One attribute's value inside a tag, found by its name in any case where a
/// space opens it: quoted, or up to the next space.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let folded = tag.to_ascii_lowercase();
    folded.match_indices(name).find_map(|(at, _)| {
        let opened = at
            .checked_sub(1)
            .and_then(|before| folded.as_bytes().get(before))
            .is_some_and(u8::is_ascii_whitespace);
        let rest = tag.get(at.checked_add(name.len())?..)?.trim_start();
        let value = rest.strip_prefix('=')?.trim_start();
        let quoted = value
            .chars()
            .next()
            .filter(|first| matches!(first, '"' | '\''));
        let value = match quoted {
            Some(quote) => value.get(1..)?.split(quote).next()?,
            None => value
                .split(|ch: char| ch.is_ascii_whitespace() || ch == '/')
                .next()?,
        };
        opened.then_some(value)
    })
}

fn redirect_target(current: &Url, headers: &reqwest::header::HeaderMap) -> Option<Url> {
    let location = headers.get(reqwest::header::LOCATION)?.to_str().ok()?;
    current.join(location).ok()
}

/// A destination safe to print: credentials stripped when the URL parses,
/// and a fixed placeholder when it does not, so nothing secret reaches a log.
pub(crate) fn shown(destination: &str) -> String {
    match Url::parse(destination) {
        Ok(mut url) => {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.to_string()
        }
        Err(_defect) => "an unparsable destination".to_owned(),
    }
}

/// The URL shapes a probe may even consider: https, no credentials, and a
/// named host, since IP literals skip the naming layer vetting relies on.
fn vetted(url: Url) -> Option<Url> {
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(url.host(), Some(url::Host::Domain(_))))
    .then_some(url)
}

enum Resolution {
    NoRecords,
    NothingGlobal,
}

/// Resolves the host and picks a globally routable address to pin, so the
/// connection can never reach what the name resolution tried to smuggle.
fn resolved_global(url: &Url) -> Result<SocketAddr, Resolution> {
    let host = url.host_str().ok_or(Resolution::NoRecords)?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_defect| Resolution::NoRecords)?;
    let mut any = false;
    for address in addresses {
        any = true;
        if global(address.ip()) {
            return Ok(address);
        }
    }
    Err(if any {
        Resolution::NothingGlobal
    } else {
        Resolution::NoRecords
    })
}

#[derive(Clone, Copy)]
struct Ipv4Prefix {
    network: std::net::Ipv4Addr,
    bits: u32,
}

impl Ipv4Prefix {
    fn contains(self, address: std::net::Ipv4Addr) -> bool {
        32_u32.checked_sub(self.bits).is_some_and(|host_bits| {
            u32::from(address).checked_shr(host_bits)
                == u32::from(self.network).checked_shr(host_bits)
        })
    }
}

#[derive(Clone, Copy)]
struct Ipv6Prefix {
    network: std::net::Ipv6Addr,
    bits: u32,
}

impl Ipv6Prefix {
    fn contains(self, address: std::net::Ipv6Addr) -> bool {
        128_u32.checked_sub(self.bits).is_some_and(|host_bits| {
            u128::from(address).checked_shr(host_bits)
                == u128::from(self.network).checked_shr(host_bits)
        })
    }
}

const fn v4(a: u8, b: u8, c: u8, d: u8, bits: u32) -> Ipv4Prefix {
    Ipv4Prefix {
        network: std::net::Ipv4Addr::new(a, b, c, d),
        bits,
    }
}

const fn v6(network: u128, bits: u32) -> Ipv6Prefix {
    Ipv6Prefix {
        network: std::net::Ipv6Addr::from_bits(network),
        bits,
    }
}

// IANA special-purpose registries, last updated 2025-10-09, plus multicast refusal.
const NON_GLOBAL_V4: &[Ipv4Prefix] = &[
    v4(0, 0, 0, 0, 8),
    v4(10, 0, 0, 0, 8),
    v4(100, 64, 0, 0, 10),
    v4(127, 0, 0, 0, 8),
    v4(169, 254, 0, 0, 16),
    v4(172, 16, 0, 0, 12),
    v4(192, 0, 0, 0, 24),
    v4(192, 0, 2, 0, 24),
    v4(192, 88, 99, 0, 24),
    v4(192, 168, 0, 0, 16),
    v4(198, 18, 0, 0, 15),
    v4(198, 51, 100, 0, 24),
    v4(203, 0, 113, 0, 24),
    v4(224, 0, 0, 0, 4),
    v4(240, 0, 0, 0, 4),
];

const GLOBAL_IETF_V6: &[Ipv6Prefix] = &[
    v6(0x2001_0001_0000_0000_0000_0000_0000_0001, 128),
    v6(0x2001_0001_0000_0000_0000_0000_0000_0002, 128),
    v6(0x2001_0001_0000_0000_0000_0000_0000_0003, 128),
    v6(0x2001_0003_0000_0000_0000_0000_0000_0000, 32),
    v6(0x2001_0004_0112_0000_0000_0000_0000_0000, 48),
    v6(0x2001_0020_0000_0000_0000_0000_0000_0000, 28),
    v6(0x2001_0030_0000_0000_0000_0000_0000_0000, 28),
];

const NON_GLOBAL_V6: &[Ipv6Prefix] = &[
    v6(0x0000_0000_0000_0000_0000_0000_0000_0000, 96),
    v6(0x0000_0000_0000_0000_0000_ffff_0000_0000, 96),
    v6(0x0064_ff9b_0000_0000_0000_0000_0000_0000, 32),
    v6(0x0100_0000_0000_0000_0000_0000_0000_0000, 64),
    v6(0x0100_0000_0000_0001_0000_0000_0000_0000, 64),
    v6(0x2001_0000_0000_0000_0000_0000_0000_0000, 23),
    v6(0x2001_0db8_0000_0000_0000_0000_0000_0000, 32),
    v6(0x2002_0000_0000_0000_0000_0000_0000_0000, 16),
    v6(0x3fff_0000_0000_0000_0000_0000_0000_0000, 20),
    v6(0x5f00_0000_0000_0000_0000_0000_0000_0000, 16),
    v6(0xfc00_0000_0000_0000_0000_0000_0000_0000, 7),
    v6(0xfe80_0000_0000_0000_0000_0000_0000_0000, 10),
    v6(0xff00_0000_0000_0000_0000_0000_0000_0000, 8),
];

fn global(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            matches!(v4.octets(), [192, 0, 0, 9 | 10])
                || !NON_GLOBAL_V4.iter().any(|prefix| prefix.contains(v4))
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            if matches!(segments, [0x0064, 0xff9b, 0, 0, 0, 0, _, _]) {
                let embedded = (u32::from(segments[6]) << 16) | u32::from(segments[7]);
                return global(IpAddr::V4(std::net::Ipv4Addr::from(embedded)));
            }
            if GLOBAL_IETF_V6.iter().any(|prefix| prefix.contains(v6)) {
                return true;
            }
            if NON_GLOBAL_V6.iter().any(|prefix| prefix.contains(v6)) {
                return false;
            }
            if let Some(embedded) = v6.to_ipv4() {
                return global(IpAddr::V4(embedded));
            }
            true
        }
    }
}

/// One client per request, pinned to the vetted address for exactly this
/// host, proxies and redirects and cookies all off.
fn pinned(url: &Url, address: SocketAddr) -> Result<Client, reqwest::Error> {
    let host = url.host_str().unwrap_or_default();
    Client::builder()
        .resolve(host, address)
        .no_proxy()
        .https_only(true)
        .redirect(Policy::none())
        .connect_timeout(CONNECT)
        .timeout(OPERATION)
        .user_agent(concat!("amiss-probe/", env!("CARGO_PKG_VERSION")))
        .build()
}
