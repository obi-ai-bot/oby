use oby_check_blok::{CheckInput, CheckNode, GROUPS};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .with_writer(std::io::stdout)
        .init();
    let port: u16 = std::env::var("GRPC_PORT")?.parse()?;
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
    blok::grpc_server::serve_grpc_on(
        Arc::new(Mutex::new(registry)),
        "127.0.0.1",
        port,
        env!("CARGO_PKG_VERSION"),
        4 * 1024 * 1024,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}
