//! Which lane a job's agent runs in (docs/contracts/lanes.md): the owner's work in its CLI's own
//! sandbox on the account's own folder and login, another Oby's or an untrusted job's in Oby's box.

use super::*;

impl Jobs {
    /// What in a conversation's room Claude Code would load as its settings, skills, MCP servers
    /// or instructions: never written by the session, in either lane (LANES-12), so nothing a turn
    /// read steers the next one; an own session loads only the owner's user settings anyway.
    pub(crate) const ROOM_CONFIG: [&'static str; 4] =
        [".claude", ".mcp.json", "CLAUDE.md", "CLAUDE.local.md"];

    /// Whether `job`'s `tool` run is in the own lane (LANES-1): every Claude Code job of the
    /// owner's, unless another Oby asked for it ([`Job::obi`]) or it's marked untrusted
    /// ([`Job::untrusted`]); the other marks don't move it (the owner's decision of 2026-11-09).
    /// Oby's own review stays boxed, as `review_may_run` expects, and so does the owner's review of
    /// someone else's code (MERGING-36). Its run gets [`own_sandbox_env`] and
    /// [`own_sandbox_settings`] and no egress proxy (LANES-3, LANES-6); every other run is boxed
    /// with a copy of its login. `claudeOwnSandbox` doesn't change it.
    pub(crate) fn own_lane(job: &Job, tool: Tool) -> bool {
        tool == Tool::Claude
            && job.obi.is_none()
            && !job.untrusted
            && job.review_of.is_none()
            && !(job.read_only() && job.someone_elses().is_some())
    }

    /// Whether job `id`'s agent had a network obid's proxy didn't see: Claude Code ran it, now or
    /// before a move ([`Job::moves`]), in the own lane, where no proxy marks a site it reached
    /// `reached`. Until a command's sites do (LANES-6), its check gets no declared secrets, as a
    /// job whose agent reached past the package registries gets none.
    pub(super) fn ran_unproxied(&self, id: &str) -> bool {
        self.get(id).is_some_and(|job| {
            let claude = Tool::Claude.name();
            let ran_claude = job.agent == Tool::Claude.id()
                || job
                    .moves
                    .iter()
                    .any(|m| m.from.starts_with(claude) || m.to.starts_with(claude));
            ran_claude && Self::own_lane(&job, Tool::Claude)
        })
    }

    /// An own Claude Code conversation's session (crate::brain::harness, LANES-12), as an own job's
    /// run: on `account`'s own folder and login, unboxed by obid, its Bash in Claude Code's own
    /// sandbox ([`own_sandbox_settings`]), which writes only `room` (not `frozen` in it) and `temp`,
    /// its commands' `HOME`, set by `env.sh` in `login`; Claude Code itself gets an own job's
    /// environment ([`own_sandbox_env`]: the person's `HOME`, the owner's shell's). The brain's
    /// planner and every other conversation stay boxed (`Jobs::conversation_launch`).
    #[allow(clippy::too_many_arguments)]
    pub fn conversation_own_launch(
        &self,
        account: &crate::config::Account,
        room: &Path,
        temp: &Path,
        login: &Path,
        mut frozen: Vec<PathBuf>,
        flags: Vec<OsString>,
        stderr: PathBuf,
    ) -> Result<Launch, String> {
        // The room's own settings, MCP servers and instructions, read-only to its Bash and tools.
        let real_room = room.canonicalize().unwrap_or_else(|_| room.to_path_buf());
        frozen.extend(Self::ROOM_CONFIG.map(|name| real_room.join(name)));
        let sandbox = self
            .sandbox
            .as_ref()
            .map_err(|why| format!("obid can't run Claude Code: {why}"))?;
        let bin = self
            .agent_bin(Tool::Claude)
            .ok_or("obid can't find Claude Code's binary.")?;
        let mut hidden = vec![self.config.home.clone()];
        hidden.extend(person_homes());
        hidden.extend(self.config.agents.hidden_folders(&self.config.home));
        let read_only = sandbox::tool_roots(
            &process::tools_path(),
            &hidden,
            std::slice::from_ref(&self.config.home),
        );
        let spec = Spec {
            worktree: room.to_path_buf(),
            git_admin: None,
            git_common: None,
            commits: false,
            temp: temp.to_path_buf(),
            accounts: Vec::new(),
            read_only,
            hidden,
            network: true,
            loopback: false,
            proxied: false,
            frozen,
            fixed: Vec::new(),
        };
        let mut extra = own_sandbox_env(account, &spec, login)?;
        // The owner's user settings, which it loads (CONVO-4), held to an own job's rule: nothing
        // obid's `--settings` can't take back, no file tool allowed outside the room.
        own_settings_allowed(&own_settings_file(account)?, &spec, &extra)?;
        extra.extend(CLAUDE_TELEMETRY_OFF.map(|name| (name, "1".into())));
        // Off as `--restricted` had it: what a turn read never lands in the account's memory.
        extra.push((process::NO_AUTO_MEMORY, "1".into()));
        let settings = own_sandbox_settings(&spec, login, &extra)?;
        let mut settings: Value = serde_json::from_str(&settings)
            .map_err(|e| format!("obid can't make Claude Code's settings: {e}"))?;
        if let Some(deny) = settings["permissions"]["deny"].as_array_mut() {
            deny.extend(self.conversation_denials().into_iter().map(Value::from));
        }
        let settings = settings.to_string();
        let argv: Vec<OsString> = std::iter::once(bin.into_os_string())
            .chain(flags)
            .chain(["--settings".into(), settings.into()])
            .collect();
        Ok(Launch {
            own_sandbox: true,
            group: None,
            argv: sandbox.unboxed(&argv),
            // The owner's login shell's environment under obid's, as an own job's (LANES-4).
            env: {
                let mut env = process::sandbox_env(&process::tools_path(), temp, &extra);
                process::with_owners(&mut env, &process::login_env().unwrap_or_default());
                env
            },
            cwd: room.to_path_buf(),
            stderr,
        })
    }

    /// Read and Edit deny rules (Read's cover Grep and Glob, Edit's Write and the rest) for what an
    /// own conversation's file tools never reach, whatever the owner's settings or a skill allow
    /// (a deny beats every allow; obid answers only what Claude Code asks, `rule`): everything in
    /// obid's home but its conversations (the vault, its keys and token, the logins and accounts,
    /// made later too); in every conversation's folder, its login, config, session, log and its
    /// commands' `HOME`'s Claude Code login copy (its room stays, as other rooms do); and each
    /// Claude Code account's settings, state, login and hooks, so no turn plants a hook.
    fn conversation_denials(&self) -> Vec<String> {
        let home = &self.config.home;
        let real_home = home.canonicalize().unwrap_or_else(|_| home.clone());
        let mut places: Vec<PathBuf> = std::fs::read_dir(home)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name())
            .filter(|name| name != "conversations")
            .map(|name| home.join(name))
            .collect();
        places.extend(["vault", "keys", "api-token", "accounts", "logins"].map(|n| home.join(n)));
        let each = home.join("conversations/*");
        places.extend(
            [
                "login",
                "config",
                "session.json",
                "agent.log",
                "home/.claude",
                "home/.claude.json",
            ]
            .map(|n| each.join(n)),
        );
        let mut folders: Vec<PathBuf> = self
            .accounts(Tool::Claude)
            .into_iter()
            .filter_map(|account| account.config_dir)
            .collect();
        if let Some(person) = login_home() {
            folders.push(person.join(".claude"));
            places.push(person.join(".claude.json"));
        }
        for folder in folders {
            let made = [
                "settings*.json",
                ".claude.json",
                ".credentials.json",
                "hooks",
            ];
            places.extend(made.map(|n| folder.join(n)));
        }
        // As named and where it leads (a temporary folder's `/var` is `/private/var`).
        let real = |p: &PathBuf| {
            let (parent, name) = (p.parent()?, p.file_name()?);
            parent.canonicalize().ok().map(|r| r.join(name))
        };
        let mut paths = places.clone();
        paths.extend(places.iter().filter_map(real));
        paths.extend(
            places
                .iter()
                .filter_map(|p| p.strip_prefix(home).ok())
                .map(|rest| real_home.join(rest)),
        );
        paths.sort();
        paths.dedup();
        let mut rules = Vec::new();
        for path in paths.iter().map(|p| p.to_string_lossy()) {
            for tool in ["Read", "Edit"] {
                rules.push(format!("{tool}(/{path})"));
                rules.push(format!("{tool}(/{path}/**)"));
            }
        }
        rules
    }

