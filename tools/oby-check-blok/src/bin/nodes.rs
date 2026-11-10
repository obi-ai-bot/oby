use oby_check_blok::{CheckNode, GROUPS};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = std::env::var("GRPC_PORT")?.parse()?;
    let mut registry = blok::NodeRegistry::new(env!("CARGO_PKG_VERSION"));
    for (name, group) in GROUPS {
        registry.register_typed(CheckNode(name, group));
    }
    blok::grpc_server::serve_grpc_on(Arc::new(Mutex::new(registry)), "127.0.0.1", port, env!("CARGO_PKG_VERSION"), 4 * 1024 * 1024)
        .await.map_err(|e| anyhow::anyhow!("{e}"))
}
