use std::borrow::Cow;
use std::num::NonZeroU32;
use std::sync::atomic::Ordering;
use std::time::Instant;

use amiss_wire::model::{ObjectFormat, Oid};
use gix::protocol::fetch::negotiate::{Action, Round};
use gix::protocol::fetch::{Arguments, Negotiate};
use gix::protocol::transport::client::TransportWithoutIO as _;
use gix::protocol::transport::client::blocking_io::Transport as _;
use gix::protocol::transport::client::blocking_io::http;
use gix::protocol::transport::{Protocol, Service};
use secrecy::ExposeSecret as _;

use super::pack;
use super::{ExactFetch, ExactWant, GitCredential, GitFetchBounds, GitFetchError, http_options};

const USER_AGENT: &str = "amiss-controller";
type HttpTransport = http::Transport<http::reqwest::Remote>;
type Wanted = (gix::ObjectId, String);

pub(super) fn fetch_exact(fetch: ExactFetch<'_>) -> Result<(), GitFetchError> {
    let started = Instant::now();
    active(&fetch, started)?;
    let parsed = exact_https_url(fetch.url)?;
    let wanted = exact_wants(fetch.wants)?;
    let repository = initialize(fetch.destination)?;
    let mut transport = http::Transport::new_http(
        http::reqwest::Remote::default(),
        parsed,
        Protocol::V2,
        false,
    );
    transport
        .configure(&http_options(fetch.bounds, started))
        .map_err(fetch_error)?;
    let mut handshake = v2_handshake(&mut transport, fetch.credential)?;
    let installed = receive_pack(
        &repository,
        &wanted,
        &mut transport,
        &mut handshake,
        fetch.bounds,
        fetch.cancelled,
        started,
    )?;
    gix::protocol::indicate_end_of_interaction(&mut transport, false).map_err(fetch_error)?;
    create_refs(&repository, &wanted)?;
    if let Some(keep_path) = installed.keep_path {
        std::fs::remove_file(keep_path).map_err(fetch_error)?;
    }
    Ok(())
}

fn active(fetch: &ExactFetch<'_>, started: Instant) -> Result<(), GitFetchError> {
    (!fetch.cancelled.load(Ordering::Acquire) && started.elapsed() < fetch.bounds.request)
        .then_some(())
        .ok_or(GitFetchError("the exact Git fetch was interrupted"))
}

fn exact_https_url(url: &str) -> Result<gix::Url, GitFetchError> {
    let parsed = gix::url::parse(url.as_bytes().into()).map_err(fetch_error)?;
    let valid = url.starts_with("https://")
        && !url.contains(['?', '#'])
        && parsed.scheme == gix::url::Scheme::Https
        && parsed.user.is_none()
        && parsed.password.is_none()
        && parsed.host.as_ref().is_some_and(|host| !host.is_empty())
        && parsed.port.is_none()
        && !parsed.serialize_alternative_form
        && parsed.path.starts_with(b"/")
        && parsed.path.len() > 1;
    valid
        .then_some(parsed)
        .ok_or(GitFetchError("the exact Git URL is not strict HTTPS"))
}

fn initialize(destination: &std::path::Path) -> Result<gix::Repository, GitFetchError> {
    gix::ThreadSafeRepository::init_opts(
        destination,
        gix::create::Kind::WithWorktree,
        gix::create::Options {
            destination_must_be_empty: Some(true),
            object_hash: Some(gix::hash::Kind::Sha1),
            ..Default::default()
        },
        gix::open::Options::isolated().strict_config(true),
    )
    .map(|repository| repository.to_thread_local())
    .map_err(fetch_error)
}

