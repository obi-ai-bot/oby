use blok::grpc_server::{proto::node_runtime_server::NodeRuntimeServer, BlokNodeRuntime};
use oby_check_blok::{CheckInput, CheckNode, GROUPS};
use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;

/// The one connection: gRPC over this process's stdin and stdout, which carry nothing else, so
/// the nodes need no port nor socket and run in a box with no network at all (BLOK-3).
struct Pipes(tokio::io::Join<tokio::io::Stdin, tokio::io::Stdout>);

impl tonic::transport::server::Connected for Pipes {
    type ConnectInfo = ();
    fn connect_info(&self) {}
}

impl AsyncRead for Pipes {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for Pipes {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let input = CheckInput {
        checkout: std::env::var("OBY_CHECK_DIR")?,
        base: std::env::var("OBY_CHECK_BASE")?,
    };
    let token = std::env::var("OBY_CHECK_TOKEN")?;
    anyhow::ensure!(!token.is_empty(), "missing node token");
    let mut registry = blok::NodeRegistry::new(env!("CARGO_PKG_VERSION"));
    for (name, group) in GROUPS {
        registry.register_typed(CheckNode(name, group, input.clone(), token.clone()));
    }
    // blok-rs's own service, as its `serve_grpc_on` serves it, on the pipes in place of a port.
    let runtime = BlokNodeRuntime::new(Arc::new(Mutex::new(registry)), env!("CARGO_PKG_VERSION"));
    let pipes = Pipes(tokio::io::join(tokio::io::stdin(), tokio::io::stdout()));
    // ponytail: the stream never ends, so tonic keeps serving the pipes; the host kills this
    // process when its workflow ends.
    let incoming = tokio_stream::once(Ok::<_, io::Error>(pipes)).chain(tokio_stream::pending());
    tonic::transport::Server::builder()
        .add_service(
            NodeRuntimeServer::new(runtime)
                .max_decoding_message_size(4 * 1024 * 1024)
                .max_encoding_message_size(4 * 1024 * 1024),
        )
        .serve_with_incoming(incoming)
        .await?;
    Ok(())
}
