//! obid's direct addresses (docs/contracts/remote.md, COMPUTERS-10): the gateway's own router
//! (crate::gateway) served over HTTPS on the addresses the owner picks (`direct` in the
//! configuration), with this computer's own certificate (super::certificate), so another
//! computer's window reaches it over any network they already have, its certificate pinned.
//! Nothing but the gateway is served there: the same pairing, device tokens and allowlist.
//!
//! A handshake runs in a task of its own, at most 10 seconds, so a slow or silent client holds
//! up no other; one that fails is dropped and the listener goes on.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_rustls::server::TlsStream;

use super::certificate::{self, Certificate};

/// How long a client has to finish its handshake.
const HANDSHAKE: Duration = Duration::from_secs(10);

/// The TLS server configuration for `certificate`.
pub fn tls(certificate: &Certificate) -> Result<Arc<rustls::ServerConfig>, String> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(
                certificate.der.clone(),
            )],
            rustls::pki_types::PrivateKeyDer::Pkcs8(certificate.key.clone().into()),
        )
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(config))
}

/// A bound direct address, as axum serves it: each connection already past its handshake.
pub struct Direct {
    local: SocketAddr,
    pin: super::pin::Pin,
    ready: mpsc::Receiver<(TlsStream<TcpStream>, SocketAddr)>,
}

impl Direct {
    /// Takes TCP connections on `listener` and finishes their handshakes with `tls`, until the
    /// `Direct` is dropped.
    pub fn new(
        listener: TcpListener,
        tls: Arc<rustls::ServerConfig>,
        pin: super::pin::Pin,
    ) -> std::io::Result<Direct> {
        let local = listener.local_addr()?;
        let acceptor = tokio_rustls::TlsAcceptor::from(tls);
        let (send, ready) = mpsc::channel(64);
        tokio::spawn(async move {
            loop {
                let (tcp, peer) = tokio::select! {
                    accepted = listener.accept() => match accepted {
                        Ok(accepted) => accepted,
                        // Out of descriptors, or the like: the next try may work.
                        Err(_) => {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            continue;
                        }
                    },
                    () = send.closed() => return,
                };
                let acceptor = acceptor.clone();
                let send = send.clone();
                tokio::spawn(async move {
                    if let Ok(Ok(tls)) = tokio::time::timeout(HANDSHAKE, acceptor.accept(tcp)).await
                    {
                        let _ = send.send((tls, peer)).await;
                    }
                });
            }
        });
        Ok(Direct { local, pin, ready })
    }
}

impl axum::serve::Listener for Direct {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(ready) => ready,
            // The accepting task ended only once this was dropped: nothing more comes.
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.local)
    }
}

/// Binds each of `addresses` with this computer's certificate (made now the first time, in
/// `home/keys`): the listeners bound, and what `GET /v1/status` says of each under
/// `gateway.direct`, `{"address", "state": "listening", "pin"}` or `{"address", "state": "down",
/// "detail"}`. A busy address is a warning, never a stop, as the gateway's is.
pub async fn bind(home: &Path, addresses: &[SocketAddr]) -> (Vec<Direct>, Vec<Value>) {
    if addresses.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let down = |address: &SocketAddr, detail: &str| {
        tracing::warn!("no direct address {address}: {detail}");
        json!({"address": address.to_string(), "state": "down", "detail": detail})
    };
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let made = certificate::load_or_make(home, &host)
        .and_then(|certificate| Ok((tls(&certificate)?, certificate.pin)));
    let (tls, pin) = match made {
        Ok(made) => made,
        Err(detail) => {
            let states = addresses.iter().map(|a| down(a, &detail)).collect();
            return (Vec::new(), states);
        }
    };
    let mut bound = Vec::new();
    let mut states = Vec::new();
    for address in addresses {
        let listener = match TcpListener::bind(address).await {
            Ok(listener) => listener,
            Err(e) => {
                states.push(down(address, &format!("can't listen on {address}: {e}")));
                continue;
            }
        };
        match Direct::new(listener, tls.clone(), pin) {
            Ok(direct) => {
                states.push(
                    json!({"address": direct.local.to_string(), "state": "listening",
                                   "pin": pin.to_string()}),
                );
                bound.push(direct);
            }
            Err(e) => states.push(down(address, &e.to_string())),
        }
    }
    (bound, states)
}

/// Serves `app`, the gateway's router, on each of `directs` until obid stops. Where each is goes
/// to the log and `GET /v1/status`, never stdout: a launcher reads its first lines and may close it.
pub fn serve(directs: Vec<Direct>, app: &axum::Router, stopping: &watch::Receiver<bool>) {
    for direct in directs {
        tracing::info!(address = %direct.local, pin = %direct.pin, "obid direct listening");
        let app = app.clone();
        let mut stopping = stopping.clone();
        tokio::spawn(async move {
            let stopped = async move {
                while !*stopping.borrow() {
                    if stopping.changed().await.is_err() {
                        return;
                    }
                }
            };
            if let Err(e) = axum::serve(direct, app)
                .with_graceful_shutdown(stopped)
                .await
            {
                tracing::error!("a direct address stopped: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::{pin::Pin, pinned};

    async fn served(certificate: &Certificate) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let direct = Direct::new(listener, tls(certificate).unwrap(), certificate.pin).unwrap();
        let address = axum::serve::Listener::local_addr(&direct).unwrap();
        let app = axum::Router::new().route("/v1/status", axum::routing::get(|| async { "here" }));
        tokio::spawn(async move { axum::serve(direct, app).await });
        address
    }

    #[tokio::test]
    async fn computers_11_the_pinned_client_reaches_its_computer_and_no_other() {
        let mine = certificate::make("Studio", 1_800_000_000).unwrap();
        let address = served(&mine).await;
        // A client that sends nothing holds up nobody.
        let _silent = TcpStream::connect(address).await.unwrap();
        let url = format!("https://{address}/v1/status");
        let pinned = pinned::client(mine.pin, Duration::from_secs(20)).unwrap();
        let answer = pinned.get(&url).send().await.unwrap();
        assert_eq!(answer.status(), 200);
        assert_eq!(answer.text().await.unwrap(), "here");

        let other = pinned::client(Pin([7; 32]), Duration::from_secs(20)).unwrap();
        let refused = other.get(&url).send().await.unwrap_err();
        let said = pinned::said(&refused);
        assert!(said.starts_with(pinned::NOT_THE_PINNED), "{said}");
        assert!(said.contains(&mine.pin.to_string()), "{said}");
    }
}