    /// The box job `id`'s check runs in, from its agent's `spec`, in either lane (LANES-6): Oby's,
    /// with no network and no account's folder, loopback only for a job no other Oby asked for
    /// ([`Job::check_keeps_loopback`]).
    pub(super) fn check_spec(&self, id: &str, spec: &Spec) -> Spec {
        let mut spec = spec.clone();
        spec.network = false;
        spec.loopback = self.get(id).is_some_and(|job| job.check_keeps_loopback());
        spec.accounts.clear();
        spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LANES-1 (docs/contracts/lanes.md): another Oby's job, one marked `untrusted` and Oby's own
    /// review are foreign; one marked only `foreign` or `reached` is the owner's, and so own.
    /// Codex and Cursor stay boxed for now.
    #[test]
    fn lanes_1_whose_work_picks_the_lane() {
        let owners = Job::new(
            "j".into(),
            "o/r".into(),
            "fix".into(),
            "Fix it".into(),
            "claude".into(),
            "a".into(),
        );
        let marked = |mark: fn(&mut Job)| {
            let mut job = owners.clone();
            mark(&mut job);
            Jobs::own_lane(&job, Tool::Claude)
        };
        assert!(Jobs::own_lane(&owners, Tool::Claude));
        assert!(!Jobs::own_lane(&owners, Tool::Codex));
        assert!(!Jobs::own_lane(&owners, Tool::Cursor));
        assert!(marked(
            |job| job.foreign = Some("someone else's issue".into())
        ));
        assert!(marked(
            |job| job.reached = Some("its agent reached a site".into())
        ));
        assert!(!marked(|job| job.untrusted = true));
        assert!(!marked(|job| job.review_of = Some("k".into())));
        assert!(!marked(|job| {
            job.obi = Some(crate::job::ObiRef {
                correlation_id: "c".into(),
                from: "obi-ernesto".into(),
                from_owner: "ernesto".into(),
                request_id: "r".into(),
            })
        }));
        // MERGING-36: the owner's review is own until its code is someone else's.
        assert!(marked(|job| job.kind = "review".into()));
        assert!(!marked(|job| {
            job.kind = "review".into();
            job.foreign = Some("someone else's pull request".into());
        }));
    }

    /// LANES-12 (docs/contracts/lanes.md): an own conversation's Claude Code gets settings that
    /// keep `repos/` and the room's own settings, MCP servers and CLAUDE.md read-only, hide obid's
    /// home from its commands and keep them off the code hosts, on the account's own folder with
    /// no proxy of obid's and its auto-memory off.
    #[cfg(target_os = "macos")]
    #[test]
    fn lanes_12_an_own_conversations_settings_keep_it_in_its_room() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-lanes-12-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let session = root.join("session");
        std::fs::write(&session, "#!/bin/sh\nshift 2\nexec \"$@\"\n").unwrap();
        std::fs::set_permissions(&session, std::fs::Permissions::from_mode(0o755)).unwrap();
        let account = root.join("account");
        std::fs::create_dir_all(&account).unwrap();
        let config = crate::config::Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": root.join("home"), "owner": "o",
                "agents": {"claude": {"bin": "/usr/bin/true",
                    "accounts": [{"id": "a", "name": "A", "configDir": account}]}}})
            .to_string(),
        )
        .unwrap();
        let jobs = Jobs::open(
            config,
            crate::events::Events::new(),
            Sandbox::detect(&session),
        )
        .unwrap();
        let home = jobs.config.home.canonicalize().unwrap();
        let place = home.join("conversations/c");
        let [room, temp, login] = ["room", "temp", "login"].map(|d| place.join(d));
        let repos = room.join("repos");
        for dir in [&repos, &temp] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let claude = jobs.accounts(Tool::Claude).remove(0);
        let launch = jobs
            .conversation_own_launch(
                &claude,
                &room,
                &temp,
                &login,
                vec![repos.clone()],
                vec!["-p".into()],
                place.join("agent.log"),
            )
            .unwrap();
        assert!(launch.own_sandbox);
        let argv: Vec<String> = launch
            .argv
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let at = argv.iter().position(|a| a == "--settings").unwrap();
        let settings: Value = serde_json::from_str(&argv[at + 1]).unwrap();
        let listed = |list: &Value, item: &str| {
            list.as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str() == Some(item))
        };
        let fs = &settings["sandbox"]["filesystem"];
        let deny = &settings["permissions"]["deny"];
        // repos/ and the room's own settings: neither its commands nor its tools write them.
        let mut frozen = vec![repos.clone()];
        frozen.extend(Jobs::ROOM_CONFIG.map(|name| room.join(name)));
        for path in &frozen {
            let path = path.to_str().unwrap();
            assert!(listed(&fs["denyWrite"], path), "{path}: {fs}");
            assert!(listed(deny, &format!("Edit(/{path}/**)")), "{path}: {deny}");
        }
        assert!(listed(&fs["allowWrite"], room.to_str().unwrap()), "{fs}");
        // obid's home stays hidden from its commands: the vault, logins, other conversations.
        assert!(listed(&fs["denyRead"], home.to_str().unwrap()), "{fs}");
        assert!(!listed(&fs["allowRead"], home.to_str().unwrap()), "{fs}");
        // Its commands reach no code host: only Publish pushes.
        for host in super::CODE_HOSTS {
            assert!(
                listed(&settings["sandbox"]["network"]["deniedDomains"], host),
                "{host}"
            );
        }
        let env = |name: &str| {
            launch
                .env
                .iter()
                .rev()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(env(process::NO_AUTO_MEMORY), Some("1".into()));
        assert_eq!(env("GIT_DIR"), None);
        // An own job's environment (LANES-5): the person's own HOME, where the owner's hooks find
        // what they name, and the account's folder only because it has one (the default account
        // gets none, `own_sandbox_env_and_settings`).
        assert_eq!(env("HOME"), login_home().map(PathBuf::into_os_string));
        assert_eq!(
            env("CLAUDE_CONFIG_DIR"),
            Some(account.clone().into_os_string())
        );
        assert!(settings["env"]["GIT_DIR"].is_null(), "{settings}");
        for (name, value) in &launch.env {
            if name.to_string_lossy().ends_with("_PROXY") {
                assert_eq!(std::env::var_os(name).as_ref(), Some(value), "{name:?}");
            }
        }
        // Its file tools never reach obid's secret places or an account's settings and hooks,
        // whatever the owner's settings allow: denied outright, as the room is not.
        let user_settings = account.join("settings.json");
        let (home, account) = (home.display(), account.display());
        for rule in [
            format!("Read(/{home}/vault/**)"),
            format!("Edit(/{home}/vault/**)"),
            format!("Read(/{home}/api-token)"),
            format!("Read(/{home}/logins/**)"),
            format!("Read(/{home}/conversations/*/login/**)"),
            format!("Read(/{home}/conversations/*/session.json)"),
            format!("Read(/{home}/conversations/*/home/.claude/**)"),
            format!("Edit(/{account}/settings*.json)"),
            format!("Edit(/{account}/.claude.json)"),
            format!("Edit(/{account}/hooks/**)"),
        ] {
            assert!(listed(deny, &rule), "{rule}: {deny}");
        }
        let room_text = room.display().to_string();
        assert!(
            !deny.as_array().unwrap().iter().any(|rule| {
                let rule = rule.as_str().unwrap();
                let path = rule[rule.find('/').unwrap() + 1..].trim_end_matches(')');
                room_text.starts_with(path.trim_end_matches("/**"))
            }),
            "{deny}"
        );
        // The owner's user settings may not let a file tool past the room without asking.
        let launch_with = |set: &str| {
            std::fs::write(&user_settings, set).unwrap();
            jobs.conversation_own_launch(
                &claude,
                &room,
                &temp,
                &login,
                vec![repos.clone()],
                Vec::new(),
                place.join("agent.log"),
            )
        };
        for set in [
            r#"{"permissions": {"allow": ["Read"]}}"#,
            r#"{"permissions": {"allow": ["Read(~/**)"]}}"#,
            r#"{"permissions": {"allow": ["Edit(//etc/**)"]}}"#,
            r#"{"permissions": {"allow": ["Grep"]}}"#,
            r#"{"permissions": {"allow": ["Glob"]}}"#,
            r#"{"permissions": {"allow": ["LS"]}}"#,
            r#"{"permissions": {"additionalDirectories": ["/tmp"]}}"#,
            r#"{"sandbox": {"excludedCommands": ["git"]}}"#,
            // A Claude Code settings source that would outrank obid's `--settings` (LANES-5).
            r#"{"env": {"CLAUDE_CODE_MANAGED_SETTINGS_PATH": "/tmp/x.json"}}"#,
        ] {
            assert!(launch_with(set).is_err(), "{set}");
        }
        for set in [
            r#"{"permissions": {"allow": ["Read(./notes.md)", "Bash(git status)"]}}"#,
            r#"{"hooks": {}}"#,
        ] {
            assert!(launch_with(set).is_ok(), "{set}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-6 (docs/contracts/lanes.md): an own Claude Code job runs with no proxy of obid's,
    /// and its check is still Oby's box with no network, no account's folder and only loopback.
    #[cfg(target_os = "macos")]
    #[test]
    fn lanes_6_an_own_jobs_check_has_no_network() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-lanes-6-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let session = root.join("session");
        std::fs::write(&session, "#!/bin/sh\nshift 2\nexec \"$@\"\n").unwrap();
        std::fs::set_permissions(&session, std::fs::Permissions::from_mode(0o755)).unwrap();
        let account = root.join("account");
        std::fs::create_dir_all(&account).unwrap();
        let config = crate::config::Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": root.join("home"), "owner": "o",
                "repos": [{"repo": "o/r", "remote": "https://example.com/o/r.git"}],
                "agents": {"claude": {"bin": "/usr/bin/true",
                    "accounts": [{"id": "a", "name": "A", "configDir": account}]}}})
            .to_string(),
        )
        .unwrap();
        let jobs = Jobs::open(
            config,
            crate::events::Events::new(),
            Sandbox::detect(&session),
        )
        .unwrap();
        let job = Job::new(
            "own".into(),
            "o/r".into(),
            "fix".into(),
            "Fix it".into(),
            "claude".into(),
            "a".into(),
        );
        let [worktree, admin, common, temp] =
            ["worktree", "admin", "common", "temp"].map(|d| root.join(d));
        for dir in [&worktree, &admin, &common, &temp] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let mut spec = jobs.spec(&worktree, &admin, &common, &temp, true, Tool::Claude);
        let account = jobs.accounts(Tool::Claude).remove(0);
        let launch = jobs
            .launch(&job, &spec, Tool::Claude, Vec::new(), &account, None)
            .unwrap();
        assert!(launch.own_sandbox);
        // None of obid's own: a proxy the tests' own environment holds may pass through.
        for (name, value) in &launch.env {
            if name.to_string_lossy().ends_with("_PROXY") {
                assert_eq!(std::env::var_os(name).as_ref(), Some(value), "{name:?}");
            }
        }
        // No proxy saw what it reached, so its check gets no declared secrets; a boxed job's,
        // and a Codex job's that never ran on Claude Code, are judged as before.
        let mut boxed = job.clone();
        boxed.id = "boxed".into();
        boxed.untrusted = true;
        let mut codex = job.clone();
        codex.id = "codex".into();
        codex.agent = "codex".into();
        let mut moved = codex.clone();
        moved.id = "moved".into();
        moved.moves.push(crate::job::Move {
            from: "Claude Code · A".into(),
            to: "Codex · B".into(),
            why: None,
            date: 0,
            handoff: false,
        });
        for other in [boxed, codex, moved] {
            jobs.lock().jobs.insert(other.id.clone(), other);
        }
        jobs.lock().jobs.insert(job.id.clone(), job);
        assert!(jobs.ran_unproxied("own"));
        assert!(jobs.ran_unproxied("moved"));
        assert!(!jobs.ran_unproxied("boxed"));
        assert!(!jobs.ran_unproxied("codex"));
        spec.accounts.push(root.join("account"));
        let check = jobs.check_spec("own", &spec);
        assert!(!check.network);
        assert!(!check.proxied);
        assert!(check.loopback);
        assert!(check.accounts.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
