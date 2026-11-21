//! The gateway (#446; docs/runner-api.md, "LAN gateway (#284)"), ported from Runner/Gateway.swift,
//! Runner/Pairing.swift, Runner/Routes.swift and the gateway tokens of Runner/Devices.swift: the
//! one way a paired phone reaches obid. It listens on its own address (`gateway` in the
//! configuration), redeems pairing invitations for device tokens, and forwards the routes a phone
//! uses (and only those) to obid's own API with the API token, which never leaves obid. Answers
//! come back through the API's own layers (the vault's secrets are already out), with an agent's
//! session id and the accounts' homes taken out too.
//!
//! Invitations are made, and paired devices listed and revoked, on the loopback API with the API
//! token (`/v1/pairing/invite`, `/v1/pairing/devices`): a phone that reaches the gateway can
//! redeem an invitation, never mint one. Only an invitation the owner made on their own device
//! (its `approval`, signed by a registered key on the owner's click) lets the phone it pairs add
//! its own publishing key, in the minutes after it redeemed it (crate::devices): the API token
//! alone pairs a phone that can't approve anything.
//!
//! A phone paired through obid's Obysphere (crate::relay) reaches the same router in process, its
//! requests sealed end to end: such a device's token works only inside its sealed envelopes,
//! never on the listener, and only through its own relay token's socket.
//!
//! ponytail: the gateway binds 127.0.0.1 and `tailscale serve` puts it on the owner's tailnet.
//! The Swift gateway's LAN listener and Bonjour come back when pairing on the home network needs them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use axum::Router;
use axum::body::{Body, Bytes, HttpBody as _};
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use base64::Engine as _;
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower::ServiceExt as _;

use crate::api::Shared;

/// How long an invitation can be redeemed (Pairing.swift: 120 seconds).
const INVITATION: Duration = Duration::from_secs(120);
/// How long another computer's invitation can be redeemed: the person walks between machines
/// (docs/design/computer-picker, decision 4; docs/contracts/remote.md, COMPUTERS-13).
const COMPUTER_INVITATION: Duration = Duration::from_secs(600);
/// Wrong credentials an invitation takes before it is gone.
const ATTEMPTS: u32 = 5;
/// How long a device token works (Devices.swift, `GatewayToken.lifetime`).
const LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// A device token used with less than this left gets a fresh [`LIFETIME`] (PAIRING-6): a phone in
/// use stays paired, one unused for 30 days still expires. Half the lifetime, so the file is written
/// at most about twice a month per device, never on every request.
const SLIDE: Duration = Duration::from_secs(15 * 24 * 60 * 60);
/// The longest a device not paired through the Obysphere stays paired, counted from pairing,
/// however it slides (PAIRING-7): then it pairs again.
const PAIRED: Duration = Duration::from_secs(90 * 24 * 60 * 60);
/// How long after redeeming an invitation the owner made a phone may add its publishing key: it
/// registers on the connection right after pairing.
const KEYABLE: Duration = Duration::from_secs(600);
/// The largest request a phone may send through.
const BODY: usize = 4 << 20;
/// What stands for an account's home in an answer.
const HOME: &str = "[home]";

/// The routes a phone may use through the gateway (Runner/Routes.swift), apart from the API's own
/// router: adding an endpoint never makes it reachable from a phone by itself. Kept off on purpose:
/// `/v1/checks/…` and `/v1/agent/…` (the Mac's and agents' own calls), `/v1/connections…`,
/// `/v1/pairing/…`, `/v1/obid/…` but the reads of the Sphere below, adding,
/// removing and signing in accounts, and everything else.
/// `*` is one path segment; `**`, last only, is one or more.
const ROUTES: &[(&str, &str)] = &[
    ("GET", "/v1/status"),
    // The iPhone's side panel lists the Obysphere and teammates' Obys (docs/contracts/ios-new-oby.md,
    // IOSNEW-4): who's in it and the membership's public facts, read only. Joining, leaving, the
    // keys and the pins stay on the machine.
    ("GET", "/v1/obid/sphere"),
    ("GET", "/v1/obid/sphere/members"),
    // A member's thread (docs/contracts/peer-thread.md): read only.
    ("GET", "/v1/obid/sphere/peers/*"),
    ("GET", "/v1/events"),
    ("GET", "/v1/conversation"),
    ("GET", "/v1/conversations"),
    ("PATCH", "/v1/conversations/*"),
    ("DELETE", "/v1/conversations/*"),
    ("GET", "/v1/routines"),
    ("POST", "/v1/routines"),
    ("GET", "/v1/routines/*"),
    ("PATCH", "/v1/routines/*"),
    ("DELETE", "/v1/routines/*"),
    ("POST", "/v1/routines/*/run"),
    ("GET", "/v1/artifacts"),
    ("GET", "/v1/artifacts/*"),
    // A version's files, and its export (`/v1/artifacts/{id}/export/{version}`, ARTIFACTS-9): the
    // phone may save one. Importing stays on the Mac.
    ("GET", "/v1/artifacts/*/*/**"),
    ("POST", "/v1/conversation/stop"),
    ("POST", "/v1/plans"),
    ("POST", "/v1/plans/cancel"),
    ("GET", "/v1/jobs"),
    ("POST", "/v1/jobs"),
    ("GET", "/v1/repos"),
    ("GET", "/v1/jobs/*"),
    ("GET", "/v1/jobs/*/wait"),
    ("GET", "/v1/jobs/*/transcript"),
    ("GET", "/v1/jobs/*/transcript/*"),
    ("GET", "/v1/jobs/*/report"),
    ("POST", "/v1/jobs/*/stop"),
    ("POST", "/v1/stop-all"),
    ("POST", "/v1/jobs/*/dismiss"),
    ("POST", "/v1/jobs/*/archive"),
    ("DELETE", "/v1/jobs/*"),
    ("PATCH", "/v1/jobs/*"),
    ("POST", "/v1/jobs/*/delete"),
    // Settings › Agents, from the iPhone as from the Mac (the owner's decision of 2026-11-01): an
    // account's name, payer, model and efforts (never adding one, removing it or signing it in),
    // and which one new tasks go to.
    ("PATCH", "/v1/accounts/*"),
    ("PUT", "/v1/settings/jobs"),
    ("POST", "/v1/jobs/*/move"),
    ("POST", "/v1/jobs/*/start"),
    ("POST", "/v1/jobs/*/mode"),
    ("POST", "/v1/jobs/*/permission"),
    ("POST", "/v1/jobs/*/messages"),
    ("GET", "/v1/jobs/*/publish"),
    ("POST", "/v1/jobs/*/publish"),
    ("POST", "/v1/clear"),
    ("GET", "/v1/github/work"),
    ("POST", "/v1/github/work/refresh"),
    ("POST", "/v1/github/work/*/start"),
    ("GET", "/v1/inbox"),
    ("POST", "/v1/inbox/*/answer"),
    ("GET", "/v1/limit-decisions"),
    ("POST", "/v1/limit-decisions/*/answer"),
    ("GET", "/v1/failures"),
    ("GET", "/v1/failures/*"),
    ("POST", "/v1/failures/*/fix"),
    ("POST", "/v1/accounts/*/reserve"),
    ("GET", "/v1/settings/models"),
    ("GET", "/v1/settings/models/options"),
    ("GET", "/v1/settings/jobs"),
    ("GET", "/v1/settings/permissions"),
    ("PUT", "/v1/settings/permissions"),
    ("GET", "/v1/permissions/rules"),
    ("DELETE", "/v1/permissions/rules/*"),
    ("POST", "/v1/devices"),
    ("GET", "/v1/devices/*/pairing"),
    ("POST", "/v1/devices/*/pairing"),
    ("POST", "/v1/notifications"),
];

/// The routes another computer's token reaches past [`ROUTES`] (docs/contracts/remote.md,
/// COMPUTERS-14), each with the screen of the window that needs it there. Each is added on its own,
/// reviewed, and only where no doc of obid's keeps it on its loopback API.
pub const COMPUTER_ROUTES: &[(&str, &str, &str)] = &[
    (
        "GET",
        "/v1/skills",
        "the message box's `/` list, of the computer in use (COMPOSER-25)",
    ),
    (
        "GET",
        "/v1/connectors",
        "Settings › Connectors, of the computer in use",
    ),
];

/// Whether another computer's token may `method` `path` through the gateway: a phone's
/// [`ROUTES`], and [`COMPUTER_ROUTES`].
pub fn allows_computer(method: &Method, path: &str) -> bool {
    allows(method, path)
        || COMPUTER_ROUTES
            .iter()
            .any(|(m, p, _)| *m == method.as_str() && *p == path)
}

/// Whether a phone may `method` `path` through the gateway: `*` is one path segment.
pub fn allows(method: &Method, path: &str) -> bool {
    let actual: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    ROUTES.iter().any(|(m, pattern)| {
        let mut expected: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
        let rest = expected.last() == Some(&"**");
        if rest {
            expected.pop();
        }
        *m == method.as_str()
            && (if rest {
                actual.len() > expected.len()
            } else {
                actual.len() == expected.len()
            })
            && expected
                .iter()
                .zip(&actual)
                .all(|(e, a)| *e == "*" || e == a)
    })
}

/// `path` as one route, decoded, before any list is read: `None` when a segment is empty, `.` or
/// `..`, holds a backslash or NUL, or has a `%` escape that isn't two hex digits, isn't UTF-8, or
/// decodes to any of those, a `/` or a `%`. A path that could be read two ways is refused, so the
/// gateway's list reads the route obid's router serves (docs/contracts/remote.md, COMPUTERS-14).
pub fn plain_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix('/')?;
    let mut plain = String::with_capacity(path.len());
    for segment in rest.split('/') {
        let segment = decoded(segment)?;
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.contains(['/', '\\', '%', '\0'])
        {
            return None;
        }
        plain.push('/');
        plain.push_str(&segment);
    }
    Some(plain)
}

