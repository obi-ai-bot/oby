//! A page's `localhost` on another of the owner's computers (docs/contracts/remote.md, COMPUTERS-26
//! to 28; board 14, "Its localhost is devbox's, not this Mac's"), ported from T3 Code's
//! `apps/server/src/preview/PreviewTunnel.ts` and `apps/desktop/src/preview/LoopbackTunnel.ts`
//! (pingdotgg/t3code@1d028248): one WebSocket per TCP connection, raw bytes both ways, to that
//! computer's own loopback and nowhere else.
//!
//! - That computer's end ([`serve`]): `GET /v1/preview/tunnel?port=<n>` on its gateway, for a
//!   paired computer's own device token only. A bad port, one of obid's own ([`tunnel_port`]) or
//!   one nothing listens on is refused before the upgrade; the dial is to `127.0.0.1` or `[::1]`
//!   only, and the tunnel ends with its pairing.
//! - This Mac's end ([`carrier`]): what a computer's door (crate::egress, `Kind::Computer`) hands
//!   each connection it let through, carried to that computer's end over its best route with its
//!   device token, and never dialed on this Mac.
//!
//! It owns nothing: the doors are crate::remote::computers', the pairings crate::gateway's. No
//! secret scan: the bytes go only between the owner's own paired computers, over a pinned or SSH
//! route, as a forwarded request's answers do (COMPUTERS-22).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Weak};
use std::time::SystemTime;

use axum::body::Bytes;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{FromRequestParts as _, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use futures_util::{Sink, SinkExt as _, Stream, StreamExt as _};
use serde_json::json;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

use super::reach::Base;
use crate::api::Shared;
use crate::egress::{Carried, Carrier};

/// The most read off a socket at once, each read one binary frame.
const READ: usize = 64 * 1024;
/// The most a tunnel's WebSocket holds unsent, and the largest message and frame it takes.
const MOST: usize = 1024 * 1024;

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

/// Why a port isn't tunnelled to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    BadPort,
    OwnPort,
}

/// The port a tunnel may dial: 1 to 65535, and none of obid's own (`own`).
pub fn tunnel_port(asked: u32, own: &[u16]) -> Result<u16, Refused> {
    let Ok(port) = u16::try_from(asked) else {
        return Err(Refused::BadPort);
    };
    if port == 0 {
        return Err(Refused::BadPort);
    }
    if own.contains(&port) {
        return Err(Refused::OwnPort);
    }
    Ok(port)
}

fn refusal(status: StatusCode, message: &str, reason: &'static str) -> Response {
    (
        status,
        axum::Json(json!({"error": message, "reason": reason})),
    )
        .into_response()
}

/// The paired computer a tunnel's request comes from: its own device token, alone, on the
/// gateway's listener or a `direct` one; never through the relay, nor one naming a computer.
fn caller(shared: &Shared, parts: &Parts) -> Option<String> {
    if parts.extensions.get::<crate::gateway::Relayed>().is_some()
        || parts.headers.contains_key(super::computers::HEADER)
    {
        return None;
    }
    let token = crate::gateway::bearer(&parts.headers)?;
    match shared.gateway.device_via(token, SystemTime::now(), None)? {
        Ok(id) if shared.gateway.is_computer(&id) => Some(id),
        _ => None,
    }
}

/// The query's one `port`, digits only: none for anything else.
fn asked_port(query: Option<&str>) -> Option<u32> {
    let mut ports = query
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| pair.strip_prefix("port="));
    let port = ports.next()?;
    let digits = !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit());
    if !digits || ports.next().is_some() {
        return None;
    }
    port.parse().ok()
}

