//! An own Codex run's settings (docs/contracts/lanes.md, LANES-18 and LANES-13, the owner's "Yes,
//! match T3" of 2026-11-19): the owner's Codex job or conversation runs unboxed by obid in every
//! mode, with Codex's own sandbox and approvals as its limit, as T3 Code runs it. Oby's mode picks
//! them as T3's runtime mode does (T3 Code's `codexRuntimeModeTurnDefaults`, CodexAdapterV2.ts):
//! a built-in permission profile and an approval policy. In every mode but Bypass the profile is
//! obid's `oby`, that built-in one with reads of obid's secret places denied, as Claude Code's
//! file tools are denied them (LANES-27, the owner's "Merge, add guard after" of 2026-11-20); in
//! Bypass Codex has no sandbox, so nothing denies them. Codex merges obid's `-c` settings with its
//! other layers, and a managed layer outranks them, so a run starts only once Codex says these hold,
//! for the server ([`Policy::config_problem`]) and for the thread ([`Policy::thread_problem`]). It
//! owns no state and writes nothing.

use serde_json::{Value, json};

use crate::permissions::Mode;

/// T3's `full-access` approvals: Codex asks nobody but for an MCP server's own question
/// (`mcp_elicitations`), as TOML for `-c`.
const FULL_ACCESS: &str = "{granular={mcp_elicitations=true, request_permissions=false, rules=false, sandbox_approval=false, skill_approval=false}}";

/// The permission profile of an own run in every mode but Bypass: the mode's built-in one, its
/// `extends`, with obid's secret places denied (LANES-27).
pub const PROFILE: &str = "oby";

/// The built-in profile with no sandbox (Bypass): nothing in it can deny a read.
const NO_SANDBOX: &str = ":danger-full-access";

/// What an own Codex run starts with in a mode: Codex's approval policy, the built-in permission
/// profile (its sandbox) and, in it, what its commands may never read. Measured on codex-cli
/// 0.163.0, on macOS and Linux: `config/read` gives them back from the session's flags (each denied
/// path as `"deny"`), a thread started with them as its `config` comes back on them, and a denied
/// path's read fails in Codex's sandbox while the worktree stays readable and writable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// `approval_policy`, as TOML for `-c`.
    approval: &'static str,
    /// The built-in profile: `:read-only`, `:workspace` or `:danger-full-access`.
    pub base: &'static str,
    /// What its commands never read, each as Codex's sandbox names a path (a `*` and `**` match).
    denied: Vec<String>,
    /// What its commands write besides the working folder, and what stays read-only inside it: a
    /// conversation's folder and its git (WORKSPACE-1), only where Codex writes without asking.
    writes: Vec<String>,
    kept: Vec<String>,
}

/// The policy an own Codex run gets in `mode`, as T3 Code maps its runtime modes: Ask and Plan
/// (T3's `approval-required`) ask on request in a sandbox that writes nothing; Accept edits and
/// Sandbox (`auto-accept-edits`, `auto`) ask on request in one that writes the working folder and
/// the temporary folders, with no network; Bypass (`full-access`) runs with no sandbox and asks
/// nobody but for an MCP server's own question. T3 sends `untrusted` for `approval-required`;
/// codex-cli 0.163.0 refuses it in its configuration ("no longer supported"), so it asks on request.
/// It denies nothing until [`Policy::denying`] names what.
pub fn policy(mode: Mode) -> Policy {
    let (approval, base) = match mode {
        Mode::Ask | Mode::Plan => ("\"on-request\"", ":read-only"),
        Mode::AcceptEdits | Mode::Sandbox => ("\"on-request\"", ":workspace"),
        Mode::Bypass => (FULL_ACCESS, NO_SANDBOX),
    };
    Policy {
        approval,
        base,
        denied: Vec::new(),
        writes: Vec::new(),
        kept: Vec::new(),
    }
}

impl Policy {
    /// This policy, its commands never reading `places` (LANES-27); in Bypass there's no sandbox to
    /// deny them in, so it's unchanged.
    pub fn denying(mut self, places: impl IntoIterator<Item = String>) -> Self {
        if self.guarded() {
            self.denied = places.into_iter().collect();
            self.denied.sort();
            self.denied.dedup();
        }
        self
    }