/// A path [`plain_path`] decoded, encoded the one standard way: every byte but the unreserved ones
/// (RFC 3986: letters, digits, `-`, `.`, `_`, `~`) as `%` and two upper-case hex digits. obid's
/// router matches it as the gateway's list read it, a literal segment as itself.
pub fn canonical(plain: &str) -> String {
    let mut out = String::with_capacity(plain.len());
    for (n, segment) in plain.split('/').enumerate() {
        if n > 0 {
            out.push('/');
        }
        for byte in segment.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                out.push(byte as char);
            } else {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

/// `segment` with its `%` escapes decoded, when each is two hex digits and the result is UTF-8.
fn decoded(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = std::str::from_utf8(bytes.get(at + 1..at + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The open invitation: digests only, never the secret or the code./// The open invitation: digests only, never the secret or the code.
struct Pending {
    secret: String,
    code: String,
    expires: SystemTime,
    attempts: u32,
    name: String,
    hostname: String,
    /// The SHA-256 of its relay token, when it pairs through the Obysphere.
    relay: Option<String>,
    /// The owner made it on their own device (`approval`): the phone it pairs may add its key.
    owned: bool,
    /// Another computer's (COMPUTERS-13, 14): it pairs a computer, not a phone.
    computer: bool,
    /// The registered key `obid pair-computer` named, owned invitations only: redeeming binds it to
    /// the pairing it makes (COMPUTERS-15).
    key: Option<String>,
}

/// A paired device, kept in `home/gateway-devices.json` under its token's digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Device {
    id: String,
    name: String,
    created: i64,
    expires_at: i64,
    /// The SHA-256 of its relay token, when it paired through the Obysphere: then its token works
    /// only in its sealed envelopes, on that relay token's socket (crate::relay).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    relay: Option<String>,
    /// Until when it may add its own publishing key (seconds since the epoch): set only when it
    /// redeemed an invitation the owner made on their own device. A pairing from before has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keyable_until: Option<i64>,
    /// Another computer of the owner's, not a phone (COMPUTERS-14): only its own key's words count
    /// through its token, and it may end its own pairing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    computer: bool,
}

impl Device {
    /// `at`, but never past 90 days from pairing for a device not paired through the Obysphere
    /// (PAIRING-7). Every record has its pairing time, `created`, since the first.
    fn capped(&self, at: i64) -> i64 {
        match self.relay {
            Some(_) => at,
            None => at.min(self.created + PAIRED.as_secs() as i64),
        }
    }

    /// When its token stops working: its sliding expiry, capped. A record that slid past the cap
    /// before PAIRING-7 is capped here too.
    fn until(&self) -> i64 {
        self.capped(self.expires_at)
    }
}

/// What `POST /v1/pairing/invite` answers: the QR code's parts and the six digits.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invitation {
    pub url: String,
    pub name: String,
    pub hostname: String,
    /// `{"hostname":…,"name":…}`, and `relay` when there is one, as the QR code carries it.
    pub link: String,
    pub secret: String,
    pub code: String,
    pub expires_at: String,
    /// How the phone reaches obid through its Obysphere: what the link carries too.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<Value>,
    /// Another computer's invitation: `oby-computer:` and this computer's direct addresses, each
    /// pinned (crate::remote::invite, COMPUTERS-13), when it has any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computer_link: Option<String>,
}

/// What a phone needs to reach obid through its Obysphere (docs/runner-api.md, "obid only: the
/// phone relay"): the Sphere's base URL, the relay's id, the phone's own relay token and the Mac's
/// X25519 public key, all in the QR code.
pub struct RelayInvite {
    pub sphere: String,
    pub relay: String,
    pub token: String,
    pub key: String,
}

/// What redeeming buys: the device token, shown once, and the Mac's link.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Redeemed {
    pub token: String,
    #[serde(rename = "deviceID")]
    pub device_id: String,
    pub expires_at: String,
    pub link: Value,
    /// The key the invitation named, to bind to this pairing (COMPUTERS-15): never answered.
    #[serde(skip)]
    pub key: Option<String>,
}

/// The paired device a forwarded request came from, as a request extension: only the gateway's
/// in-process call sets it, so nothing that reaches the API over a socket can claim a device.
#[derive(Debug, Clone)]
pub struct PairedDevice(pub String);

/// A request that came sealed through the phone relay (crate::relay), with the device id the
/// Sphere routed it by (the SHA-256 of a relay token, hex): set only in process, as
/// [`PairedDevice`] is.
#[derive(Debug, Clone)]
pub struct Relayed(pub String);

pub struct Gateway {
    file: PathBuf,
    devices: Mutex<HashMap<String, Device>>,
    pending: Mutex<Option<Pending>>,
    /// Another computer's open invitation, apart from the phone's: neither replaces the other
    /// (COMPUTERS-13).
    computer: Mutex<Option<Pending>>,
}

fn digest(value: &str) -> String {
    hex::encode(obi_protocol::keys::sha256(value.as_bytes()))
}

fn random(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).expect("the system's random source works");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer)
}

