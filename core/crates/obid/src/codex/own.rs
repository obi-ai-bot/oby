//! An own Codex run's settings (docs/contracts/lanes.md, LANES-18 and LANES-13, the owner's "Yes,
//! match T3" of 2026-11-19): the owner's Codex job or conversation runs unboxed by obid in every
//! mode, with Codex's own sandbox and approvals as its limit, as T3 Code runs it. Oby's mode picks
//! them as T3's runtime mode does (T3 Code's `codexRuntimeModeTurnDefaults`, CodexAdapterV2.ts):
//! a built-in permission profile and an approval policy, and obid adds no profile, trust, hook or
//! environment setting of its own. Codex merges obid's `-c` settings with its other layers, and a
//! managed layer outranks them, so a run starts only once Codex says these two hold, for the
//! server ([`Policy::config_problem`]) and for the thread ([`Policy::thread_problem`]). It owns no
//! state and writes nothing.

use serde_json::{Value, json};

use crate::permissions::Mode;

/// T3's `full-access` approvals: Codex asks nobody but for an MCP server's own question
/// (`mcp_elicitations`), as TOML for `-c`.
const FULL_ACCESS: &str = "{granular={mcp_elicitations=true, request_permissions=false, rules=false, sandbox_approval=false, skill_approval=false}}";

/// What an own Codex run starts with in a mode: Codex's approval policy and the built-in
/// permission profile (its sandbox). Measured on codex-cli 0.163.0: `config/read` gives both back
/// from the session's flags, and a thread started with them as its `config` comes back with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// `approval_policy`, as TOML for `-c`.
    approval: &'static str,
    /// `default_permissions`: `:read-only`, `:workspace` or `:danger-full-access`.
    pub profile: &'static str,
}

/// The policy an own Codex run gets in `mode`, as T3 Code maps its runtime modes: Ask and Plan
/// (T3's `approval-required`) ask on request in a sandbox that writes nothing; Accept edits and
/// Sandbox (`auto-accept-edits`, `auto`) ask on request in one that writes the working folder and
/// the temporary folders, with no network; Bypass (`full-access`) runs with no sandbox and asks
/// nobody but for an MCP server's own question. T3 sends `untrusted` for `approval-required`;
/// codex-cli 0.163.0 refuses it in its configuration ("no longer supported"), so it asks on request.
pub fn policy(mode: Mode) -> Policy {
    match mode {
        Mode::Ask | Mode::Plan => Policy {
            approval: "\"on-request\"",
            profile: ":read-only",
        },
        Mode::AcceptEdits | Mode::Sandbox => Policy {
            approval: "\"on-request\"",
            profile: ":workspace",
        },
        Mode::Bypass => Policy {
            approval: FULL_ACCESS,
            profile: ":danger-full-access",
        },
    }
}

impl Policy {
    /// The approval policy as Codex's JSON gives it (`config/read`, a thread's answer).
    pub fn approval(&self) -> Value {
        match self.approval.strip_prefix('"') {
            Some(name) => json!(name.trim_end_matches('"')),
            None => json!({"granular": {"mcp_elicitations": true, "request_permissions": false,
                "rules": false, "sandbox_approval": false, "skill_approval": false}}),
        }
    }

    /// The `-c` settings that start Codex on it, before `app-server`.
    pub fn settings(&self) -> [String; 4] {
        [
            "-c".into(),
            format!("approval_policy={}", self.approval),
            "-c".into(),
            format!("default_permissions=\"{}\"", self.profile),
        ]
    }

    /// What every thread, new or resumed, is started with (its `config`): a resumed thread would
    /// otherwise come back in the policy and profile it was last saved with.
    pub fn thread_config(&self) -> Value {
        json!({"approval_policy": self.approval(), "default_permissions": self.profile})
    }

    /// How far it lets a run go without asking: nothing written, the working folder, everything.
    fn reach(&self) -> u8 {
        match self.profile {
            ":read-only" => 0,
            ":workspace" => 1,
            _ => 2,
        }
    }

    /// Whether `now` lets a run go less far than this, the one it launched with: an own run asks
    /// obid only what its policy asks, so its turn ends and the next one starts on `now`'s.
    pub fn narrowed_by(&self, now: Policy) -> bool {
        now.reach() < self.reach()
    }

