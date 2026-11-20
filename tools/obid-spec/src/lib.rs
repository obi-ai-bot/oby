//! obid-spec: obid's endpoints and background triggers proven black-box, as BLOK workflows and
//! Rust nodes, against the real obid binary (docs/contracts/obid-spec.md).

pub mod app;
pub mod browser;
pub mod coverage;
/// The conversations area's coverage (`workflows/routes/conversations/`): its own nodes.
pub mod coverage_conversations;
pub mod desktop;
pub mod events;
pub mod expect;
pub mod http;
pub mod keys;
pub mod lab;
pub mod liar;
pub mod nodes;
pub mod obid;
pub mod procs;
pub mod sphere;
pub mod t3_conversations;
pub mod workflow;

/// T3 Code's worktree and git bugs (`workflows/t3/tasks/`): the nodes that act on obid's folders.
pub mod t3_tasks;

/// Every node's name, the internal `cleanup` among them.
pub fn node_names() -> Vec<&'static str> {
    nodes::NODES
        .iter()
        .map(|(n, _)| *n)
        .chain(t3_tasks::NODES.iter().map(|(n, _)| *n))
        .chain(lab::nodes().map(|(n, _)| n))
        .chain(t3_conversations::NODES.iter().map(|(n, _)| *n))
        .chain(coverage_conversations::NODES.iter().map(|(n, _)| *n))
        .chain(["cleanup"])
        .collect()
}

/// The one node type the registry holds, once per name: its input is the step's `inputs` object,
/// its output `{"ok": true, "output": …}` or `{"ok": false, "error": "…"}`, so a failure keeps
/// every word of why.
pub struct SpecNode {
    pub name: &'static str,
    pub description: &'static str,
    pub run: obid::Run,
}

#[async_trait::async_trait]
impl blok::TypedNode for SpecNode {
    type Input = serde_json::Value;
    type Output = serde_json::Value;
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    async fn run(
        &self,
        _: &mut blok::Context,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, blok::BlokError> {
        Ok(match nodes::run_node(&self.run, self.name, input).await {
            Ok(output) => serde_json::json!({"ok": true, "output": output}),
            Err(e) => serde_json::json!({"ok": false, "error": format!("{e:#}")}),
        })
    }
}

/// A client of the nodes child over its stdin and stdout, which carry nothing else (BLOK-3, as
/// oby-check-blok's): no port nor socket.
pub async fn pipes_client(
    child: &mut tokio::process::Child,
) -> anyhow::Result<
    blok::grpc_server::proto::node_runtime_client::NodeRuntimeClient<tonic::transport::Channel>,
> {
    use anyhow::Context;
    let pipes = tokio::io::join(
        child.stdout.take().context("nodes stdout missing")?,
        child.stdin.take().context("nodes stdin missing")?,
    );
    let mut pipes = Some(hyper_util::rt::TokioIo::new(pipes));
    let channel = tonic::transport::Endpoint::from_static("http://obid-spec-nodes")
        .connect_with_connector(tower::service_fn(move |_| {
            let pipes = pipes.take();
            async move { pipes.ok_or_else(|| std::io::Error::other("the nodes' pipes are gone")) }
        }))
        .await?;
    Ok(
        blok::grpc_server::proto::node_runtime_client::NodeRuntimeClient::new(channel)
            .max_decoding_message_size(16 * 1024 * 1024)
            .max_encoding_message_size(16 * 1024 * 1024),
    )
}