fn seconds(at: SystemTime) -> i64 {
    at.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Gateway {
    /// The devices paired before, from `home/gateway-devices.json`. A file that doesn't read
    /// counts as none: every phone pairs again.
    pub fn open(home: &Path) -> Gateway {
        let file = home.join("gateway-devices.json");
        let devices = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Gateway {
            file,
            devices: Mutex::new(devices),
            pending: Mutex::new(None),
            computer: Mutex::new(None),
        }
    }

    /// A new invitation, replacing the one before. `owned`: the owner made it on their own device,
    /// so the phone it pairs may add its publishing key.
    pub fn invite(
        &self,
        url: &str,
        name: &str,
        hostname: &str,
        now: SystemTime,
        relay: Option<RelayInvite>,
        owned: bool,
    ) -> Invitation {
        self.make(url, name, hostname, now, relay, owned, None)
    }

    /// Another computer's invitation (COMPUTERS-13), replacing the computer's one before but not a
    /// phone's: ten minutes, never through the Obysphere. `owned`: the owner made it with their
    /// own device or at this computer (`obid invite-computer`), so the computer it pairs may add
    /// its publishing key. `key`, owned only: the registered key redeeming binds to the pairing.
    pub fn invite_computer(
        &self,
        url: &str,
        name: &str,
        hostname: &str,
        now: SystemTime,
        owned: bool,
        key: Option<String>,
    ) -> Invitation {
        self.make(
            url,
            name,
            hostname,
            now,
            None,
            owned,
            Some(key.filter(|_| owned)),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn make(
        &self,
        url: &str,
        name: &str,
        hostname: &str,
        now: SystemTime,
        relay: Option<RelayInvite>,
        owned: bool,
        computer: Option<Option<String>>,
    ) -> Invitation {
        let secret = random(32);
        let code = {
            let mut bytes = [0u8; 4];
            getrandom::fill(&mut bytes).expect("the system's random source works");
            format!("{:06}", u32::from_le_bytes(bytes) % 1_000_000)
        };
        let expires = now
            + if computer.is_some() {
                COMPUTER_INVITATION
            } else {
                INVITATION
            };
        let slot = if computer.is_some() {
            &self.computer
        } else {
            &self.pending
        };
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(Pending {
            secret: digest(&secret),
            code: digest(&code),
            expires,
            attempts: 0,
            name: name.to_owned(),
            hostname: hostname.to_owned(),
            relay: relay.as_ref().map(|r| digest(&r.token)),
            owned,
            computer: computer.is_some(),
            key: computer.flatten(),
        });
        let relay = relay
            .map(|r| json!({"sphere": r.sphere, "id": r.relay, "token": r.token, "key": r.key}));
        let mut link = json!({"hostname": hostname, "name": name});
        if let Some(relay) = &relay {
            link["relay"] = relay.clone();
        }
        Invitation {
            url: url.to_owned(),
            name: name.to_owned(),
            hostname: hostname.to_owned(),
            link: link.to_string(),
            secret,
            code,
            expires_at: obi_protocol::time::format(seconds(expires)),
            relay,
            computer_link: None,
        }
    }

    /// Redeems the open invitation with its secret or its code, once: `410` when there's none or
    /// it expired, `401` for a wrong credential (the fifth uses it up). `via` is the relay device
    /// id a sealed request came by, and must be the invitation's own relay token's: an invitation
    /// made through the Obysphere is redeemed only sealed, on that token's socket, and pairs a
    /// device bound to it; any other only on the listener (`None`), never through a relay.
    pub fn redeem(
        &self,
        credential: &str,
        name: &str,
        now: SystemTime,
        via: Option<&str>,
    ) -> Result<Redeemed, (StatusCode, &'static str)> {
        const GONE: &str = "That pairing invitation has expired or was already used.";
        let given = digest(credential);
        let open = {
            let mut phone = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            let mut computer = self.computer.lock().unwrap_or_else(|e| e.into_inner());
            for slot in [&mut *computer, &mut *phone] {
                if slot.as_ref().is_some_and(|p| now > p.expires) {
                    *slot = None;
                }
            }
            if phone.is_none() && computer.is_none() {
                return Err((StatusCode::GONE, GONE));
            }
            let matches = |p: &Pending| {
                p.relay.as_deref() == via
                    && (crate::token::same(&given, &p.secret)
                        || crate::token::same(&given, &p.code))
            };
            if computer.as_ref().is_some_and(matches) {
                computer.take().expect("checked above")
            } else if phone.as_ref().is_some_and(matches) {
                phone.take().expect("checked above")
            } else {
                // A wrong try counts against each open invitation; the fifth ends it.
                for slot in [&mut *computer, &mut *phone] {
                    if let Some(open) = slot.as_mut() {
                        open.attempts += 1;
                        if open.attempts >= ATTEMPTS {
                            *slot = None;
                        }
                    }
                }
                return Err((StatusCode::UNAUTHORIZED, "That pairing code is not valid."));
            }
        };
        let token = random(64);
        let device = Device {
            id: uuid::Uuid::new_v4().to_string(),
            // No control characters: the name goes in obid's log.
            name: name.chars().filter(|c| !c.is_control()).take(100).collect(),
            created: seconds(now),
            expires_at: seconds(now + LIFETIME),
            relay: via.map(str::to_owned),
            keyable_until: open.owned.then(|| seconds(now + KEYABLE)),
            computer: open.computer,
        };
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        devices.insert(digest(&token), device.clone());
        if self.save(&devices).is_err() {
            devices.remove(&digest(&token));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "obid couldn't keep the paired device.",
            ));
        }
        tracing::info!("paired device {}: {}", device.id, device.name);
        Ok(Redeemed {
            token,
            device_id: device.id,
            expires_at: obi_protocol::time::format(device.expires_at),
            link: json!({"name": open.name, "hostname": open.hostname}),
            key: open.key,
        })
    }

    /// Whether the paired device `id` may add its own publishing key now: it redeemed an invitation
    /// the owner made on their own device, in the last 10 minutes, and its token still works.
    pub fn may_add_key(&self, id: &str, now: SystemTime) -> bool {
        let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        devices.values().any(|d| {
            d.id == id
                && d.until() > seconds(now)
                && d.keyable_until.is_some_and(|until| until > seconds(now))
        })
    }

    /// Whether the paired device `id` is another computer of the owner's (COMPUTERS-14).
    pub fn is_computer(&self, id: &str) -> bool {
        let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        devices.values().any(|d| d.id == id && d.computer)
    }

    /// Whether `token` is a paired device's, and still works.
    pub fn accepts(&self, token: &str, now: SystemTime) -> bool {
        self.device(token, now).is_some()
    }

    /// The id of the paired device `token` is, while it still works.
    pub fn device(&self, token: &str, now: SystemTime) -> Option<String> {
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        self.used(&mut devices, token, now, |_| true)?.ok()
    }

    /// [`Gateway::device`], only where that device may be reached: a device paired through the
    /// relay only `via` its own relay token's socket, sealed; any other only on the listener
    /// (`via` none). `Err` is a working token in the wrong place, and doesn't extend it.
    pub fn device_via(
        &self,
        token: &str,
        now: SystemTime,
        via: Option<&str>,
    ) -> Option<Result<String, ()>> {
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        self.used(&mut devices, token, now, |d| d.relay.as_deref() == via)
    }

    /// The id of the device `token` is, while it still works: `Err` when it isn't `right` here.
    /// Only a right use gives it a fresh [`LIFETIME`], once it has under [`SLIDE`] left
    /// (PAIRING-6), so a request that always fails can't keep a leaked token alive. Saved only
    /// when extended; if the save fails the old expiry stays, so the next request tries again.
    fn used(
        &self,
        devices: &mut HashMap<String, Device>,
        token: &str,
        now: SystemTime,
        right: impl Fn(&Device) -> bool,
    ) -> Option<Result<String, ()>> {
        let key = digest(token);
        let device = devices.get_mut(&key).filter(|d| d.until() > seconds(now))?;
        if !right(device) {
            return Some(Err(()));
        }
        let id = device.id.clone();
        // Never past the 90 days (PAIRING-7): at the cap there's nothing to write.
        let fresh = device.capped(seconds(now + LIFETIME));
        if device.until() >= seconds(now + SLIDE) || fresh <= device.expires_at {
            return Some(Ok(id));
        }
        let before = device.expires_at;
        device.expires_at = fresh;
        if let Err(e) = self.save(devices) {
            tracing::warn!("couldn't extend paired device {id}: {e}");
            if let Some(d) = devices.get_mut(&key) {
                d.expires_at = before;
            }
        }
        Some(Ok(id))
    }

    /// The relay tokens' digests the Obysphere must know (crate::relay): every device paired
    /// through it that still works, and the open invitation's.
    pub fn relay_devices(&self, now: SystemTime) -> HashSet<String> {
        let mut wanted: HashSet<String> = {
            let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
            devices
                .values()
                .filter(|d| d.until() > seconds(now))
                .filter_map(|d| d.relay.clone())
                .collect()
        };
        let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(open) = pending.as_ref().filter(|p| p.expires > now) {
            wanted.extend(open.relay.clone());
        }
        wanted
    }

    /// The relay token digest of the device `id`, when it paired through the Obysphere.
    pub fn relay_of(&self, id: &str) -> Option<String> {
        let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        devices
            .values()
            .find(|d| d.id == id)
            .and_then(|d| d.relay.clone())
    }

    /// The ids of the devices still paired at `now`.
    pub fn paired(&self, now: SystemTime) -> HashSet<String> {
        let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        devices
            .values()
            .filter(|d| d.until() > seconds(now))
            .map(|d| d.id.clone())
            .collect()
    }

    /// The paired devices, oldest first: never a token or its digest.
    pub fn list(&self) -> Value {
        let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let mut listed: Vec<&Device> = devices.values().collect();
        listed.sort_by_key(|d| d.created);
        listed
            .into_iter()
            .map(|d| {
                let mut listed = json!({"id": d.id, "name": d.name,
                       "created": obi_protocol::time::format(d.created),
                       "expiresAt": obi_protocol::time::format(d.until())});
                if d.computer {
                    listed["computer"] = json!(true);
                }
                listed
            })
            .collect()
    }

    /// Forgets the device `id`: false when there's none.
    pub fn revoke(&self, id: &str) -> Result<bool, std::io::Error> {
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let before = devices.len();
        devices.retain(|_, d| d.id != id);
        if devices.len() == before {
            return Ok(false);
        }
        self.save(&devices)?;
        Ok(true)
    }

    fn save(&self, devices: &HashMap<String, Device>) -> Result<(), std::io::Error> {
        let bytes = serde_json::to_vec_pretty(devices).map_err(std::io::Error::other)?;
        crate::store::write_private(&self.file, &bytes)
    }
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(json!({"error": message}))).into_response()
}

// ---------------------------------------------------------------------------------------------
// The loopback API's pairing endpoints, behind the API token

#[derive(Deserialize)]
pub struct InviteRequest {
    url: Option<String>,
    name: Option<String>,
    hostname: Option<String>,
    /// The owner's device's signature over `"oby-pair-invite\n<signedAt>"` (crate::devices): the
    /// phone it pairs may add its publishing key. Without one it pairs a phone that can't.
    approval: Option<Value>,
    /// Another computer of the owner's, added over SSH (`obid pair-computer`, crate::remote::pair;
    /// docs/contracts/remote.md, COMPUTERS-8): the invitation is for this gateway's `url` only, never
    /// through the Obysphere, which is never the way to one's own computers.
    #[serde(default)]
    computer: bool,
    /// With `computer`: what `obid invite-computer` wrote in `home/invite-computer` at this machine,
    /// the owner's presence there (crate::remote::invite), so the computer it pairs may add its key.
    presence: Option<String>,
    /// With `computer`: a registered key bound to nothing yet, which the pairing it makes takes as
    /// its own (`obid pair-computer`, COMPUTERS-15). Only an owned invitation names one.
    device: Option<String>,
}

/// `POST /v1/pairing/invite`. With an Obysphere the phone pairs through it, sealed (crate::relay),
/// and only so: the invitation carries its relay token, registered with the Sphere first, and no
/// route to the listener (a `url` given is left out). Without one, only a `url` the phone reaches
/// the gateway by (Remote access, the tailnet) makes an invitation. An `approval` that doesn't
/// hold is `403` with nothing made.
pub async fn invite(
    State(shared): State<Arc<Shared>>,
    axum::Json(request): axum::Json<InviteRequest>,
) -> Response {
    let present = request.computer
        && request.presence.as_deref().is_some_and(|nonce| {
            crate::remote::invite::presence_check(&shared.config.mac_home, nonce, SystemTime::now())
        });
    let owned = match &request.approval {
        None => present,
        Some(approval) if shared.devices.approves_invite(approval) => true,
        Some(_) => {
            return error(
                StatusCode::FORBIDDEN,
                "That approval isn't a registered device's, signed for an invitation in the last two minutes. Try again.",
            );
        }
    };
    let authority = match request.url.as_deref().map(|url| url.split_once("://")) {
        None => None,
        Some(Some((scheme, rest)))
            if matches!(scheme, "http" | "https")
                && !rest.split('/').next().unwrap_or("").is_empty() =>
        {
            Some(rest.split('/').next().unwrap_or("").to_owned())
        }
        Some(_) => return error(StatusCode::BAD_REQUEST, "url must be an HTTP or HTTPS URL."),
    };
    let relay = match request.computer {
        true if authority.is_none() => {
            return error(
                StatusCode::BAD_REQUEST,
                "Another computer is invited to a url: its gateway's.",
            );
        }
        true => None,
        false => match shared.relay.invite(&shared.link).await {
            Ok(relay) => relay,
            Err((status, message)) => return error(status, &message),
        },
    };
    let name = request.name.unwrap_or_else(|| shared.host.clone());
    let (url, hostname) = match (request.url, authority, &relay) {
        (_, _, Some(relay)) => (
            relay.sphere.clone(),
            request.hostname.unwrap_or_else(|| shared.host.clone()),
        ),
        (Some(url), Some(authority), None) => (url, request.hostname.unwrap_or(authority)),
        _ => {
            return error(
                StatusCode::CONFLICT,
                "Join or host an Obysphere first (Settings › Obysphere).",
            );
        }
    };
    let through_relay = relay.is_some();
    if request.computer {
        if let Some(device) = &request.device
            && !(owned && {
                let paired = shared.gateway.paired(SystemTime::now());
                shared.devices.bindable(device, |p| paired.contains(p))
            })
        {
            return error(
                StatusCode::FORBIDDEN,
                "A computer's invitation names a key only when it's made at that computer, for a key registered there and no pairing's yet.",
            );
        }
        let mut invitation = shared.gateway.invite_computer(
            &url,
            &name,
            &hostname,
            SystemTime::now(),
            owned,
            request.device,
        );
        invitation.computer_link = shared
            .gateway_state
            .as_ref()
            .and_then(|state| crate::remote::invite::link(&name, &state["direct"]))
            .map(|link| link.encode());
        return axum::Json(invitation).into_response();
    }
    let invitation = shared
        .gateway
        .invite(&url, &name, &hostname, SystemTime::now(), relay, owned);
    shared.relay.wake();
    if through_relay {
        shared.relay.ready(Duration::from_secs(5)).await;
    }
    axum::Json(invitation).into_response()
}