    /// Why a run can't start on Codex's answer `read` to `config/read`: the approval policy or the
    /// profile Codex merged isn't this one (a managed layer's, say).
    pub fn config_problem(&self, read: &Value) -> Option<String> {
        let c = &read["config"];
        let wrong = [
            ("approval_policy", c["approval_policy"] != self.approval()),
            (
                "default_permissions",
                c["default_permissions"] != self.profile,
            ),
        ];
        let (what, _) = wrong.into_iter().find(|(_, wrong)| *wrong)?;
        Some(not_obids(what))
    }

    /// Why a run can't go on in the thread Codex `started` or resumed: not on this policy, or not
    /// in this profile.
    pub fn thread_problem(&self, started: &Value) -> Option<String> {
        let held = started["approvalPolicy"] == self.approval()
            && started["activePermissionProfile"]["id"] == self.profile;
        (!held).then(|| not_obids("the thread's approval policy or permission profile"))
    }
}

/// What obid tells the owner when an own run can't start on Codex's settings.
fn not_obids(what: &str) -> String {
    format!(
        "Codex's own settings on this computer change what this mode sets ({what}), so obid started no run. Take that out of Codex's config.toml or its managed settings."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LANES-18 (the owner's "Yes, match T3"): Oby's modes become Codex's own settings as T3
    /// Code's runtime modes do, the same on every OS.
    #[test]
    fn lanes_18_oby_modes_map_to_codex_as_t3s() {
        let got = Mode::ALL.map(|m| (m, policy(m).approval(), policy(m).profile));
        let on_request = json!("on-request");
        for (mode, approval, profile) in got {
            let want = match mode {
                Mode::Ask | Mode::Plan => (on_request.clone(), ":read-only"),
                Mode::AcceptEdits | Mode::Sandbox => (on_request.clone(), ":workspace"),
                Mode::Bypass => (
                    json!({"granular": {"mcp_elicitations": true, "request_permissions": false,
                        "rules": false, "sandbox_approval": false, "skill_approval": false}}),
                    ":danger-full-access",
                ),
            };
            assert_eq!((approval, profile), want, "{mode:?}");
        }
        assert_eq!(
            policy(Mode::Ask).settings(),
            [
                "-c",
                "approval_policy=\"on-request\"",
                "-c",
                "default_permissions=\":read-only\""
            ]
        );
        assert!(policy(Mode::Bypass).settings()[1].starts_with("approval_policy={granular="));
    }

    /// LANES-18: a run starts only on the settings its mode sets, as Codex answers `config/read`
    /// (codex-cli 0.163.0's shape) and as the thread comes back.
    #[test]
    fn lanes_18_settings_codex_merged_in_stop_an_own_run() {
        for mode in Mode::ALL {
            let p = policy(mode);
            let read = json!({"config": p.thread_config()});
            assert_eq!(p.config_problem(&read), None, "{mode:?}");
            let thread = json!({"approvalPolicy": p.approval(),
                "activePermissionProfile": {"id": p.profile, "extends": null}});
            assert_eq!(p.thread_problem(&thread), None, "{mode:?}");
            for other in Mode::ALL.map(policy).into_iter().filter(|o| *o != p) {
                let read = json!({"config": other.thread_config()});
                assert!(p.config_problem(&read).is_some(), "{mode:?}");
            }
        }
        let ask = policy(Mode::Ask);
        let never =
            json!({"config": {"approval_policy": "never", "default_permissions": ":read-only"}});
        assert!(
            ask.config_problem(&never)
                .unwrap()
                .contains("approval_policy")
        );
        let wide = json!({"approvalPolicy": "on-request",
            "activePermissionProfile": {"id": ":danger-full-access"}});
        assert!(ask.thread_problem(&wide).is_some());
    }

    /// LANES-18: a mode that lets a run go less far than the one it launched with ends its turn.
    #[test]
    fn lanes_18_a_narrower_mode_ends_an_own_turn() {
        let (bypass, edits, ask) = (
            policy(Mode::Bypass),
            policy(Mode::AcceptEdits),
            policy(Mode::Ask),
        );
        assert!(bypass.narrowed_by(edits) && bypass.narrowed_by(ask) && edits.narrowed_by(ask));
        assert!(!ask.narrowed_by(bypass) && !edits.narrowed_by(bypass) && !ask.narrowed_by(ask));
        assert!(!policy(Mode::Plan).narrowed_by(ask) && !edits.narrowed_by(policy(Mode::Sandbox)));
    }
}