fn exact_wants(wants: &[ExactWant<'_>]) -> Result<Vec<Wanted>, GitFetchError> {
    if wants.is_empty() {
        return Err(GitFetchError("the exact Git fetch has no wanted objects"));
    }
    wants
        .iter()
        .map(|want| {
            let exact_sha1 = Oid::new(ObjectFormat::Sha1, want.oid.as_str().to_owned()).as_ref()
                == Some(want.oid);
            if !exact_sha1 || !private_ref(want.reference) {
                return Err(GitFetchError("an exact Git want is invalid"));
            }
            gix::ObjectId::from_hex(want.oid.as_str().as_bytes())
                .map(|oid| (oid, want.reference.to_owned()))
                .map_err(fetch_error)
        })
        .collect()
}

fn private_ref(reference: &str) -> bool {
    reference.strip_prefix("refs/amiss/").is_some_and(|name| {
        !name.is_empty()
            && !name.starts_with('/')
            && !name.ends_with('/')
            && !name.contains("//")
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_'))
    })
}

fn v2_handshake(
    transport: &mut HttpTransport,
    credential: Option<GitCredential<'_>>,
) -> Result<gix::protocol::Handshake, GitFetchError> {
    if let Some(credential) = credential {
        if !credential_username(credential.username) {
            return Err(GitFetchError("the exact Git credential is invalid"));
        }
        transport
            .set_identity(gix::sec::identity::Account {
                username: credential.username.to_owned(),
                password: credential.password.expose_secret().to_owned(),
                oauth_refresh_token: None,
            })
            .map_err(fetch_error)?;
    }
    let response = transport
        .handshake(Service::UploadPack, &[])
        .map_err(fetch_error)?;
    if response.actual_protocol != Protocol::V2 || response.refs.is_some() {
        return Err(GitFetchError("the Git server did not use protocol v2"));
    }
    Ok(gix::protocol::Handshake {
        server_protocol_version: response.actual_protocol,
        refs: None,
        v1_shallow_updates: None,
        capabilities: response.capabilities,
    })
}

fn credential_username(username: &str) -> bool {
    !username.is_empty()
        && username.len() <= 256
        && username
            .chars()
            .all(|character| character != ':' && !character.is_control())
}

fn receive_pack(
    repository: &gix::Repository,
    wanted: &[Wanted],
    transport: &mut HttpTransport,
    handshake: &mut gix::protocol::Handshake,
    bounds: GitFetchBounds,
    cancelled: &std::sync::atomic::AtomicBool,
    started: Instant,
) -> Result<pack::InstalledPack, GitFetchError> {
    let mut negotiate = ExactWants {
        wants: wanted.iter().map(|(oid, _reference)| *oid).collect(),
    };
    let shallow = gix::protocol::fetch::Shallow::DepthAtRemote(
        NonZeroU32::new(1).ok_or(GitFetchError("the shallow depth is invalid"))?,
    );
    let pack_directory = repository.git_dir().join("objects").join("pack");
    let mut installed = None;
    let outcome = gix::protocol::fetch(
        &mut negotiate,
        |reader, progress, interrupt| -> Result<bool, pack::PackError> {
            installed = Some(pack::validate_and_install(
                reader,
                &pack_directory,
                progress,
                interrupt,
                started,
                bounds.request,
            )?);
            Ok(true)
        },
        gix::progress::Discard,
        cancelled,
        gix::protocol::fetch::Context {
            handshake,
            transport,
            user_agent: ("agent", Some(Cow::Owned(gix::protocol::agent(USER_AGENT)))),
            trace_packetlines: false,
        },
        gix::protocol::fetch::Options {
            shallow_file: repository.shallow_file(),
            shallow: &shallow,
            tags: gix::protocol::fetch::Tags::None,
            reject_shallow_remote: true,
        },
    )
    .map_err(fetch_error)?;
    outcome
        .and(installed)
        .ok_or(GitFetchError("the server did not return the exact objects"))
}

fn create_refs(repository: &gix::Repository, wanted: &[Wanted]) -> Result<(), GitFetchError> {
    wanted.iter().try_for_each(|(oid, reference)| {
        repository
            .has_object(oid)
            .then_some(())
            .ok_or(GitFetchError("the server omitted an exact wanted object"))?;
        repository
            .reference(
                reference.as_str(),
                *oid,
                gix::refs::transaction::PreviousValue::MustNotExist,
                "amiss authenticated acquisition",
            )
            .map(|_reference| ())
            .map_err(fetch_error)
    })
}

struct ExactWants {
    wants: Vec<gix::ObjectId>,
}

impl Negotiate for ExactWants {
    fn mark_complete_and_common_ref(
        &mut self,
    ) -> Result<Action, gix::protocol::fetch::negotiate::Error> {
        Ok(Action::MustNegotiate {
            remote_ref_target_known: vec![false; self.wants.len()],
        })
    }

    fn add_wants(&mut self, arguments: &mut Arguments, _remote_ref_target_known: &[bool]) -> bool {
        self.wants.iter().for_each(|oid| arguments.want(oid));
        !self.wants.is_empty()
    }

    fn one_round(
        &mut self,
        _state: &mut gix::protocol::fetch::negotiate::one_round::State,
        _arguments: &mut Arguments,
        _previous_response: Option<&gix::protocol::fetch::Response>,
    ) -> Result<(Round, bool), gix::protocol::fetch::negotiate::Error> {
        Ok((
            Round {
                haves_sent: 0,
                in_vain: 0,
                haves_to_send: 0,
                previous_response_had_at_least_one_in_common: false,
            },
            true,
        ))
    }
}

fn fetch_error<E>(_defect: E) -> GitFetchError {
    GitFetchError("the exact Git fetch failed")
}

#[path = "../tests/internal/protocol.rs"]
mod tests;