/// `GET /v1/pairing/devices`.
pub async fn devices(State(shared): State<Arc<Shared>>) -> Response {
    axum::Json(shared.gateway.list()).into_response()
}

/// `DELETE /v1/pairing/devices/{id}`.
pub async fn revoke(
    State(shared): State<Arc<Shared>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    end(&shared, &id)
}

/// Ends the pairing `id`: its token, its push tokens, the publishing key it registered, and its
/// sealed session if it had one.
fn end(shared: &Shared, id: &str) -> Response {
    let id = id.to_owned();
    let relay = shared.gateway.relay_of(&id);
    match shared.gateway.revoke(&id) {
        Ok(true) => {
            // A lost phone gets no more alerts: its push tokens go with it, and so does the
            // publishing key it registered with this pairing (crate::devices).
            shared.push.forget(&id);
            if let Err(e) = shared.devices.forget_paired(&id) {
                tracing::error!("can't forget the publishing key of paired device {id}: {e}");
            }
            // Its sealed session ends now, and its relay token opens nothing once the Sphere
            // forgets it (crate::relay).
            if let Some(relay) = relay {
                shared.relay.forget(&relay);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => error(StatusCode::NOT_FOUND, "That paired device was not found."),
        Err(_) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "obid couldn't forget the paired device.",
        ),
    }
}

// ---------------------------------------------------------------------------------------------
// The gateway's own listener

#[derive(Clone)]
struct Through {
    shared: Arc<Shared>,
    api: Router,
}

/// The gateway's router: redeeming, a computer ending its own pairing, and every allowed route
/// forwarded to `api`.
pub fn router(shared: Arc<Shared>, api: Router) -> Router {
    Router::new()
        .route("/v1/pairing", post(redeem).delete(unpair))
        .fallback(forward)
        .with_state(Through { shared, api })
}

#[derive(Deserialize)]
struct RedeemRequest {
    credential: String,
    name: Option<String>,
}

/// `POST /v1/pairing`, on the gateway only. Only a phone redeems: a browser says its `Origin`,
/// and a page can't send JSON across origins without asking first, so neither a page the owner
/// opens nor a rebound name can use an invitation up.
async fn redeem(State(through): State<Through>, request: Request) -> Response {
    let via = request.extensions().get::<Relayed>().map(|r| r.0.clone());
    if request.headers().contains_key(header::ORIGIN) {
        return error(StatusCode::FORBIDDEN, "Only a phone pairs with obid.");
    }
    let json = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("application/json"));
    if !json {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Pairing takes JSON (Content-Type: application/json).",
        );
    }
    let Ok(bytes) = axum::body::to_bytes(request.into_body(), 64 * 1024).await else {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "That request is too large.");
    };
    let Ok(asked) = serde_json::from_slice::<RedeemRequest>(&bytes) else {
        return error(
            StatusCode::BAD_REQUEST,
            "Pairing takes {\"credential\": …, \"name\": …}.",
        );
    };
    let name = asked.name.unwrap_or_else(|| "iPhone".into());
    let redeemed =
        through
            .shared
            .gateway
            .redeem(&asked.credential, &name, SystemTime::now(), via.as_deref());
    // The invitation's relay token is a device's now, or no longer wanted.
    through.shared.relay.wake();
    match redeemed {
        Ok(redeemed) => {
            // The key `obid pair-computer` named is this pairing's own now (COMPUTERS-15).
            let paired = through.shared.gateway.paired(SystemTime::now());
            if let Some(key) = &redeemed.key
                && let Err(e) = through
                    .shared
                    .devices
                    .bind_named(key, &redeemed.device_id, |p| {
                        p != redeemed.device_id && paired.contains(p)
                    })
            {
                tracing::warn!(
                    "couldn't bind key {key} to computer {}: {e}",
                    redeemed.device_id
                );
            }
            axum::Json(redeemed).into_response()
        }
        Err((status, message)) => error(status, message),
    }
}

/// The one `Authorization: Bearer` token a request carries, or none.
fn bearer(headers: &axum::http::HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    match (values.next(), values.next()) {
        (Some(value), None) => value
            .to_str()
            .ok()
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim),
        _ => None,
    }
}

/// `DELETE /v1/pairing`, on the gateway only: another computer's own token ends its pairing, as
/// "Remove this computer" in the window does (COMPUTERS-16), and forgets the key it registered,
/// as revoking on that computer does. A phone's token isn't one.
async fn unpair(State(through): State<Through>, request: Request) -> Response {
    let via = request.extensions().get::<Relayed>().map(|r| r.0.clone());
    let device = bearer(request.headers()).and_then(|t| {
        through
            .shared
            .gateway
            .device_via(t, SystemTime::now(), via.as_deref())
    });
    match device {
        Some(Ok(id)) if through.shared.gateway.is_computer(&id) => end(&through.shared, &id),
        Some(Ok(_)) => error(
            StatusCode::NOT_FOUND,
            "That route isn't available through the gateway.",
        ),
        _ => error(
            StatusCode::UNAUTHORIZED,
            "Every gateway request needs a device token.",
        ),
    }
}

/// Every other request: an allowed route with a paired device's token goes to the API, as obid's
/// own loopback call with the API token. Nothing else the phone sent goes with it but its body and
/// its content type.
async fn forward(State(through): State<Through>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    // Only the window on a computer's own Mac names a computer (COMPUTERS-22): a request through
    // the gateway that does is refused, never sent on from here.
    if parts.headers.contains_key(crate::remote::computers::HEADER) {
        return error(
            StatusCode::BAD_REQUEST,
            "Only Oby's window on its own Mac works on another computer.",
        );
    }
    // A phone's and a computer's token reach the same list (COMPUTERS-14): a route off it is one
    // obid's docs keep on its loopback API, until the owner adds it by name.
    let Some(path) =
        plain_path(parts.uri.path()).filter(|path| allows_computer(&parts.method, path))
    else {
        return error(
            StatusCode::NOT_FOUND,
            "That route isn't available through the gateway.",
        );
    };
    let token = bearer(&parts.headers);
    // A route on a phone's list, or only on a computer's; without a device token that works here,
    // one off a phone's list is `404`, as it always was.
    let listed = allows(&parts.method, &path);
    // A phone paired through the Obysphere is reached only sealed, by its own relay token's
    // socket; any other only here, on the listener.
    let via = parts.extensions.get::<Relayed>().map(|r| r.0.as_str());
    let device =
        match token.and_then(|t| through.shared.gateway.device_via(t, SystemTime::now(), via)) {
            Some(Ok(device)) => device,
            _ if !listed => {
                return error(
                    StatusCode::NOT_FOUND,
                    "That route isn't available through the gateway.",
                );
            }
            Some(Err(())) if via.is_none() => {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "This phone reaches obid sealed, through its Obysphere.",
                );
            }
            Some(Err(())) => {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "That device token isn't this phone's.",
                );
            }
            None => {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "Every gateway request needs a device token.",
                );
            }
        };
    // Only another computer's own key's words count through its token (COMPUTERS-15).
    if through.shared.gateway.is_computer(&device) {
        let words = parts
            .headers
            .get(crate::approval::WORDS)
            .map(|v| v.to_str().ok().and_then(|v| v.split(' ').next()));
        if let Some(signer) = words
            && !signer.is_some_and(|s| through.shared.devices.belongs_to_pairing(s, &device))
        {
            return error(
                StatusCode::FORBIDDEN,
                "Those words aren't signed by this computer's own key. Add this computer again.",
            );
        }
    } else if !listed {
        // A route only a computer's token reaches.
        return error(
            StatusCode::NOT_FOUND,
            "That route isn't available through the gateway.",
        );
    }
    let Ok(body) = axum::body::to_bytes(body, BODY).await else {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "That request is too large.");
    };
    // obid serves exactly the route the list read: the decoded path, encoded the one standard way,
    // never the request's own spelling (COMPUTERS-14).
    let target = match parts.uri.query() {
        Some(query) => format!("{}?{query}", canonical(&path)),
        None => canonical(&path),
    };
    let mut upstream = Request::builder()
        .method(parts.method)
        .uri(target)
        .header(header::HOST, "127.0.0.1")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", through.shared.token),
        )
        .extension(PairedDevice(device));
    if let Some(kind) = parts.headers.get(header::CONTENT_TYPE) {
        upstream = upstream.header(header::CONTENT_TYPE, kind);
    }
    // The phone's signature over words its owner typed (crate::approval::WORDS): checked there.
    if let Some(signed) = parts.headers.get(crate::approval::WORDS) {
        upstream = upstream.header(crate::approval::WORDS, signed);
    }
    // A request key (docs/contracts/receipts.md): a retried request gets its first answer.
    if let Some(key) = parts.headers.get("idempotency-key") {
        upstream = upstream.header("idempotency-key", key);
    }
    let Ok(upstream) = upstream.body(Body::from(body)) else {
        return error(StatusCode::BAD_REQUEST, "That request can't be forwarded.");
    };
    let response = match through.api.oneshot(upstream).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let scrubber = Scrubber {
        homes: homes(&through.shared.config),
    };
    let token = token.unwrap_or_default().to_owned();
    let shared = through.shared.clone();
    scrubbed(response, scrubber, move || {
        shared.gateway.accepts(&token, SystemTime::now())
    })
    .await
}

