mod tests;

use std::net::{IpAddr, SocketAddr, ToSocketAddrs as _};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use url::Url;

const CONNECT: Duration = Duration::from_secs(5);
const OPERATION: Duration = Duration::from_secs(10);
const MAX_HOPS: usize = 5;

pub(crate) enum Observation {
    Answered {
        method: &'static str,
        status: i64,
        final_destination: Option<String>,
    },
    Failed {
        method: &'static str,
        failure: &'static str,
    },
    Refused,
}

enum Attempted {
    Answered {
        status: i64,
        final_destination: Option<String>,
    },
    Failed(&'static str),
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
        Attempted::Answered {
            status,
            final_destination,
        } if get_retries(status) => match attempt(&url, true, deadline) {
            Attempted::Answered {
                status,
                final_destination,
            } => Observation::Answered {
                method: "get",
                status,
                final_destination,
            },
            // A failed retry does not erase the HEAD answer; the judge
            // treats an unconfirmed absence as unproven anyway.
            Attempted::Failed(_) | Attempted::Refused => Observation::Answered {
                method: "head",
                status,
                final_destination,
            },
        },
        Attempted::Answered {
            status,
            final_destination,
        } => Observation::Answered {
            method: "head",
            status,
            final_destination,
        },
        Attempted::Failed(failure) => Observation::Failed {
            method: "head",
            failure,
        },
        Attempted::Refused => Observation::Refused,
    }
}

/// The statuses whose HEAD answer is not evidence: absence needs the GET
/// confirmation, and 405 or 501 mean HEAD itself was the problem.
const fn get_retries(status: i64) -> bool {
    matches!(status, 404 | 405 | 410 | 501)
}

fn attempt(start: &Url, get: bool, deadline: Instant) -> Attempted {
    let mut url = start.clone();
    let mut standing_redirect = None;
    for _hop in 0..=MAX_HOPS {
        // The ceiling binds between requests; one resolver lookup or one
        // in-flight request can overhang it, nothing more.
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return match standing_redirect {
                Some(status) => Attempted::Answered {
                    status,
                    final_destination: Some(url.to_string()),
                },
                None => Attempted::Failed("timeout"),
            };
        }
        // A hop whose name resolves nowhere usable keeps the redirect that
        // pointed at it: the redirect is the observation, the hop is not.
        let address = match (resolved_global(&url), standing_redirect) {
            (Ok(address), _) => address,
            (Err(_defect), Some(status)) => {
                return Attempted::Answered {
                    status,
                    final_destination: Some(url.to_string()),
                };
            }
            (Err(Resolution::NoRecords), None) => return Attempted::Failed("dns"),
            (Err(Resolution::NothingGlobal), None) => return Attempted::Refused,
        };
        let Ok(client) = pinned(&url, address) else {
            return Attempted::Refused;
        };
        let request = if get {
            client.get(url.clone())
        } else {
            client.head(url.clone())
        }
        .timeout(OPERATION.min(left));
        let response = match request.send() {
            Ok(response) => response,
            Err(error) if error.is_timeout() => return Attempted::Failed("timeout"),
            // Connect and protocol failures are indistinguishable from a
            // refused socket without sniffing error text; the judge treats
            // every transport failure alike, so one honest word suffices.
            Err(_error) => return Attempted::Failed("refused"),
        };
        let status = i64::from(response.status().as_u16());
        if !response.status().is_redirection() {
            return Attempted::Answered {
                status,
                final_destination: moved(start, &url),
            };
        }
        let Some(next) = redirect_target(&url, response.headers()) else {
            return Attempted::Answered {
                status,
                final_destination: moved(start, &url),
            };
        };
        // A hop the policy refuses is still worth recording: the standing
        // redirect and where it pointed, never a request to it.
        match vetted(next.clone()) {
            Some(vetted) => {
                standing_redirect = Some(status);
                url = vetted;
            }
            None => {
                return Attempted::Answered {
                    status,
                    final_destination: Some(next.to_string()),
                };
            }
        }
    }
    // Hops exhausted: the last redirect the server actually sent is the
    // record, never an invented status.
    match standing_redirect {
        Some(status) => Attempted::Answered {
            status,
            final_destination: moved(start, &url),
        },
        None => Attempted::Failed("refused"),
    }
}

fn moved(start: &Url, current: &Url) -> Option<String> {
    (current != start).then(|| current.to_string())
}

fn redirect_target(current: &Url, headers: &reqwest::header::HeaderMap) -> Option<Url> {
    let location = headers.get(reqwest::header::LOCATION)?.to_str().ok()?;
    current.join(location).ok()
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

/// Globally routable or not. Every v6 form that embeds or routes toward a
/// v4 address, mapped, compatible, NAT64, and 6to4, defers to that v4's
/// answer, so the two tables cannot drift apart.
fn global(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || octets[0] == 0
                || (octets[0] == 100 && (octets[1] & 0b1100_0000) == 64)
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                || (octets[0] == 198 && (octets[1] & 0b1111_1110) == 18)
                || octets[0] >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(embedded) = v6.to_ipv4() {
                return global(IpAddr::V4(embedded));
            }
            let segments = v6.segments();
            if segments[0] == 0x0064 && segments[1] == 0xff9b && segments[2..6] == [0, 0, 0, 0] {
                let embedded = (u32::from(segments[6]) << 16) | u32::from(segments[7]);
                return global(IpAddr::V4(std::net::Ipv4Addr::from(embedded)));
            }
            if segments[0] == 0x2002 {
                let embedded = (u32::from(segments[1]) << 16) | u32::from(segments[2]);
                return global(IpAddr::V4(std::net::Ipv4Addr::from(embedded)));
            }
            !(v6.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] == 0x2001 && segments[1] == 0x0000)
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)
                || (segments[0] == 0x2001 && segments[1] == 0x0002 && segments[2] == 0)
                || (segments[0] == 0x2001 && (segments[1] & 0xfff0) == 0x0010)
                || (segments[0] == 0x0100 && segments[1..4] == [0, 0, 0]))
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
