mod tests;

use std::net::{IpAddr, SocketAddr, ToSocketAddrs as _};
use std::time::Duration;

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
pub(crate) fn probe(destination: &str) -> Observation {
    let Some(url) = Url::parse(destination).ok().and_then(vetted) else {
        return Observation::Refused;
    };
    match attempt(&url, false) {
        Attempted::Answered { status, .. } if get_retries(status) => match attempt(&url, true) {
            Attempted::Answered {
                status,
                final_destination,
            } => Observation::Answered {
                method: "get",
                status,
                final_destination,
            },
            Attempted::Failed(failure) => Observation::Failed {
                method: "get",
                failure,
            },
            Attempted::Refused => Observation::Refused,
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

fn attempt(start: &Url, get: bool) -> Attempted {
    let mut url = start.clone();
    let mut standing_redirect = None;
    for _hop in 0..=MAX_HOPS {
        let address = match resolved_global(&url) {
            Ok(address) => address,
            Err(Resolution::NoRecords) => return Attempted::Failed("dns"),
            Err(Resolution::NothingGlobal) => return Attempted::Refused,
        };
        let Ok(client) = pinned(&url, address) else {
            return Attempted::Refused;
        };
        let request = if get {
            client.get(url.clone())
        } else {
            client.head(url.clone())
        };
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

/// Globally routable or not, by the closed deny table: loopback, private,
/// link-local, carrier NAT, benchmarking, documentation, multicast,
/// reserved, and their v6 counterparts including v4-mapped forms.
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
                || (octets[0] == 100 && (octets[1] & 0b1100_0000) == 64)
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                || (octets[0] == 198 && (octets[1] & 0b1111_1110) == 18)
                || octets[0] >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return global(IpAddr::V4(mapped));
            }
            let segments = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] == 0x2001 && segments[1] == 0x0db8))
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