/// The accounts' own folders, and the jobs' copies of their logins, which never leave in an answer.
fn homes(config: &crate::config::Config) -> Vec<String> {
    let agents = config.agents.get();
    [agents.claude, agents.codex, agents.cursor]
        .into_iter()
        .flatten()
        .flat_map(|agent| agent.accounts)
        .filter_map(|account| account.config_dir)
        // Where obid copies a job's login for its agent (crate::jobs, `login_home`).
        .chain([config.home.join("logins")])
        .map(|dir| dir.to_string_lossy().into_owned())
        .filter(|dir| !dir.is_empty())
        .collect()
}

/// The API's answer, with its status and content type only, scrubbed: whole when it has a known
/// length, chunk by chunk otherwise (obid writes each event in one chunk, as crate::vault::answers
/// relies on too, and `/wait` a space at once and the job in one chunk at its end). A stream stops
/// at the first chunk after its device was revoked, before that chunk goes out.
async fn scrubbed(
    response: Response,
    scrubber: Scrubber,
    still_paired: impl Fn() -> bool + Send + 'static,
) -> Response {
    let (parts, body) = response.into_parts();
    let kind = parts.headers.get(header::CONTENT_TYPE).cloned();
    let kind_text = kind
        .as_ref()
        .and_then(|k| k.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let events = kind_text.starts_with("text/event-stream");
    // What isn't text (an artifact's image or PDF) goes as it is: read as text, it would break.
    // The API's own layer (crate::vault::answers) passes it as it is too.
    let text = kind_text.is_empty()
        || kind_text.starts_with("application/json")
        || kind_text.starts_with("text/")
        || kind_text.contains("+json");
    let mut out = if !text {
        if body.size_hint().exact().is_none() {
            Response::new(Body::from_stream(
                body.into_data_stream()
                    .take_while(move |_| std::future::ready(still_paired())),
            ))
        } else {
            Response::new(body)
        }
    } else if events || body.size_hint().exact().is_none() {
        let chunks = body
            .into_data_stream()
            .take_while(move |_| std::future::ready(still_paired()))
            .map(move |chunk| {
                chunk.map(|bytes| {
                    let text = String::from_utf8_lossy(&bytes);
                    Bytes::from(if events {
                        scrubber.frame(&text)
                    } else {
                        scrubber.document(&text)
                    })
                })
            });
        Response::new(Body::from_stream(chunks))
    } else {
        let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
            return error(
                StatusCode::BAD_GATEWAY,
                "The gateway got no whole answer from obid.",
            );
        };
        Response::new(Body::from(
            scrubber.document(&String::from_utf8_lossy(&bytes)),
        ))
    };
    *out.status_mut() = parts.status;
    let headers = out.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        kind.unwrap_or(HeaderValue::from_static("application/json")),
    );
    // An artifact's file keeps what holds it in (crate::artifacts, POLICY).
    for name in [
        header::CACHE_CONTROL,
        header::CONTENT_SECURITY_POLICY,
        header::X_CONTENT_TYPE_OPTIONS,
        header::REFERRER_POLICY,
    ] {
        if let Some(value) = parts.headers.get(&name) {
            headers.insert(name, value.clone());
        }
    }
    out
}

/// Events about logins and the watcher, which only the Mac's own Settings uses: Tetrix's
/// sign-in, and the watcher's rules and runs (what people wrote, quoted). And the Sphere's room,
/// which a phone doesn't reach yet (docs/contracts/rooms.md).
const MAC_ONLY_EVENTS: &[&str] = &["tetrix", "hooks", "watch", "roomPost"];

/// `text`, a piece of the event stream, without the frames of [`MAC_ONLY_EVENTS`]. The stream
/// comes a frame at a time, each ending with a blank line.
fn without_mac_only(text: &str) -> String {
    text.split_inclusive("\n\n")
        .filter(|frame| {
            !frame.lines().any(|line| {
                line.strip_prefix("event:")
                    .is_some_and(|name| MAC_ONLY_EVENTS.contains(&name.trim()))
            })
        })
        .collect()
}

/// What never leaves in a gateway answer besides the vault's secrets (already out): an agent's
/// session id (a job's `session`, which resumes the agent's own conversation), as null, and the
/// accounts' homes, as `[home]` (Runner/Gateway.swift, `GatewayScrubber`).
struct Scrubber {
    homes: Vec<String>,
}

impl Scrubber {
    fn document(&self, text: &str) -> String {
        self.without_homes(&self.json(text))
    }

    /// One event-stream frame: each `data:` line's JSON as a document. The Mac's own events
    /// ([`MAC_ONLY_EVENTS`]) never reach a phone.
    fn frame(&self, text: &str) -> String {
        let text = &without_mac_only(text);
        let lines: Vec<String> = text
            .split('\n')
            .map(|line| match line.strip_prefix("data:") {
                Some(data) => format!("data: {}", self.json(data.trim_start_matches(' '))),
                None => line.to_owned(),
            })
            .collect();
        self.without_homes(&lines.join("\n"))
    }

    /// `text` as JSON with `session` and `resumeCommand` null and the homes out of every string, or as it came when
    /// it isn't JSON or nothing in it changes.
    fn json(&self, text: &str) -> String {
        let Ok(mut value) = serde_json::from_str::<Value>(text) else {
            return text.to_owned();
        };
        if self.scrub(&mut value) {
            value.to_string()
        } else {
            text.to_owned()
        }
    }

    fn scrub(&self, value: &mut Value) -> bool {
        match value {
            Value::Object(fields) => {
                let mut changed = false;
                for (key, field) in fields.iter_mut() {
                    // An own job's `resumeCommand` names its session too (LANES-9).
                    if (key == "session" || key == "resumeCommand") && !field.is_null() {
                        *field = Value::Null;
                        changed = true;
                    } else {
                        changed |= self.scrub(field);
                    }
                }
                changed
            }
            Value::Array(items) => items
                .iter_mut()
                .fold(false, |changed, item| self.scrub(item) | changed),
            Value::String(text) => {
                let clean = self.without_homes(text);
                let changed = clean != *text;
                *text = clean;
                changed
            }
            _ => false,
        }
    }

    fn without_homes(&self, text: &str) -> String {
        self.homes.iter().fold(text.to_owned(), |text, home| {
            text.replace(home.as_str(), HOME)
        })
    }
}

#[cfg(test)]
mod tests {
    /// ROOMS: a phone doesn't reach the room yet, so its event doesn't either.
    #[test]
    fn the_rooms_event_never_reaches_a_phone() {
        let stream = "event: job\ndata: {}\n\nevent: roomPost\ndata: {\"text\":\"x\"}\n\n";
        assert_eq!(super::without_mac_only(stream), "event: job\ndata: {}\n\n");
    }

