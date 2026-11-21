//! The browser's MCP server as each agent's configuration names it (docs/contracts/browser.md,
//! BROWSER-17): the server `browser`, its URL (`<obid>/v1/browser/mcp`, [`c::PATH_MCP`]), the job's
//! broker token, and one permission rule per tool it serves (`mcp__browser__<tool>`, from
//! [`c::MCP_TOOLS`]), in the shape Claude Code's `--mcp-config`, Codex's `-c` settings and an ACP
//! agent's `mcpServers` (Cursor) each take.
//!
//! Its state is the server's URL and the job's token, assigned when it is made; it does no I/O
//! and never writes: whoever runs the agent writes its configuration, and the token, the broker's
//! (crate::connections), dies with the job.

use serde_json::{Value, json};

use super::contract as c;

/// The server's name in every agent's configuration.
pub const NAME: &str = "browser";

/// What every permission rule for the server's tools starts with: `mcp__browser__`.
pub const RULE_PREFIX: &str = "mcp__browser__";

/// obid's browser server for one job's agent.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentServer {
    url: String,
    token: String,
}

impl AgentServer {
    /// The server at `base` (where agents reach obid, `http://127.0.0.1:<port>`), for the job
    /// whose broker token is `token`.
    pub fn new(base: &str, token: &str) -> AgentServer {
        AgentServer {
            url: format!("{}{}", base.trim_end_matches('/'), c::PATH_MCP),
            token: token.to_owned(),
        }
    }

    /// The same server at `url`, where this run reaches obid: an own run on the host reaches it on
    /// loopback, never at the box's address (LANES-26, `Jobs::agent_api`).
    pub fn reached_at(self, url: String) -> AgentServer {
        AgentServer { url, ..self }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// One permission rule per tool the server serves, in `tools/list`'s order.
    pub fn rules() -> impl Iterator<Item = String> {
        c::MCP_TOOLS
            .iter()
            .map(|tool| format!("{RULE_PREFIX}{tool}"))
    }

    /// Whether `tool` (`mcp__browser__oby_browser_open`) is one the server serves.
    pub fn serves(tool: &str) -> bool {
        tool.strip_prefix(RULE_PREFIX)
            .is_some_and(|name| c::MCP_TOOLS.contains(&name))
    }

    /// Claude Code's entry under `mcpServers.browser` in its `--mcp-config` file.
    pub fn claude(&self) -> Value {
        json!({"type": "http", "url": self.url,
               "headers": {"Authorization": format!("Bearer {}", self.token)}})
    }

    /// Codex's `-c` settings, each after its own `-c`: the URL, and the token read from the
    /// environment variable `token_var`, never an argument.
    pub fn codex(&self, token_var: &str) -> [String; 2] {
        [
            format!("mcp_servers.{NAME}.url=\"{}\"", self.url),
            format!("mcp_servers.{NAME}.bearer_token_env_var=\"{token_var}\""),
        ]
    }

    /// An ACP agent's entry in `session/new`'s and `session/load`'s `mcpServers` (Cursor).
    pub fn acp(&self) -> Value {
        json!({"type": "http", "name": NAME, "url": self.url,
               "headers": [{"name": "Authorization", "value": format!("Bearer {}", self.token)}]})
    }
}

/// The token stays out of anything printed.
impl std::fmt::Debug for AgentServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentServer")
            .field("url", &self.url)
            .field("token", &"…")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://127.0.0.1:7777";

    #[test]
    fn browser_17_the_server_is_browser_at_obids_mcp_path_with_the_jobs_token() {
        for base in [BASE, "http://127.0.0.1:7777/"] {
            let server = AgentServer::new(base, "tok");
            assert_eq!(server.url(), "http://127.0.0.1:7777/v1/browser/mcp");
            assert_eq!(server.token(), "tok");
        }
        assert_eq!(NAME, "browser");
    }

    #[test]
    fn browser_17_one_rule_per_served_tool_and_no_other() {
        let rules: Vec<String> = AgentServer::rules().collect();
        assert_eq!(rules.len(), 11);
        assert_eq!(rules[0], "mcp__browser__oby_browser_open");
        assert_eq!(rules[10], "mcp__browser__oby_browser_close_tab");
        for (rule, tool) in rules.iter().zip(c::MCP_TOOLS) {
            assert_eq!(rule, &format!("mcp__browser__{tool}"));
            assert!(AgentServer::serves(rule), "{rule}");
        }
        for not in [
            "mcp__browser__oby_browser_steal",
            "mcp__browser__",
            "mcp__browser__oby_browser_open_x",
            "mcp__tetrix__oby_browser_open",
            "oby_browser_open",
            "mcp__browserx__oby_browser_open",
        ] {
            assert!(!AgentServer::serves(not), "{not}");
        }
    }

    #[test]
    fn browser_17_each_clis_shape_carries_the_url_and_token() {
        let server = AgentServer::new(BASE, "tok");
        assert_eq!(
            server.claude(),
            json!({"type": "http", "url": "http://127.0.0.1:7777/v1/browser/mcp",
                   "headers": {"Authorization": "Bearer tok"}})
        );
        assert_eq!(
            server.codex("OBY_MCP_TOKEN"),
            [
                r#"mcp_servers.browser.url="http://127.0.0.1:7777/v1/browser/mcp""#.to_owned(),
                r#"mcp_servers.browser.bearer_token_env_var="OBY_MCP_TOKEN""#.to_owned(),
            ]
        );
        assert_eq!(
            server.acp(),
            json!({"type": "http", "name": "browser", "url": "http://127.0.0.1:7777/v1/browser/mcp",
                   "headers": [{"name": "Authorization", "value": "Bearer tok"}]})
        );
        // Codex never gets the token as an argument.
        let codex = AgentServer::new(BASE, "s3cr3t").codex("OBY_MCP_TOKEN");
        assert!(!codex.concat().contains("s3cr3t"), "{codex:?}");
    }

    #[test]
    fn browser_17_a_run_reaches_the_server_where_it_reaches_obid() {
        let server = AgentServer::new("http://10.0.2.2:7777", "tok")
            .reached_at("http://127.0.0.1:7777/v1/browser/mcp".into());
        assert_eq!(server.url(), "http://127.0.0.1:7777/v1/browser/mcp");
        assert_eq!(server.token(), "tok");
        assert_eq!(
            server.claude()["url"],
            "http://127.0.0.1:7777/v1/browser/mcp"
        );
    }

    #[test]
    fn browser_17_the_token_is_never_printed() {
        let printed = format!("{:?}", AgentServer::new(BASE, "secret-token"));
        assert!(!printed.contains("secret-token"), "{printed}");
        assert!(printed.contains("/v1/browser/mcp"), "{printed}");
    }
}
