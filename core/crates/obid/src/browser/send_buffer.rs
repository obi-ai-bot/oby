//! A small send buffer on the API's connections, so a slow reader of a tab's stream drops frames
//! instead of queueing them (BROWSER-14). A frame is sent only once the socket takes it, so the
//! socket's own send buffer is the only queue between obid's newest-frame slot and a stuck reader:
//! Linux grows it to megabytes (dozens of stale frames), macOS keeps it small. 64 KB holds about
//! two frames, and over loopback still carries far more than any answer needs. Found on the Linux
//! CI runner (#2620: a stuck reader dropped none); the lab sets the same on its stream connections
//! (tools/obid-spec/src/lab/browserapi.rs, `STREAM_SEND_BUFFER`). It owns no state.

/// The send buffer each API connection gets: about two frames.
pub const SEND_BUFFER: usize = 64 * 1024;

/// Sets a connection's send buffer to [`SEND_BUFFER`]; a socket that refuses keeps its own.
pub fn small_send_buffer(connection: &tokio::net::TcpStream) {
    if let Err(e) = rustix::net::sockopt::set_socket_send_buffer_size(connection, SEND_BUFFER) {
        tracing::debug!("a connection kept its own send buffer: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The option is set: the buffer a connection reports after is no more than about twice what
    /// was set (Linux doubles the value it's given for its own bookkeeping). What it does for a
    /// stuck reader is proven by the stream workflow on Linux (`get-browser-tab-stream`).
    #[tokio::test]
    async fn browser_14_an_api_connection_gets_a_small_send_buffer() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = tokio::spawn(tokio::net::TcpStream::connect(address));
        let (server, _) = listener.accept().await.unwrap();
        let _client = client.await.unwrap().unwrap();
        small_send_buffer(&server);
        let got = rustix::net::sockopt::socket_send_buffer_size(&server).unwrap();
        assert!(got > 0 && got <= 2 * SEND_BUFFER, "{got}");
    }
}