    /// This policy, its commands also writing `writes` and never `kept` inside them (WORKSPACE-1):
    /// only where Codex's sandbox writes its working folder without asking (`:workspace`, Accept
    /// edits and Sandbox); in Ask and Plan a write still asks, and Bypass has no sandbox.
    pub fn writing(mut self, writes: Vec<String>, kept: Vec<String>) -> Self {
        if self.base == ":workspace" {
            (self.writes, self.kept) = (writes, kept);
        }
        self
    }

    /// Its `filesystem` entries: each path and what its commands may do there.
    fn entries(&self) -> std::collections::BTreeMap<&str, &'static str> {
        let mut all = std::collections::BTreeMap::new();
        all.extend(self.writes.iter().map(|p| (p.as_str(), "write")));
        all.extend(self.kept.iter().map(|p| (p.as_str(), "read")));
        all.extend(self.denied.iter().map(|p| (p.as_str(), "none")));
        all
    }

    /// Whether its commands run in a sandbox obid's profile can add denies to: all but Bypass.
    fn guarded(&self) -> bool {
        self.base != NO_SANDBOX
    }

    /// The profile Codex runs it in: obid's `oby`, or the built-in one with no sandbox.
    pub fn profile(&self) -> &'static str {
        if self.guarded() { PROFILE } else { self.base }
    }

    /// The approval policy as Codex's JSON gives it (`config/read`, a thread's answer).
    pub fn approval(&self) -> Value {
        match self.approval.strip_prefix('"') {
            Some(name) => json!(name.trim_end_matches('"')),
            None => json!({"granular": {"mcp_elicitations": true, "request_permissions": false,
                "rules": false, "sandbox_approval": false, "skill_approval": false}}),
        }
    }

    /// The `-c` settings that start Codex on it, before `app-server`: the policy, the profile and,
    /// when it's obid's, its base and what it denies.
    pub fn settings(&self) -> Vec<String> {
        let mut set = vec![
            format!("approval_policy={}", self.approval),
            format!("default_permissions=\"{}\"", self.profile()),
        ];
        if self.guarded() {
            let denied: Vec<String> = (self.entries().into_iter())
                .map(|(path, mode)| format!("{} = \"{mode}\"", json!(path)))
                .collect();
            set.push(format!("permissions.{PROFILE}.extends=\"{}\"", self.base));
            set.push(format!(
                "permissions.{PROFILE}.filesystem={{{}}}",
                denied.join(", ")
            ));
        }
        set.into_iter().flat_map(|s| ["-c".to_owned(), s]).collect()
    }

    /// What every thread, new or resumed, is started with (its `config`): a resumed thread would
    /// otherwise come back in the policy and profile it was last saved with.
    pub fn thread_config(&self) -> Value {
        json!({"approval_policy": self.approval(), "default_permissions": self.profile()})
    }

    /// How far it lets a run go without asking: nothing written, the working folder, everything.
    fn reach(&self) -> u8 {
        match self.base {
            ":read-only" => 0,
            ":workspace" => 1,
            _ => 2,
        }
    }

    /// Whether `now` lets a run go less far than this, the one it launched with: an own run asks
    /// obid only what its policy asks, so its turn ends and the next one starts on `now`'s.
    pub fn narrowed_by(&self, now: &Policy) -> bool {
        now.reach() < self.reach()
    }

    /// Why a run can't start on Codex's answer `read` to `config/read`: the approval policy or the
    /// profile Codex merged isn't this one (a managed layer's, say), obid's profile doesn't extend
    /// the mode's built-in one, a path it denies isn't denied, or a key of it isn't obid's own
    /// (another layer's entry, an `auth.json` to read, would widen it).
    pub fn config_problem(&self, read: &Value) -> Option<String> {
        let (c, origins) = (&read["config"], &read["origins"]);
        let profile = &c["permissions"][PROFILE];
        let prefix = format!("permissions.{PROFILE}.");
        let added = (origins.as_object().into_iter().flatten())
            .find(|(key, at)| key.starts_with(&prefix) && at["name"]["type"] != "sessionFlags")
            .map(|(key, _)| key.as_str());
        // Codex says a `none` entry as `"deny"` (codex-cli 0.163.0).
        let undenied = (self.entries().into_iter())
            .find(|(path, mode)| {
                let said = if *mode == "none" { "deny" } else { mode };
                profile["filesystem"][*path] != said
            })
            .map(|(path, _)| format!("{prefix}filesystem.{path}"));
        let guarded = self.guarded();
        let wrong = [
            ("approval_policy", c["approval_policy"] != self.approval()),
            (
                "default_permissions",
                c["default_permissions"] != self.profile(),
            ),
            (
                "permissions.oby.extends",
                guarded && profile["extends"] != self.base,
            ),
            (undenied.as_deref().unwrap_or_default(), undenied.is_some()),
            (added.unwrap_or_default(), guarded && added.is_some()),
        ];
        let (what, _) = wrong.into_iter().find(|(_, wrong)| *wrong)?;
        Some(not_obids(what))
    }

    /// Why a run can't go on in the thread Codex `started` or resumed: not on this policy, or not
    /// in this profile (obid's, extending the mode's built-in one).
    pub fn thread_problem(&self, started: &Value) -> Option<String> {
        let active = &started["activePermissionProfile"];
        let held = started["approvalPolicy"] == self.approval()
            && active["id"] == self.profile()
            && (!self.guarded() || active["extends"] == self.base);
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

    const VAULT: &str = "/h/.oby/home/vault";

    /// What codex-cli 0.163.0 answers `config/read` with for `p`'s settings and nothing else
    /// configured: each key from the session's flags, each denied path as `"deny"`.
    fn read_of(p: &Policy) -> Value {
        let flags = json!({"name": {"type": "sessionFlags"}, "version": "v"});
        let mut config = p.thread_config();
        let mut origins = json!({"approval_policy": flags, "default_permissions": flags});
        if p.guarded() {
            let files: serde_json::Map<String, Value> = (p.denied.iter())
                .map(|d| (d.clone(), json!("deny")))
                .collect();
            config["permissions"] = json!({PROFILE: {"extends": p.base, "filesystem": files}});
            origins[format!("permissions.{PROFILE}.extends")] = flags.clone();
            for d in &p.denied {
                origins[format!("permissions.{PROFILE}.filesystem.{d}")] = flags.clone();
            }
        }
        json!({"config": config, "origins": origins})
    }

    /// LANES-18 (the owner's "Yes, match T3"): Oby's modes become Codex's own settings as T3
    /// Code's runtime modes do, the same on every OS. LANES-27: every mode but Bypass runs in
    /// obid's profile, the mode's built-in one with obid's secret places denied.
    #[test]
    fn lanes_18_oby_modes_map_to_codex_as_t3s() {
        let on_request = json!("on-request");
        for mode in Mode::ALL {
            let p = policy(mode).denying([VAULT.to_owned()]);
            let want = match mode {
                Mode::Ask | Mode::Plan => (on_request.clone(), ":read-only", PROFILE),
                Mode::AcceptEdits | Mode::Sandbox => (on_request.clone(), ":workspace", PROFILE),
                Mode::Bypass => (
                    json!({"granular": {"mcp_elicitations": true, "request_permissions": false,
                        "rules": false, "sandbox_approval": false, "skill_approval": false}}),
                    ":danger-full-access",
                    ":danger-full-access",
                ),
            };
            assert_eq!((p.approval(), p.base, p.profile()), want, "{mode:?}");
        }
        assert_eq!(
            policy(Mode::Ask).denying([VAULT.to_owned()]).settings(),
            [
                "-c",
                "approval_policy=\"on-request\"",
                "-c",
                "default_permissions=\"oby\"",
                "-c",
                "permissions.oby.extends=\":read-only\"",
                "-c",
                "permissions.oby.filesystem={\"/h/.oby/home/vault\" = \"none\"}",
            ]
        );
        // Bypass has no sandbox: nothing denies a read there.
        let bypass = policy(Mode::Bypass).denying([VAULT.to_owned()]).settings();
        assert_eq!(bypass.len(), 4, "{bypass:?}");
        assert!(bypass[1].starts_with("approval_policy={granular="));
        assert_eq!(bypass[3], "default_permissions=\":danger-full-access\"");
    }

    /// LANES-18, LANES-27: a run starts only on the settings its mode sets, as Codex answers
    /// `config/read` (codex-cli 0.163.0's shape) and as the thread comes back: another approval
    /// policy, another profile, another base, a denied path that isn't denied and a key of obid's
    /// profile from another layer each stop it, named.
    #[test]
    fn lanes_18_settings_codex_merged_in_stop_an_own_run() {
        for mode in Mode::ALL {
            let p = policy(mode).denying([VAULT.to_owned()]);
            assert_eq!(p.config_problem(&read_of(&p)), None, "{mode:?}");
            let thread = json!({"approvalPolicy": p.approval(),
                "activePermissionProfile": {"id": p.profile(),
                    "extends": if p.guarded() { json!(p.base) } else { Value::Null }}});
            assert_eq!(p.thread_problem(&thread), None, "{mode:?}");
            for other in Mode::ALL
                .map(policy)
                .into_iter()
                .filter(|o| o.base != p.base)
            {
                let other = other.denying([VAULT.to_owned()]);
                assert!(p.config_problem(&read_of(&other)).is_some(), "{mode:?}");
            }
        }
        let ask = policy(Mode::Ask).denying([VAULT.to_owned()]);
        let user = json!({"name": {"type": "user", "file": "/h/.codex/config.toml"}});
        let filesystem = format!("permissions.oby.filesystem.{VAULT}");
        type Change<'a> = (&'a str, &'a dyn Fn(&mut Value));
        let cases: [Change; 5] = [
            ("approval_policy", &|r| {
                r["config"]["approval_policy"] = json!("never")
            }),
            ("default_permissions", &|r| {
                r["config"]["default_permissions"] = json!(":workspace")
            }),
            ("permissions.oby.extends", &|r| {
                r["config"]["permissions"]["oby"]["extends"] = json!(":danger-full-access")
            }),
            (&filesystem, &|r| {
                r["config"]["permissions"]["oby"]["filesystem"][VAULT] = json!("read")
            }),
            ("permissions.oby.filesystem./h/.codex/auth.json", &|r| {
                r["origins"]["permissions.oby.filesystem./h/.codex/auth.json"] = user.clone()
            }),
        ];
        for (what, change) in cases {
            let mut read = read_of(&ask);
            change(&mut read);
            let why = ask.config_problem(&read).unwrap_or_default();
            assert!(why.contains(&format!("({what})")), "{what}: {why}");
        }
        for (key, value) in [
            ("approvalPolicy", json!("never")),
            (
                "activePermissionProfile",
                json!({"id": "oby", "extends": ":workspace"}),
            ),
            ("activePermissionProfile", json!({"id": ":read-only"})),
        ] {
            let mut thread = json!({"approvalPolicy": "on-request",
                "activePermissionProfile": {"id": "oby", "extends": ":read-only"}});
            assert_eq!(ask.thread_problem(&thread), None);
            thread[key] = value;
            assert!(ask.thread_problem(&thread).is_some(), "{thread}");
        }
    }

    /// LANES-18: a mode that lets a run go less far than the one it launched with ends its turn.
    #[test]
    fn lanes_18_a_narrower_mode_ends_an_own_turn() {
        let (bypass, edits, ask) = (
            policy(Mode::Bypass),
            policy(Mode::AcceptEdits),
            policy(Mode::Ask),
        );
        assert!(bypass.narrowed_by(&edits) && bypass.narrowed_by(&ask) && edits.narrowed_by(&ask));
        assert!(!ask.narrowed_by(&bypass) && !edits.narrowed_by(&bypass) && !ask.narrowed_by(&ask));
        assert!(
            !policy(Mode::Plan).narrowed_by(&ask) && !edits.narrowed_by(&policy(Mode::Sandbox))
        );
    }
}
