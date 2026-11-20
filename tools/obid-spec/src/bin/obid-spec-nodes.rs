use blok::grpc_server::{proto::node_runtime_server::NodeRuntimeServer, BlokNodeRuntime};
use obid_spec::{nodes::NODES, obid::Run, SpecNode};
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
    let run = Run::from_env()?;
    let mut registry = blok::NodeRegistry::new(env!("CARGO_PKG_VERSION"));
    for (name, description) in NODES
        .iter()
        .copied()
        .chain(obid_spec::t3_tasks::NODES.iter().copied())
        .chain(obid_spec::lab::nodes())
        .chain(obid_spec::t3_conversations::NODES.iter().copied())
        .chain(obid_spec::coverage_conversations::NODES.iter().copied())
        .chain(obid_spec::coverage_accounts::NODES.iter().copied())
        .chain([("cleanup", "Stops every process this workflow started.")])
    {
        registry.register_typed(SpecNode {
            name,
            description,
            run: run.clone(),
        });
    }
    // blok-rs's own service, on the pipes in place of a port (as oby-check-blok-nodes).
    let runtime = BlokNodeRuntime::new(Arc::new(Mutex::new(registry)), env!("CARGO_PKG_VERSION"));
    let pipes = Pipes(tokio::io::join(tokio::io::stdin(), tokio::io::stdout()));
    // The stream never ends, so tonic keeps serving the pipes; the host stops this process when
    // its workflow ends, after its `cleanup` step stopped what it started.
    let incoming = tokio_stream::once(Ok::<_, io::Error>(pipes)).chain(tokio_stream::pending());
    tonic::transport::Server::builder()
        .add_service(
            NodeRuntimeServer::new(runtime)
                .max_decoding_message_size(16 * 1024 * 1024)
                .max_encoding_message_size(16 * 1024 * 1024),
        )
        .serve_with_incoming(incoming)
        .await?;
    Ok(())
}