/// `GET /v1/preview/tunnel?port=<n>`, on the gateway (COMPUTERS-26): the caller first, so nobody
/// else learns anything (`404`, as an unlisted route); then the port, the upgrade and the dial,
/// each refused before the upgrade. Then the socket and the WebSocket are pumped both ways until
/// either ends or the computer's pairing does.
pub async fn serve(shared: &Arc<Shared>, request: Request) -> Response {
    let (mut parts, _) = request.into_parts();
    let Some(device) = caller(shared, &parts) else {
        return crate::gateway::error(
            StatusCode::NOT_FOUND,
            "That route isn't available through the gateway.",
        );
    };
    let checked = asked_port(parts.uri.query())
        .ok_or(Refused::BadPort)
        .and_then(|asked| tunnel_port(asked, shared.gateway.own_ports()));
    let port = match checked {
        Ok(port) => port,
        Err(Refused::BadPort) => {
            return refusal(
                StatusCode::BAD_REQUEST,
                "A tunnel takes a port from 1 to 65535: ?port=<n>.",
                "bad_port",
            );
        }
        Err(Refused::OwnPort) => {
            return refusal(
                StatusCode::FORBIDDEN,
                "That port is obid's own, which a tunnel never reaches.",
                "own_port",
            );
        }
    };
    let Ok(upgrade) = WebSocketUpgrade::from_request_parts(&mut parts, &()).await else {
        return refusal(
            StatusCode::BAD_REQUEST,
            "A tunnel is a WebSocket.",
            "not_a_websocket",
        );
    };
    // Before the upgrade, so a dead port fails the handshake instead of an open socket; 403, not
    // 502, so a proxy between can't swap it (T3's).
    let Some(socket) = dial(port).await else {
        return refusal(
            StatusCode::FORBIDDEN,
            "Nothing listens on that port here.",
            "nothing_listens",
        );
    };
    tracing::debug!("preview tunnel: computer {device} reaches port {port}");
    let shared = shared.clone();
    upgrade
        .max_write_buffer_size(MOST)
        .max_message_size(MOST)
        .max_frame_size(MOST)
        .on_upgrade(move |ws| async move {
            let unpaired = unpaired(&shared.gateway, &device);
            let binary = axum::extract::ws::Message::Binary;
            pump(ws, socket, None, unpaired, binary, heard_here).await
        })
}

/// This computer's own `127.0.0.1:<port>`, else `[::1]:<port>`, both within
/// [`super::reach::CONNECT`]: never any other address.
async fn dial(port: u16) -> Option<TcpStream> {
    let tries = async {
        for ip in [
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
        ] {
            if let Ok(socket) = TcpStream::connect(SocketAddr::new(ip, port)).await {
                return Some(socket);
            }
        }
        None
    };
    tokio::time::timeout(super::reach::CONNECT, tries)
        .await
        .ok()
        .flatten()
}

/// Once the paired device `device` is no longer paired here: revoked (`DELETE /v1/pairing`, or
/// revoked on this computer), or its token's time is up, checked again whenever either comes.
async fn unpaired(gateway: &crate::gateway::Gateway, device: &str) {
    let mut revoked = gateway.revocations();
    loop {
        let left = gateway
            .paired_until(device)
            .and_then(|until| until.duration_since(SystemTime::now()).ok());
        let Some(left) = left else { return };
        tokio::select! {
            changed = revoked.changed() => if changed.is_err() {
                return;
            },
            () = tokio::time::sleep(left) => {}
        }
    }
}

/// Once this Mac no longer keeps computer `id` (`DELETE /v1/computers/{id}`).
async fn forgotten(shared: Arc<Shared>, id: String) {
    let mut changes = shared.computers.changes();
    while shared.computers.get(&id).is_some() {
        if changes.changed().await.is_err() {
            return;
        }
    }
}

/// What a WebSocket message is to a tunnel: bytes for its socket, nothing (text, a ping), or its
/// end.
enum Heard {
    Bytes(Bytes),
    Nothing,
    End,
}

/// A message on this computer's end of a tunnel.
fn heard_here(message: axum::extract::ws::Message) -> Heard {
    match message {
        axum::extract::ws::Message::Binary(bytes) => Heard::Bytes(bytes),
        axum::extract::ws::Message::Close(_) => Heard::End,
        _ => Heard::Nothing,
    }
}

/// A message on the Mac's end of a tunnel.
fn heard_there(message: tungstenite::Message) -> Heard {
    match message {
        tungstenite::Message::Binary(bytes) => Heard::Bytes(bytes),
        tungstenite::Message::Close(_) => Heard::End,
        _ => Heard::Nothing,
    }
}