    use super::*;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("obid-gateway-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// Ports GatewayTests.gatewayRoutesDefaultToDeny: only the listed routes, by method and
    /// segment count.
    #[test]
    fn the_gateway_allows_only_the_listed_routes() {
        assert!(allows(&Method::GET, "/v1/status"));
        assert!(allows(&Method::GET, "/v1/jobs/abc"));
        assert!(allows(&Method::POST, "/v1/jobs/abc/messages"));
        assert!(allows(&Method::PATCH, "/v1/routines/a1b2c3d4"));
        assert!(allows(&Method::POST, "/v1/routines/a1b2c3d4/run"));
        assert!(!allows(&Method::PUT, "/v1/routines/a1b2c3d4"));
        assert!(!allows(&Method::PUT, "/v1/jobs/abc"));
        assert!(!allows(&Method::DELETE, "/v1/jobs/abc/messages"));
        // Sangam's Archive task and Delete task, from the iPhone too.
        assert!(allows(&Method::POST, "/v1/jobs/abc/archive"));
        assert!(allows(&Method::DELETE, "/v1/jobs/abc"));
        assert!(allows(&Method::PATCH, "/v1/jobs/abc"));
        // Delete task as the apps ask it: obid's Confirm first for work that isn't published.
        assert!(allows(&Method::POST, "/v1/jobs/abc/delete"));
        // The GitHub work feed: read it, refresh it, start an item's work (docs/runner-api.md,
        // "obid only: GitHub work"); nothing else under it.
        assert!(allows(&Method::GET, "/v1/github/work"));
        assert!(allows(&Method::POST, "/v1/github/work/refresh"));
        assert!(allows(&Method::POST, "/v1/github/work/abc/start"));
        assert!(!allows(&Method::POST, "/v1/github/work"));
        assert!(!allows(&Method::GET, "/v1/github/work/abc"));
        // Tetrix's login and the watcher are the Mac's alone, their events too.
        let scrubber = Scrubber { homes: vec![] };
        let stream = "event: job\ndata: {\"id\":\"j1\"}\n\nevent: watch\ndata: {\"rules\":{}}\n\nevent: tetrix\ndata: {}\n\nevent: inbox\ndata: {\"id\":\"i1\"}\n\n";
        assert_eq!(
            scrubber.frame(stream),
            "event: job\ndata: {\"id\":\"j1\"}\n\nevent: inbox\ndata: {\"id\":\"i1\"}\n\n"
        );
        assert!(!allows(&Method::GET, "/v1/slack"));
        assert!(!allows(&Method::PATCH, "/v1/slack"));
        // Sentry's and the hooks' routes are gone (docs/trim.md): never through the gateway.
        assert!(!allows(&Method::GET, "/v1/sentry"));
        assert!(!allows(&Method::PUT, "/v1/sentry"));
        assert!(!allows(&Method::PATCH, "/v1/sentry/projects/api"));
        // Hooks are made and forgotten on the Mac; only their deliveries come in, on a route of
        // the gateway's own (`deliver`), never through `allows`.
        assert!(!allows(&Method::GET, "/v1/hooks"));
        assert!(!allows(&Method::POST, "/v1/hooks"));
        assert!(!allows(&Method::DELETE, "/v1/hooks/abc"));
        assert!(!allows(&Method::POST, "/v1/hooks/abc/deliver"));
        assert!(!allows(&Method::POST, "/v1/slack/sign-in"));
        assert!(!allows(&Method::PATCH, "/v1/watch"));
        assert!(!allows(&Method::POST, "/v1/watch/run"));
        assert!(!allows(&Method::PUT, "/v1/listen"));
        assert!(!allows(&Method::POST, "/v1/listen/catch-up"));
        // Settings › Agents on the iPhone (the owner's decision of 2026-11-01): an account's name,
        // payer, model and efforts, and the account new tasks go to.
        assert!(allows(&Method::PATCH, "/v1/accounts/abc"));
        assert!(allows(&Method::PUT, "/v1/settings/jobs"));
        // Accounts are still added, removed and signed in on the machine itself: a paired phone's
        // token could otherwise sign obid into someone else's account (docs/runner-api.md,
        // "POST /v1/accounts").
        assert!(!allows(&Method::POST, "/v1/accounts"));
        assert!(!allows(&Method::DELETE, "/v1/accounts/abc"));
        assert!(!allows(&Method::POST, "/v1/accounts/abc/reserve/x"));
        assert!(!allows(&Method::GET, "/v1/accounts/abc/sign-in"));
        assert!(!allows(&Method::POST, "/v1/accounts/abc/sign-in"));
        // CHATGPT-24: a yes for work nobody's watching only from this Mac's app.
        assert!(!allows(&Method::POST, "/v1/accounts/abc/background"));
        assert!(!allows(&Method::GET, "/v1/jobs/abc/def/wait"));
        // The Swift runner's routes, which obid never served (docs/runner-api.md, "Routes obid
        // does not serve"): refused at the gateway. The models' reads stay.
        assert!(!allows(&Method::POST, "/v1/jobs/abc/proposals/0/file"));
        assert!(!allows(&Method::POST, "/v1/jobs/abc/start-when"));
        assert!(!allows(&Method::PUT, "/v1/settings/models"));
        assert!(!allows(&Method::POST, "/v1/activities"));
        assert!(!allows(&Method::GET, "/v1/watches"));
        assert!(!allows(&Method::POST, "/v1/watches"));
        assert!(!allows(&Method::DELETE, "/v1/watches/abc"));
        assert!(allows(&Method::GET, "/v1/settings/models"));
        assert!(allows(&Method::GET, "/v1/settings/models/options"));
        for (method, path) in [
            (Method::GET, "/v1/connections"),
            (Method::POST, "/v1/pairing/invite"),
            (Method::GET, "/v1/pairing/devices"),
            (Method::DELETE, "/v1/pairing/devices/abc"),
            (Method::GET, "/v1/settings/jobs/x"),
            (Method::PATCH, "/v1/settings/jobs"),
            (Method::GET, "/v1/checks/next"),
            (Method::POST, "/v1/agent/mcp/abc"),
            (Method::POST, "/v1/agent/chatgpt/abc/responses"),
            (Method::GET, "/v1/agent/chatgpt/abc/models"),
            (Method::GET, "/v1/settings/chatgpt"),
            (Method::PUT, "/v1/settings/chatgpt"),
            (Method::GET, "/v1/obid/plugins"),
            (Method::GET, "/app/"),
        ] {
            assert!(!allows(&method, path), "{method} {path}");
        }
    }

    /// IOSNEW-4: a phone reads who's in the Obysphere and changes nothing about it.
    #[test]
    fn iosnew_4_a_phone_reads_the_sphere_and_changes_nothing() {
        assert!(allows(&Method::GET, "/v1/obid/sphere"));
        assert!(allows(&Method::GET, "/v1/obid/sphere/members"));
        assert!(allows(&Method::GET, "/v1/obid/sphere/peers/ernesto"));
        for (method, path) in [
            (Method::POST, "/v1/obid/sphere/peers/ernesto"),
            // No phone asks from a thread yet: SPH-13 and SPH-14 add it, with their signing.
            (Method::POST, "/v1/obid/sphere/peers/ernesto/ask"),
            (Method::DELETE, "/v1/obid/sphere"),
            (Method::POST, "/v1/obid/sphere/join"),
            (Method::POST, "/v1/obid/sphere/key"),
            (Method::POST, "/v1/obid/sphere/card"),
            (Method::GET, "/v1/obid/sphere/pins"),
            (Method::POST, "/v1/obid/sphere/pins/ernesto"),
        ] {
            assert!(!allows(&method, path), "{method} {path}");
        }
    }

    /// The phone shows artifacts: it lists and reads them, any file at any depth, and changes
    /// nothing.
    #[test]
    fn the_gateway_reads_artifacts_and_only_reads_them() {
        for path in [
            "/v1/artifacts",
            "/v1/artifacts/abc",
            "/v1/artifacts/abc/1/index.html",
            "/v1/artifacts/abc/1/assets/css/site.css",
        ] {
            assert!(allows(&Method::GET, path), "{path}");
        }
        for (method, path) in [
            (Method::PATCH, "/v1/artifacts/abc"),
            (Method::DELETE, "/v1/artifacts/abc"),
            (Method::POST, "/v1/artifacts"),
            (Method::GET, "/v1/artifacts/abc/1"),
            (Method::PUT, "/v1/artifacts/abc/1/index.html"),
        ] {
            assert!(!allows(&method, path), "{method} {path}");
        }
        // `**` is the rest of the path, at the end of a pattern only.
        assert!(!allows(&Method::GET, "/v1/jobs/abc/def/wait"));
    }

    /// ARTIFACTS-9: the phone may export a version; it never imports (that is the Mac's, signed).
    #[test]
    fn artifacts_9_the_phone_exports_and_never_imports() {
        assert!(allows(&Method::GET, "/v1/artifacts/abc/export/1"));
        assert!(!allows(&Method::POST, "/v1/artifacts/import"));
    }

    #[test]
    fn paired_devices_can_read_and_answer_limit_decisions_only() {
        assert!(allows(&Method::GET, "/v1/limit-decisions"));
        assert!(allows(&Method::POST, "/v1/limit-decisions/abc/answer"));
        assert!(!allows(&Method::POST, "/v1/limit-decisions/abc/def/answer"));
        assert!(!allows(&Method::DELETE, "/v1/limit-decisions/abc"));
        // ALERTS-1's list is the Mac's, on the loopback only.
        assert!(!allows(&Method::GET, "/v1/alerts"));
    }

    /// Ports GatewayTests.pairingSecretAndCodeWorkOnceAndExpire.
    #[test]
    fn an_invitation_works_once_by_its_secret_or_its_code_and_expires() {
        let home = folder("once");
        let gateway = Gateway::open(&home);
        let now = SystemTime::now();
        let invitation = gateway.invite(
            "https://mac.example",
            "Mac",
            "mac.example",
            now,
            None,
            false,
        );
        assert_eq!(invitation.code.len(), 6);
        assert!(invitation.secret.len() >= 32);
        let redeemed = gateway
            .redeem(&invitation.secret, "Phone", now, None)
            .unwrap();
        assert!(gateway.accepts(&redeemed.token, now));
        assert_eq!(
            gateway
                .redeem(&invitation.code, "Phone", now, None)
                .unwrap_err()
                .0,
            StatusCode::GONE
        );
        let invitation = gateway.invite(
            "https://mac.example",
            "Mac",
            "mac.example",
            now,
            None,
            false,
        );
        let later = now + INVITATION + Duration::from_secs(1);
        assert_eq!(
            gateway
                .redeem(&invitation.code, "Phone", later, None)
                .unwrap_err()
                .0,
            StatusCode::GONE
        );
        // A token stops working at the end of its life.
        assert!(!gateway.accepts(&redeemed.token, now + LIFETIME + Duration::from_secs(1)));
        // Only its digest is kept.
        let kept = std::fs::read_to_string(home.join("gateway-devices.json")).unwrap();
        assert!(!kept.contains(&redeemed.token));
        let _ = std::fs::remove_dir_all(home);
    }

    /// PAIRING-6: a device used with under 15 days left gets 30 more, kept on disk; one left
    /// unused still expires at day 30, and Revoke still ends it at once.
    #[test]
    fn pairing_6_a_used_device_stays_paired() {
        let home = folder("sliding");
        let gateway = Gateway::open(&home);
        let now = SystemTime::now();
        let day = Duration::from_secs(24 * 60 * 60);
        let pair = |name: &str| {
            let invitation = gateway.invite(
                "https://mac.example",
                "Mac",
                "mac.example",
                now,
                None,
                false,
            );
            gateway.redeem(&invitation.secret, name, now, None).unwrap()
        };
        let used = pair("Used");
        let idle = pair("Idle");
        let listened = pair("Listened");
        // Day 10: 20 days left, nothing changes.
        assert!(gateway.accepts(&used.token, now + 10 * day));
        let kept = |token: &str| {
            let devices: HashMap<String, Device> =
                serde_json::from_slice(&std::fs::read(home.join("gateway-devices.json")).unwrap())
                    .unwrap();
            devices[&digest(token)].expires_at
        };
        assert_eq!(kept(&used.token), seconds(now + LIFETIME));
        // Day 16: under 15 left, so it slides to day 46, on disk too (a restart keeps it).
        assert!(gateway.accepts(&used.token, now + 16 * day));
        assert_eq!(kept(&used.token), seconds(now + 16 * day + LIFETIME));
        assert!(Gateway::open(&home).accepts(&used.token, now + 45 * day));
        // `device_via`, the per-request lookup of the gateway and the relay, slides too.
        assert_eq!(
            gateway.device_via(&listened.token, now + 16 * day, None),
            Some(Ok(listened.device_id.clone()))
        );
        assert!(gateway.accepts(&listened.token, now + 45 * day));
        // A request the wrong way (a listener-paired token through a relay) is refused and
        // doesn't extend it.
        let wrong = pair("Wrong");
        assert_eq!(
            gateway.device_via(&wrong.token, now + 16 * day, Some("relay")),
            Some(Err(()))
        );
        assert_eq!(kept(&wrong.token), seconds(now + LIFETIME));
        assert!(!gateway.accepts(&wrong.token, now + LIFETIME + Duration::from_secs(1)));
        // A device paired through the Obysphere: sent unsealed to the listener it's refused and
        // not extended; on its own relay token's socket it slides.
        let relay = |name: &str| {
            let invitation = gateway.invite(
                "https://sphere.example",
                "Mac",
                "mac.example",
                now,
                Some(RelayInvite {
                    sphere: "https://sphere.example".into(),
                    relay: "relay-id".into(),
                    token: format!("relay token of {name}"),
                    key: "key".into(),
                }),
                false,
            );
            let via = digest(&format!("relay token of {name}"));
            let redeemed = gateway
                .redeem(&invitation.secret, name, now, Some(&via))
                .unwrap();
            (redeemed, via)
        };
        let (sealed, via) = relay("Sealed");
        assert_eq!(
            gateway.device_via(&sealed.token, now + 16 * day, None),
            Some(Err(()))
        );
        assert_eq!(kept(&sealed.token), seconds(now + LIFETIME));
        assert_eq!(
            gateway.device_via(&sealed.token, now + 16 * day, Some(&via)),
            Some(Ok(sealed.device_id.clone()))
        );
        assert_eq!(kept(&sealed.token), seconds(now + 16 * day + LIFETIME));
        // The unused one still ends at day 30.
        assert!(!gateway.accepts(&idle.token, now + LIFETIME + Duration::from_secs(1)));
        // Revoke stays instant.
        assert!(gateway.revoke(&used.device_id).unwrap());
        assert!(!gateway.accepts(&used.token, now + 17 * day));
        let _ = std::fs::remove_dir_all(home);
    }

    /// PAIRING-7: a phone paired on the listener (local network, Remote access, the tailnet)
    /// stops at 90 days from pairing though it was used the day before, and Paired devices says
    /// so; one paired through the Obysphere keeps sliding.
    #[test]
    fn pairing_7_a_listener_paired_phone_pairs_again_after_90_days() {
        let home = folder("ninety");
        let gateway = Gateway::open(&home);
        let now = SystemTime::now();
        let day = Duration::from_secs(24 * 60 * 60);
        let invitation = gateway.invite(
            "https://mac.example",
            "Mac",
            "mac.example",
            now,
            None,
            false,
        );
        let lan = gateway
            .redeem(&invitation.secret, "LAN", now, None)
            .unwrap();
        let invitation = gateway.invite(
            "https://sphere.example",
            "Mac",
            "mac.example",
            now,
            Some(RelayInvite {
                sphere: "https://sphere.example".into(),
                relay: "relay-id".into(),
                token: "relay token".into(),
                key: "key".into(),
            }),
            false,
        );
        let via = digest("relay token");
        let sphere = gateway
            .redeem(&invitation.secret, "Sphere", now, Some(&via))
            .unwrap();
        // Both used every 16 days, the last time on day 89.
        for used in [16, 32, 48, 64, 80, 89] {
            assert!(gateway.accepts(&lan.token, now + used * day), "day {used}");
            assert_eq!(
                gateway.device_via(&sphere.token, now + used * day, Some(&via)),
                Some(Ok(sphere.device_id.clone()))
            );
        }
        // Paired devices shows the true end: day 90 for the LAN phone, not day 80 + 30.
        let cap = obi_protocol::time::format(seconds(now) + PAIRED.as_secs() as i64);
        let listed = gateway.list();
        let shown = |id: &str| {
            listed
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["id"] == id)
                .unwrap()["expiresAt"]
                .clone()
        };
        assert_eq!(shown(&lan.device_id), cap);
        assert_ne!(shown(&sphere.device_id), cap);
        // Day 90 and a second: the LAN phone is refused, everywhere; the Obysphere's isn't.
        let past = now + PAIRED + Duration::from_secs(1);
        assert!(!gateway.accepts(&lan.token, past));
        assert_eq!(gateway.device_via(&lan.token, past, None), None);
        assert!(!gateway.paired(past).contains(&lan.device_id));
        assert_eq!(
            gateway.device_via(&sphere.token, past, Some(&via)),
            Some(Ok(sphere.device_id.clone()))
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// PAIRING-7 for a record that slid past the 90 days before the limit: capped when read.
    #[test]
    fn pairing_7_a_record_that_slid_past_90_days_is_capped() {
        let home = folder("ninety-old");
        let now = SystemTime::now();
        let day = Duration::from_secs(24 * 60 * 60);
        let created = seconds(now) - 100 * day.as_secs() as i64;
        let device = Device {
            id: "old".into(),
            name: "Old".into(),
            created,
            expires_at: seconds(now + 20 * day),
            relay: None,
            keyable_until: None,
            computer: false,
        };
        std::fs::write(
            home.join("gateway-devices.json"),
            serde_json::to_vec(&HashMap::from([(digest("old token"), device)])).unwrap(),
        )
        .unwrap();
        let gateway = Gateway::open(&home);
        assert!(!gateway.accepts("old token", now));
        assert_eq!(
            gateway.list()[0]["expiresAt"],
            obi_protocol::time::format(created + PAIRED.as_secs() as i64)
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// Only an invitation the owner made on their own device lets the phone it pairs add its
    /// publishing key, for 10 minutes after redeeming it; one the API token made alone never does.
    #[test]
    fn only_an_owned_invitation_lets_its_phone_add_a_key_for_ten_minutes() {
        let home = folder("keyable");
        let gateway = Gateway::open(&home);
        let now = SystemTime::now();
        let token_only = gateway.invite(
            "https://mac.example",
            "Mac",
            "mac.example",
            now,
            None,
            false,
        );
        let plain = gateway
            .redeem(&token_only.secret, "Phone", now, None)
            .unwrap();
        assert!(!gateway.may_add_key(&plain.device_id, now));
        let owned = gateway.invite("https://mac.example", "Mac", "mac.example", now, None, true);
        let phone = gateway.redeem(&owned.secret, "Phone", now, None).unwrap();
        assert!(gateway.may_add_key(&phone.device_id, now));
        assert!(gateway.may_add_key(&phone.device_id, now + KEYABLE - Duration::from_secs(1)));
        assert!(!gateway.may_add_key(&phone.device_id, now + KEYABLE + Duration::from_secs(1)));
        assert!(!gateway.may_add_key("someone-else", now));
        // Kept with the pairing, across a restart.
        assert!(Gateway::open(&home).may_add_key(&phone.device_id, now));
        let _ = std::fs::remove_dir_all(home);
    }

    /// Ports GatewayTests.wrongPairingCredentialsExhaustTheInvitation.
    #[test]
    fn five_wrong_credentials_use_the_invitation_up() {
        let home = folder("wrong");
        let gateway = Gateway::open(&home);
        let now = SystemTime::now();
        let invitation = gateway.invite(
            "https://mac.example",
            "Mac",
            "mac.example",
            now,
            None,
            false,
        );
        for _ in 0..ATTEMPTS {
            assert_eq!(
                gateway.redeem("not it", "Phone", now, None).unwrap_err().0,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            gateway
                .redeem(&invitation.code, "Phone", now, None)
                .unwrap_err()
                .0,
            StatusCode::GONE
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// Ports GatewayTests.gatewayTokenNamesPersistAndTokensCanBeRevoked.
    #[test]
    fn paired_devices_last_across_restarts_and_can_be_revoked() {
        let home = folder("revoke");
        let now = SystemTime::now();
        let token = {
            let gateway = Gateway::open(&home);
            let invitation = gateway.invite(
                "https://mac.example",
                "Mac",
                "mac.example",
                now,
                None,
                false,
            );
            gateway
                .redeem(&invitation.code, "Alice's iPhone", now, None)
                .unwrap()
                .token
        };
        let gateway = Gateway::open(&home);
        assert!(gateway.accepts(&token, now));
        let listed = gateway.list();
        assert_eq!(listed[0]["name"], "Alice's iPhone");
        let id = listed[0]["id"].as_str().unwrap().to_owned();
        assert!(gateway.revoke(&id).unwrap());
        assert!(!gateway.revoke(&id).unwrap());
        assert!(!Gateway::open(&home).accepts(&token, now));
        let _ = std::fs::remove_dir_all(home);
    }

    /// Ports GatewayTests.gatewayScrubsSessionIDsAndAccountHomes, with its own answers.
    #[test]
    fn answers_lose_session_ids_and_account_homes() {
        let home = "/Users/alice/Library/Application Support/Oby/accounts/work";
        let session = "e6a24229-1bd5-48c4-85c5-43b33d433775";
        let job = format!(
            r#"{{"id":"j1","session":"{session}","step":"Signed in at {home}/.codex","account":{{"id":"work","home":"{home}"}}}}"#
        );
        let scrubber = Scrubber {
            homes: vec![home.into()],
        };
        let body = scrubber.document(&format!("[{job}]"));
        assert!(!body.contains(session) && !body.contains(home), "{body}");
        let jobs: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(jobs[0]["id"], "j1");
        assert!(jobs[0]["session"].is_null());
        assert_eq!(jobs[0]["step"], "Signed in at [home]/.codex");
        assert_eq!(jobs[0]["account"]["home"], "[home]");

        let frames = scrubber.frame(&format!(
            "event: job\ndata: {job}\n\nevent: step\ndata: \"in {home}\"\n\n"
        ));
        assert!(
            !frames.contains(session) && !frames.contains(home),
            "{frames}"
        );
        assert!(frames.starts_with("event: job\ndata: {"), "{frames}");
        assert!(frames.contains("data: \"in [home]\""), "{frames}");

        // A clean answer goes as it came, byte for byte.
        let clean = r#"{"b": 1,  "a": "x"}"#;
        assert_eq!(scrubber.document(clean), clean);
    }

    /// LANES-9: an own job's resume command, which names its session, never reaches the phone.
    #[test]
    fn lanes_9_the_phone_never_gets_a_resume_command() {
        let mut job = crate::job::Job::new(
            "j1".into(),
            "obi-ai-bot/oby".into(),
            "build".into(),
            "Add the line".into(),
            "claude".into(),
            "default".into(),
        );
        let session = "e6a24229-1bd5-48c4-85c5-43b33d433775";
        job.session = Some(session.into());
        job.worktree = Some("/Users/alice/worktrees/j1".into());
        job.own_lane = true;
        let job = job.to_json();
        assert!(job["resumeCommand"].is_string(), "{job}");
        let scrubber = Scrubber { homes: Vec::new() };
        let body = scrubber.document(&job.to_string());
        assert!(!body.contains(session), "{body}");
        let phone: Value = serde_json::from_str(&body).unwrap();
        assert!(phone["resumeCommand"].is_null(), "{body}");
        let frames = scrubber.frame(&format!("event: job\ndata: {job}\n\n"));
        assert!(!frames.contains("claude --resume"), "{frames}");
    }

    #[test]
    fn computers_13_a_computers_invitation_lasts_ten_minutes_and_a_phones_doesnt_replace_it() {
        let home = folder("computer-invite");
        let gateway = Gateway::open(&home);
        let now = SystemTime::now();
        let minute = Duration::from_secs(60);
        let url = "https://10.0.0.2:8443";
        // A phone's invitation made after the computer's leaves it open, and both pair.
        let computer = gateway.invite_computer(url, "Studio", "10.0.0.2", now, true, None);
        assert!(computer.relay.is_none());
        let phone = gateway.invite(url, "Mac", "mac", now, None, false);
        assert!(
            gateway
                .redeem(&phone.code, "iPhone", now + minute, None)
                .is_ok()
        );
        // The computer's is still there after nine minutes, and its computer may add a key.
        let paired = gateway
            .redeem(&computer.code, "Studio Mac", now + 9 * minute, None)
            .unwrap();
        assert!(gateway.may_add_key(&paired.device_id, now + 9 * minute));
        let again = gateway.redeem(&computer.code, "again", now + 9 * minute, None);
        assert_eq!(again.unwrap_err().0, StatusCode::GONE, "once");
        // Made without the owner, it pairs one that can't add a key.
        let plain = gateway.invite_computer(url, "Studio", "10.0.0.2", now, false, None);
        let paired = gateway.redeem(&plain.code, "Laptop", now, None).unwrap();
        assert!(!gateway.may_add_key(&paired.device_id, now));
        // After ten minutes it's gone.
        let late = gateway.invite_computer(url, "Studio", "10.0.0.2", now, false, None);
        let gone = gateway.redeem(&late.code, "x", now + 11 * minute, None);
        assert_eq!(gone.unwrap_err().0, StatusCode::GONE);
        // A wrong try counts against it; the fifth ends it.
        let tried = gateway.invite_computer(url, "Studio", "10.0.0.2", now, false, None);
        for _ in 0..ATTEMPTS {
            let wrong = gateway.redeem("not-the-code", "x", now, None);
            assert_eq!(wrong.unwrap_err().0, StatusCode::UNAUTHORIZED);
        }
        assert_eq!(
            gateway.redeem(&tried.code, "x", now, None).unwrap_err().0,
            StatusCode::GONE
        );
        // Never through a relay.
        let sealed = gateway.invite_computer(url, "Studio", "10.0.0.2", now, false, None);
        assert!(
            gateway
                .redeem(&sealed.code, "x", now, Some("relay"))
                .is_err()
        );
        assert!(gateway.redeem(&sealed.code, "x", now, None).is_ok());
    }

    #[test]
    fn computers_14_a_path_through_the_gateway_names_one_route_as_written() {
        for (path, plain) in [
            ("/v1/status", "/v1/status"),
            (
                "/v1/inbox/listen%2Dreview-1/answer",
                "/v1/inbox/listen-review-1/answer",
            ),
            (
                "/v1/artifacts/abc/1/my%20notes.md",
                "/v1/artifacts/abc/1/my notes.md",
            ),
            ("/v1/%70airing/devices", "/v1/pairing/devices"),
        ] {
            assert_eq!(plain_path(path).as_deref(), Some(plain), "{path}");
        }
        assert!(!allows(
            &Method::GET,
            &plain_path("/v1/%70airing/devices").unwrap()
        ));
        // What obid is sent: that path, encoded the one standard way, whatever the request's spelling.
        for (path, sent) in [
            ("/v1/jobs/abc/rep%6Frt", "/v1/jobs/abc/report"),
            ("/v1/jobs/abc/report", "/v1/jobs/abc/report"),
            (
                "/v1/inbox/listen%2Dreview-1/answer",
                "/v1/inbox/listen-review-1/answer",
            ),
            (
                "/v1/artifacts/abc/1/my%20notes.md",
                "/v1/artifacts/abc/1/my%20notes.md",
            ),
            (
                "/v1/artifacts/abc/1/a+b(1).md",
                "/v1/artifacts/abc/1/a%2Bb%281%29.md",
            ),
            (
                "/v1/artifacts/abc/1/caf%c3%a9",
                "/v1/artifacts/abc/1/caf%C3%A9",
            ),
        ] {
            assert_eq!(canonical(&plain_path(path).unwrap()), sent, "{path}");
        }
        for path in [
            "/v1/./pairing/devices",
            "//v1/pairing/devices",
            "/v1/pairing/../pairing/devices",
            "/v1/pairing/devices/",
            "/v1\\pairing",
            "v1/status",
            "/v1/jobs/%2e%2e/report",
            "/v1/jobs/%2E/report",
            "/v1/artifacts/abc/1/a%2Fb",
            "/v1/artifacts/abc/1/a%5Cb",
            "/v1/artifacts/abc/1/a%252e",
            "/v1/artifacts/abc/1/a%00",
            "/v1/artifacts/abc/1/%ff",
            "/v1/artifacts/abc/1/%2",
        ] {
            assert_eq!(plain_path(path), None, "{path}");
        }
        // A computer's token reaches the gateway's list and nothing past it.
        assert!(!allows(&Method::GET, "/v1/obid/vault"));
        assert!(!allows(&Method::PUT, "/v1/obid/vault/github"));
        assert!(!allows(&Method::GET, "/v1/pairing/devices"));
        assert!(!allows(&Method::POST, "/v1/repos/setup"));
    }

    #[test]
    fn computers_14_a_computers_own_routes_are_named_each_with_its_screen() {
        for (method, path, screen) in COMPUTER_ROUTES {
            let method = Method::from_bytes(method.as_bytes()).unwrap();
            assert!(allows_computer(&method, path), "{method} {path}");
            assert!(!allows(&method, path), "{path} isn't a phone's too");
            assert!(!screen.is_empty(), "{path} names its screen");
        }
        assert!(allows_computer(&Method::GET, "/v1/skills"));
        assert!(!allows_computer(&Method::POST, "/v1/skills"));
        assert!(!allows_computer(&Method::GET, "/v1/obid/vault"));
        assert!(allows_computer(&Method::GET, "/v1/status"), "a phone's too");
    }

    #[test]
    fn computers_15_a_computers_invitation_names_its_key_only_when_owned() {
        let home = folder("computer-key");
        let gateway = Gateway::open(&home);
        let now = SystemTime::now();
        let url = "http://127.0.0.1:7778";
        let owned = gateway.invite_computer(url, "Studio", "h", now, true, Some("k1".into()));
        let paired = gateway
            .redeem(&owned.code, "Studio Mac", now, None)
            .unwrap();
        assert_eq!(paired.key.as_deref(), Some("k1"));
        assert!(gateway.is_computer(&paired.device_id));
        assert!(gateway.list()[0]["computer"] == true);
        let plain = gateway.invite_computer(url, "Studio", "h", now, false, Some("k2".into()));
        let paired = gateway.redeem(&plain.code, "Laptop", now, None).unwrap();
        assert_eq!(paired.key, None, "not without the owner");
        let phone = gateway.invite(url, "Mac", "mac", now, None, true);
        let phone = gateway.redeem(&phone.code, "iPhone", now, None).unwrap();
        assert!(!gateway.is_computer(&phone.device_id));
        assert!(serde_json::to_string(&phone).unwrap().find("key").is_none());
    }
}
