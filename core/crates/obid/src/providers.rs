//! Each agent CLI's plain facts, one row per tool in routing order (docs/contracts/providers.md).
//! A tool's tested CLI versions are in `cli-versions.json` (crate::cli_versions), not here.

use crate::agent::Tool;

/// One agent CLI's facts.
pub struct Descriptor {
    pub tool: Tool,
    /// [`Tool::id`] and [`Tool::name`].
    pub id: &'static str,
    pub name: &'static str,
    /// The binary it runs as when the configuration names none.
    pub bin: &'static str,
    pub default_model: &'static str,
    pub efforts: &'static [&'static str],
    /// The fixed models it falls back to: `(id, name)`.
    pub models: &'static [(&'static str, &'static str)],
    /// Its API hosts, matched exactly (crate::egress::tools_api, MERGING-30).
    pub api_hosts: &'static [&'static str],
    /// Its vendor's domains, with their subdomains (crate::egress::tools_own).
    pub own_domains: &'static [&'static str],
}

/// Every tool's row, in the order of [`Tool::ALL`].
pub static PROVIDERS: &[Descriptor] = &[
    Descriptor {
        tool: Tool::Claude,
        id: "claude",
        name: "Claude Code",
        bin: "claude",
        default_model: "claude-opus-5-5",
        efforts: &["low", "medium", "high", "xhigh", "max"],
        models: &[
            ("claude-fable-5-1", "Fable 5.1"),
            ("claude-opus-5-5", "Opus 5.5"),
            ("claude-sonnet-5-5", "Sonnet 5.5"),
            ("claude-haiku-5-5", "Haiku 5.5"),
        ],
        api_hosts: &[
            "api.anthropic.com",
            "statsig.anthropic.com",
            "console.anthropic.com",
        ],
        own_domains: &["anthropic.com", "claude.ai", "claude.com"],
    },
    Descriptor {
        tool: Tool::Codex,
        id: "codex",
        name: "Codex",
        bin: "codex",
        default_model: "gpt-6.1-sol",
        efforts: &["low", "medium", "high", "xhigh"],
        models: &[
            ("gpt-6.1-sol", "gpt-6.1-sol"),
            ("gpt-6-astra", "GPT-6-Astra"),
        ],
        api_hosts: &["api.openai.com", "auth.openai.com"],
        own_domains: &["openai.com", "chatgpt.com"],
    },
    Descriptor {
        tool: Tool::Cursor,
        id: "cursor",
        name: "Cursor",
        bin: "cursor-agent",
        default_model: "auto",
        efforts: &[],
        models: &[
            ("auto", "Auto"),
            ("claude-opus-5-5", "Opus 5.5"),
            ("gpt-6.1-sol", "gpt-6.1-sol"),
        ],
        api_hosts: &["api2.cursor.sh", "api3.cursor.sh", "api4.cursor.sh"],
        own_domains: &["cursor.sh", "cursor.com"],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// PROVIDERS-1: every tool id has exactly one row, and `Tool::ALL` is the rows' order.
    #[test]
    fn providers_1_each_id_has_exactly_one_row() {
        assert_eq!(PROVIDERS.len(), Tool::ALL.len());
        for (i, tool) in Tool::ALL.into_iter().enumerate() {
            assert_eq!(PROVIDERS[i].tool, tool);
            assert!(std::ptr::eq(tool.row(), &PROVIDERS[i]), "{tool:?}");
        }
        let ids: Vec<&str> = PROVIDERS.iter().map(|row| row.id).collect();
        assert_eq!(ids, ["claude", "codex", "cursor"]);
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len());
        for id in ids {
            assert_eq!(Tool::from_id(id).map(Tool::id), Some(id));
        }
        assert_eq!(Tool::from_id("gemini"), None);
        let names: Vec<&str> = Tool::ALL.into_iter().map(Tool::name).collect();
        assert_eq!(names, ["Claude Code", "Codex", "Cursor"]);
    }
}