/// The socket and the WebSocket both ways ([`both_ways`]) until either closes or fails, then both
/// end; or until `ended`, which ends it whatever it waits on, a write or a close the other side
/// doesn't take included: then both are dropped at once, the socket reset, so nothing still queued
/// goes on.
///
/// ponytail: no idle limit on a tunnel, as T3's has none: it lasts while both sides hold it, an
/// idle one holding a socket at each end and one of its door's 64; the egress proxy's `IDLE` is
/// the upgrade.
async fn pump<W, M, E>(
    mut ws: W,
    mut socket: TcpStream,
    rest: Option<u64>,
    ended: impl Future<Output = ()>,
    binary: fn(Bytes) -> M,
    heard: fn(M) -> Heard,
) where
    W: Sink<M> + Stream<Item = Result<M, E>> + Unpin,
{
    // The graceful close races the end too: a peer that stops reading can hold it.
    let cut = tokio::select! {
        () = ended => true,
        () = async {
            both_ways(&mut ws, &mut socket, rest, binary, heard).await;
            let _ = ws.close().await;
            let _ = socket.shutdown().await;
        } => false,
    };
    if cut {
        // Closed with a reset, not a FIN: what its send buffer holds is dropped with it.
        let reset = nix::libc::linger {
            l_onoff: 1,
            l_linger: 0,
        };
        let _ = nix::sys::socket::setsockopt(&socket, nix::sys::socket::sockopt::Linger, &reset);
    }
}

/// The socket and the WebSocket both ways, as binary frames of at most [`READ`], each written
/// before the next read on its side, until either closes or fails. `rest`: only that much more is
/// read off the socket (a plain request's body), then only the answer comes back.
async fn both_ways<W, M, E>(
    ws: &mut W,
    socket: &mut TcpStream,
    mut rest: Option<u64>,
    binary: fn(Bytes) -> M,
    heard: fn(M) -> Heard,
) where
    W: Sink<M> + Stream<Item = Result<M, E>> + Unpin,
{
    let (mut read, mut write) = socket.split();
    let mut buffer = vec![0u8; READ];
    loop {
        let want = rest.map_or(READ, |left| left.min(READ as u64) as usize);
        tokio::select! {
            n = read.read(&mut buffer[..want]), if want > 0 => match n {
                Ok(n) if n > 0 => {
                    if let Some(left) = rest.as_mut() {
                        *left -= n as u64;
                    }
                    let frame = binary(Bytes::copy_from_slice(&buffer[..n]));
                    if ws.send(frame).await.is_err() {
                        break;
                    }
                }
                _ => break,
            },
            message = ws.next() => match message.map(|m| m.map(heard)) {
                Some(Ok(Heard::Bytes(bytes))) => {
                    if write.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Heard::Nothing)) => {}
                _ => break,
            },
        }
    }
}

/// What a computer's door carries its connections with (crate::egress::Egress::start_computers,
/// COMPUTERS-27): each to its computer, over a WebSocket of its own. It holds obid's state
/// weakly, as the jobs' proxy holds its jobs, so neither keeps the other.
///
/// ponytail: an authenticated egress door, not T3's SOCKS5 listener in main: Chromium can't
/// authenticate a SOCKS proxy, and on macOS any local process, Oby's box included, could ride one.
/// Its ceiling is a client that speaks only SOCKS, which gets nothing here; a SOCKS front on this
/// same door is the upgrade.
///
/// ponytail: no ticket per connection, as T3's renderer mints one: obid holds the device token and
/// sends it itself, so no credential leaves obid. Its ceiling is every tunnel riding the pairing's
/// one token; a ticket is the upgrade once anything but obid opens tunnels.
pub fn carrier(shared: Weak<Shared>) -> Carrier {
    Arc::new(move |carried| {
        let shared = shared.clone();
        Box::pin(async move {
            if let Some(shared) = shared.upgrade() {
                carry(shared, carried).await;
            }
        })
    })
}

/// One connection a computer's door let through, to that computer's `port`: a `CONNECT` hears
/// `200` once the WebSocket is open, then its early bytes go and it's pumped; a plain request
/// goes as it was forwarded, its body exactly its length. No route, a refusal there or a cut is a
/// closed connection, a `CONNECT`'s with `502`: it's never dialed on this Mac instead.
async fn carry(shared: Arc<Shared>, carried: Carried) {
    let Carried {
        mut client,
        computer,
        port,
        first,
        rest,
    } = carried;
    let Some(mut ws) = open(&shared, &computer, port).await else {
        if rest.is_none() {
            let why = "Oby couldn't reach that port on its computer.";
            let answer = format!(
                "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{why}",
                why.len()
            );
            let _ = client.write_all(answer.as_bytes()).await;
        }
        return;
    };
    tracing::debug!("preview tunnel: this Mac reaches computer {computer}'s port {port}");
    if rest.is_none()
        && client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .is_err()
    {
        return;
    }
    for chunk in first.chunks(READ) {
        let frame = tungstenite::Message::Binary(Bytes::copy_from_slice(chunk));
        if ws.send(frame).await.is_err() {
            return;
        }
    }
    let ended = forgotten(shared.clone(), computer);
    pump(
        ws,
        client,
        rest,
        ended,
        tungstenite::Message::Binary,
        heard_there,
    )
    .await;
}

/// A WebSocket to computer `id`'s tunnel to its `port`, over the first of its routes that takes
/// it: the one in use and those above it, then the rest, as a forwarded request tries them. A
/// route whose connection, TLS or pin fails gives way to the next; an answer there (a refusal)
/// ends the try. A tunnel tells the supervisor nothing: forwarded requests and `retry` do.
async fn open(shared: &Shared, id: &str, port: u16) -> Option<Ws> {
    let computer = shared.computers.get(id)?;
    let token = shared.computers.token(id).ok().flatten()?;
    // ponytail: forward.rs's route order, copied, because forward.rs is off limits to this task;
    // its ceiling is two copies of one order, and one function both call is the upgrade.
    let mut routes = shared.computers.to_probe(id);
    for route in &computer.routes {
        if !routes.iter().any(|r| r.id == route.id) {
            routes.push(route.clone());
        }
    }
    for route in routes {
        let Ok(base) = super::reach::base(shared, id, &route).await else {
            continue;
        };
        match socket(&base, &token, port).await {
            Ok(ws) => return Some(ws),
            Err(Some(status)) => {
                tracing::debug!("preview tunnel: computer {id} refused port {port} ({status})");
                return None;
            }
            Err(None) => {}
        }
    }
    None
}

/// The WebSocket to `base`'s tunnel to `port`, with the computer's `token`: `wss://` trusting
/// only the route's pin, or `ws://` through an SSH tunnel, within [`super::reach::CONNECT`]. The
/// far end's status when it answered with a refusal; none when it couldn't be reached.
async fn socket(base: &Base, token: &str, port: u16) -> Result<Ws, Option<u16>> {
    let url = match base.url.split_once("://") {
        Some(("https", rest)) => format!("wss://{rest}"),
        Some(("http", rest)) => format!("ws://{rest}"),
        _ => return Err(None),
    };
    let mut request = format!("{url}/v1/preview/tunnel?port={port}")
        .into_client_request()
        .map_err(|_| None)?;
    let bearer = format!("Bearer {token}").parse().map_err(|_| None)?;
    request.headers_mut().insert("Authorization", bearer);
    let connector = match base.pin {
        Some(pin) => Some(tokio_tungstenite::Connector::Rustls(Arc::new(
            super::pinned::tls(pin).map_err(|_| None)?,
        ))),
        None => Some(tokio_tungstenite::Connector::Plain),
    };
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_write_buffer_size(MOST)
        .max_message_size(Some(MOST))
        .max_frame_size(Some(MOST));
    let connect =
        tokio_tungstenite::connect_async_tls_with_config(request, Some(config), true, connector);
    match tokio::time::timeout(super::reach::CONNECT, connect).await {
        Ok(Ok((ws, _))) => Ok(ws),
        Ok(Err(tungstenite::Error::Http(response))) => Err(Some(response.status().as_u16())),
        _ => Err(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// COMPUTERS-26: a tunnel ends once its pairing's token stops working by time alone, with no
    /// revocation to tell it.
    #[tokio::test]
    async fn computers_26_a_tunnel_ends_when_its_pairing_expires() {
        let home =
            std::env::temp_dir().join(format!("obid-tunnel-expiry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let gateway = crate::gateway::Gateway::open(&home);
        // Paired so long ago that its token works two seconds more.
        let then = SystemTime::now() - crate::gateway::LIFETIME + std::time::Duration::from_secs(2);
        let url = "http://127.0.0.1:7778";
        let invited = gateway.invite_computer(url, "Studio", "h", then, true, None);
        let paired = gateway.redeem(&invited.code, "Mac", then, None).unwrap();
        assert!(
            gateway
                .paired(SystemTime::now())
                .contains(&paired.device_id)
        );
        let ended = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            unpaired(&gateway, &paired.device_id),
        );
        assert!(ended.await.is_ok(), "the tunnel outlived its pairing");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A WebSocket peer that ends at once, then stops reading: its close never completes.
    struct Stuck {
        heard: bool,
    }

    impl Stream for Stuck {
        type Item = Result<Bytes, ()>;
        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            match std::mem::replace(&mut self.heard, true) {
                false => std::task::Poll::Ready(Some(Ok(Bytes::new()))),
                true => std::task::Poll::Pending,
            }
        }
    }

    impl Sink<Bytes> for Stuck {
        type Error = ();
        fn poll_ready(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), ()>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn start_send(self: std::pin::Pin<&mut Self>, _: Bytes) -> Result<(), ()> {
            Ok(())
        }
        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), ()>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), ()>> {
            std::task::Poll::Pending
        }
    }

    /// COMPUTERS-26: a tunnel whose WebSocket peer stops reading while it closes still ends when
    /// its pairing does, its socket reset: the service at its end gets nothing more.
    #[tokio::test]
    async fn computers_26_a_tunnel_closing_on_a_peer_that_stopped_reading_ends_when_revoked() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (mut service, _) = listener.accept().await.unwrap();
        let (revoke, revoked) = tokio::sync::oneshot::channel::<()>();
        let ended = async {
            let _ = revoked.await;
        };
        let pumped = tokio::spawn(pump(
            Stuck { heard: false },
            socket,
            None,
            ended,
            |b| b,
            |_| Heard::End,
        ));
        // The peer ended, and its close is stuck: then the pairing ends.
        tokio::task::yield_now().await;
        let _ = revoke.send(());
        let done = tokio::time::timeout(std::time::Duration::from_secs(5), pumped).await;
        assert!(
            done.is_ok(),
            "the tunnel outlived its pairing while closing"
        );
        let mut got = [0u8; 16];
        let read = tokio::time::timeout(std::time::Duration::from_secs(5), service.read(&mut got))
            .await
            .expect("the service's connection is over");
        assert!(matches!(read, Ok(0) | Err(_)), "the service got {read:?}");
    }

    /// COMPUTERS-26: the query's port is one `port` of digits; anything else is a bad port.
    #[test]
    fn computers_26_a_tunnel_reads_one_port_of_digits() {
        assert_eq!(asked_port(Some("port=8765")), Some(8765));
        assert_eq!(asked_port(Some("x=1&port=70000")), Some(70000));
        for query in [
            None,
            Some(""),
            Some("port="),
            Some("port=-1"),
            Some("port=+80"),
            Some("port=8o"),
            Some("port=99999999999"),
            Some("port=80&port=81"),
        ] {
            assert_eq!(asked_port(query), None, "{query:?}");
        }
        assert_eq!(tunnel_port(70000, &[]), Err(Refused::BadPort));
        assert_eq!(tunnel_port(0, &[]), Err(Refused::BadPort));
        assert_eq!(tunnel_port(7777, &[7777, 7778]), Err(Refused::OwnPort));
        assert_eq!(tunnel_port(7778, &[7777, 7778]), Err(Refused::OwnPort));
        assert_eq!(tunnel_port(3000, &[7777, 7778]), Ok(3000));
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proof of the tunnel's port rule (COMPUTERS-26): `kani::any()` stands for every
    //! number a query can name and every three ports of obid's own.

    use super::*;

    /// PROOF-1, COMPUTERS-26: the tunnel takes exactly the ports 1 to 65535 that aren't one of
    /// obid's own, unchanged; past 65535 or 0 is a bad port, one of obid's own an own port.
    #[kani::proof]
    #[kani::unwind(5)]
    fn computers_26_a_tunnel_takes_only_a_real_port_off_obids_own() {
        let asked: u32 = kani::any();
        let own: [u16; 3] = kani::any();
        let expected = if asked == 0 || asked > 65535 {
            Err(Refused::BadPort)
        } else if u32::from(own[0]) == asked
            || u32::from(own[1]) == asked
            || u32::from(own[2]) == asked
        {
            Err(Refused::OwnPort)
        } else {
            Ok(asked as u16)
        };
        assert_eq!(tunnel_port(asked, &own), expected);
    }
}
