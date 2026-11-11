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

    /// obid's fixed answer to an own run's call in `worktree` of job `id` (LANES-14), which Claude
    /// Code asks about whatever the owner's settings, skills or hooks allow (its `ask` rules,
    /// [`own_sandbox_settings`]): its tools run unboxed, so never the worktree's `.git`, which
    /// obid's later git calls would otherwise follow, nor a path outside the worktree but Claude
    /// Code's own places ([`claude_place`]). None: the job's mode and rules answer it.
    pub(super) fn own_reach(
        &self,
        id: &str,
        name: &str,
        input: &Value,
        worktree: &Path,
    ) -> Option<Answer> {
        if Self::OWN_QUIET.contains(&name) || MCP_RESOURCE_TOOLS.contains(&name) {
            return Some(Answer::Allow);
        }
        let pointer = policy::real_path(&worktree.join(".git"));
        let names_pointer = ["file_path", "notebook_path"]
            .iter()
            .filter_map(|key| input.get(*key).and_then(Value::as_str))
            .any(|path| policy::real_path(&worktree.join(path)) == pointer);
        if names_pointer {
            return Some(Answer::Deny(
                "obid keeps the worktree's .git as it made it.".into(),
            ));
        }
        if !policy::names_outside(name, input, worktree) {
            return None;
        }
        let plan = self
            .get(id)
            .is_some_and(|j| Self::mode_of(&j) == Mode::Plan);
        let temp = self.temp_path(id);
        Some(
            match claude_place(
                name,
                input,
                worktree,
                Some(&temp),
                &self.claude_folders(),
                plan,
            ) {
                true => Answer::Allow,
                false => Answer::Deny(format!(
                    "Stay inside this job's worktree ({}).",
                    worktree.display()
                )),
            },
        )
    }

    /// The built-in tools of Claude Code's an own run asks obid about (its `ask` rules, LANES-14)
    /// that reach nothing past the session: finding a deferred tool, its task list, its own
    /// background tasks, its agents' names, planning, a helper's report, a skill of the owner's
    /// (whose `allowed-tools` no longer skip obid, and whose hooks never run: LANES-17). obid lets
    /// them run at once, as Claude Code did.
    #[rustfmt::skip]
    pub(crate) const OWN_QUIET: [&'static str; 11] = [
        "ToolSearch", "TodoWrite", "TaskCreate", "TaskUpdate", "TaskList", "TaskGet", "TaskStop",
        "ListAgents", "EnterPlanMode", "Skill", "SubagentHandback",
    ];

    /// Every Claude Code account's folder: each `configDir`, and the person's `~/.claude`.
    pub(crate) fn claude_folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = self
            .accounts(Tool::Claude)
            .into_iter()
            .filter_map(|account| account.config_dir)
            .collect();
        folders.extend(login_home().map(|person| person.join(".claude")));
        folders
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
        // Its `login` folder, where obid writes `env.sh` and the MCP configuration Claude Code
        // starts servers from outside any box (crate::brain::harness): never its commands' to write.
        frozen.push(login.to_path_buf());
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
            deny.extend(self.conversation_denials(room).into_iter().map(Value::from));
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
    /// made later too); in every conversation's folder, this one's too, its `login` (the MCP
    /// configuration with the turn's tokens, `env.sh`), config, session and log; every other
    /// conversation's commands' `HOME` (its room stays, as other rooms do), never the one of the
    /// conversation whose room is `room`, where Claude Code keeps its background commands' output
    /// (`CLAUDE_CODE_TMPDIR`) and its Bash reads anyway; and each Claude Code account's settings,
    /// state, login and hooks, so no turn plants a hook.
    fn conversation_denials(&self, room: &Path) -> Vec<String> {
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
        let conversations = home.join("conversations");
        // The stored threads, every conversation's (LANES-15), and the next one's id.
        places.extend(["*.json", "next-id"].map(|n| conversations.join(n)));
        let each = conversations.join("*");
        // `login` whole: a running turn's Oby and Tetrix tokens (`oby-mcp.json`), its `env.sh`, a
        // boxed one's login copy.
        places.extend(["login", "config", "session.json", "agent.log"].map(|n| each.join(n)));
        // Every other conversation's `HOME` whole, by name: a deny rule can't leave one folder out
        // of a pattern. One made after this launch has its tokens and login state denied by name
        // (a boxed session's MCP configuration is in its `HOME`).
        let this = room.parent().and_then(Path::file_name);
        places.extend(
            std::fs::read_dir(&conversations)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|entry| Some(entry.file_name().as_os_str()) != this)
                .map(|entry| entry.path().join("home")),
        );
        places.extend(
            [CONVERSATION_MCP_FILE, ".claude", ".claude.json"].map(|n| each.join("home").join(n)),
        );
        places.extend(login_home().map(|person| person.join(".claude.json")));
        for folder in self.claude_folders() {
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

/// Whether an own run's `tool` call, which names a path outside its working folder `cwd`, reaches
/// only Claude Code's own places, which it read or wrote without asking before its `ask` rules
/// (LANES-14): to read, its commands' `temp` folder (their background output, `CLAUDE_CODE_TMPDIR`)
/// and `projects/<cwd as Claude Code names it>` in an account's `folders` (long outputs it saved,
/// its memory); in `plan` mode, to write, a plan in an account's `plans/`, never through a link,
/// nor one of a file's other names (a hard link).
/// Only absolute paths: a relative one is the working folder's, which a link there may lead out of.
pub(crate) fn claude_place(
    tool: &str,
    input: &Value,
    cwd: &Path,
    temp: Option<&Path>,
    folders: &[PathBuf],
    plan: bool,
) -> bool {
    let named = ["file_path", "notebook_path", "path"]
        .iter()
        .filter_map(|key| input.get(*key).and_then(Value::as_str));
    if !named.clone().all(|p| p.starts_with('/')) {
        return false;
    }
    let real = |p: &Path| p.canonicalize().ok();
    if ["Read", "Grep", "Glob", "LS", "NotebookRead"].contains(&tool) {
        // Claude Code's name for a folder: every character but a letter or digit is `-`.
        let name = |p: &Path| -> String {
            let p = p.to_string_lossy();
            p.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect()
        };
        let names = [Some(cwd.to_path_buf()), real(cwd)].map(|p| p.map(|p| name(&p)));
        let projects = folders.iter().flat_map(|f| {
            names
                .iter()
                .flatten()
                .map(move |n| f.join("projects").join(n))
        });
        return temp
            .map(Path::to_path_buf)
            .into_iter()
            .chain(projects)
            .any(|root| !policy::names_outside(tool, input, &root));
    }
    let plans: Vec<PathBuf> = folders
        .iter()
        .filter_map(|f| real(&f.join("plans")))
        .collect();
    plan && matches!(tool, "Write" | "Edit")
        && input["file_path"]
            .as_str()
            .map(Path::new)
            .is_some_and(|file| {
                file.extension().is_some_and(|e| e == "md")
                    && std::fs::symlink_metadata(file).map_or(true, |m| {
                        m.is_file() && std::os::unix::fs::MetadataExt::nlink(&m) == 1
                    })
                    && file
                        .parent()
                        .and_then(real)
                        .is_some_and(|dir| plans.contains(&dir))
            })
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
        // As crate::brain::harness's `Place`: `home` is its commands' HOME and CLAUDE_CODE_TMPDIR.
        let [room, temp, login] = ["room", "home", "login"].map(|d| place.join(d));
        let repos = room.join("repos");
        let other = home.join("conversations/other/home");
        for dir in [&repos, &temp, &other] {
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
        // Its `login` folder, with the MCP configuration Claude Code starts servers from outside
        // any box: no command of its writes there, nor its Edit and Write tools.
        let config = login.join("oby-mcp.json");
        assert!(listed(&fs["denyWrite"], login.to_str().unwrap()), "{fs}");
        assert!(
            listed(deny, &format!("Edit(/{}/**)", login.display())),
            "{deny}"
        );
        let writable = fs["allowWrite"].as_array().unwrap().iter();
        assert!(
            !writable
                .filter_map(Value::as_str)
                .any(|w| config.starts_with(w)),
            "{fs}"
        );
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
            // Every conversation's login, every other one's commands' HOME whole, and the tokens
            // and login state in the HOME of one made after this launch.
            format!("Edit(/{home}/conversations/*/login/**)"),
            format!("Read(/{}/**)", other.display()),
            format!("Edit(/{}/**)", other.display()),
            format!("Read(/{home}/conversations/*/home/oby-mcp.json)"),
            format!("Read(/{home}/conversations/*/home/.claude/**)"),
            format!("Read(/{home}/conversations/*/home/.claude.json)"),
            format!("Edit(/{account}/settings*.json)"),
            format!("Edit(/{account}/.claude.json)"),
            format!("Edit(/{account}/hooks/**)"),
        ] {
            assert!(listed(deny, &rule), "{rule}: {deny}");
        }
        // Its room, and its own HOME, where Claude Code keeps its background commands' output
        // (`CLAUDE_CODE_TMPDIR`), which its Read reads (F-M2-62): no rule covers them.
        fn covers(pattern: &[&str], path: &[&str]) -> bool {
            match (pattern.split_first(), path.split_first()) {
                (Some((&"**", _)), _) | (None, None) => true,
                (Some((want, pattern)), Some((have, path))) => {
                    let one = match want.split_once('*') {
                        Some((a, b)) => have.starts_with(a) && have.ends_with(b),
                        None => want == have,
                    };
                    one && covers(pattern, path)
                }
                _ => false,
            }
        }
        let output = temp.join("claude-501/-room/tasks/t.output");
        for kept in [room.join("notes.md"), output] {
            let kept = kept.to_str().unwrap();
            let kept: Vec<&str> = kept.split('/').collect();
            assert!(
                !deny.as_array().unwrap().iter().any(|rule| {
                    let rule = rule.as_str().unwrap();
                    let path = rule[rule.find('/').unwrap() + 1..].trim_end_matches(')');
                    covers(&path.split('/').collect::<Vec<_>>(), &kept)
                }),
                "{kept:?}: {deny}"
            );
        }
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
            // A `CLAUDE*` name off the gateway allow-list (`process::claude_gateway`, LANES-4a): a
            // settings source that would outrank obid's `--settings`.
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

    /// obid's jobs on a home in `root`, with one Claude Code account with a folder of its own, and
    /// a "sandbox" that runs what it's given (`root/session`).
    fn own_jobs(root: &Path) -> Arc<Jobs> {
        use std::os::unix::fs::PermissionsExt;
        let session = root.join("session");
        std::fs::write(&session, "#!/bin/sh\nshift 2\nexec \"$@\"\n").unwrap();
        std::fs::set_permissions(&session, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::create_dir_all(root.join("account")).unwrap();
        let config = crate::config::Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": root.join("home"), "owner": "o",
                "agents": {"claude": {"bin": "/usr/bin/true",
                    "accounts": [{"id": "a", "name": "A", "configDir": root.join("account")}]}}})
            .to_string(),
        )
        .unwrap();
        let events = crate::events::Events::new();
        Jobs::open(config, events, Sandbox::detect(&session)).unwrap()
    }

    /// Claude Code 2.1.302's built-in tools, as its stream-json `init` event lists them with
    /// `--tools` naming every tool it might have (`lanes_14_a_skills_grant_still_asks_obid` reads
    /// a later Claude Code's list itself).
    #[rustfmt::skip]
    const INIT_TOOLS: [&str; 34] = [
        "AskUserQuestion", "Bash", "CronCreate", "CronDelete", "CronList", "DesignSync", "Edit",
        "EnterPlanMode", "EnterWorktree", "ExitPlanMode", "ExitWorktree", "Glob", "Grep",
        "ListAgents", "Monitor", "NotebookEdit", "PushNotification", "Read", "RemoteTrigger",
        "ReportFindings", "ScheduleWakeup", "SendMessage", "Skill", "Task", "TaskCreate",
        "TaskGet", "TaskList", "TaskStop", "TaskUpdate", "ToolSearch", "WebFetch", "WebSearch",
        "Workflow", "Write",
    ];

    /// LANES-14: every built-in tool of Claude Code's is one an own run asks obid about, and so is
    /// every tool obid's lists name (the file tools' rules, the harmless ones it lets run).
    #[test]
    fn lanes_14_every_built_in_tool_asks_obid() {
        for tool in INIT_TOOLS.iter().chain(&FILE_TOOLS).chain(&Jobs::OWN_QUIET) {
            assert!(CLAUDE_TOOLS.contains(tool), "{tool}");
        }
    }

    /// LANES-17: an own job's Claude Code runs none of the owner's hooks (`disableAllHooks`); and
    /// (LANES-14) it has only the built-in tools obid answers (`--tools`), its MCP servers'
    /// resources among them, never one obid doesn't know.
    #[cfg(target_os = "macos")]
    #[test]
    fn lanes_17_an_own_job_has_only_the_tools_obid_answers_and_no_hooks() {
        let root = std::env::temp_dir().join(format!("obid-lanes-14e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let jobs = own_jobs(&root);
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
        let spec = jobs.spec(&worktree, &admin, &common, &temp, true, Tool::Claude);
        let account = jobs.accounts(Tool::Claude).remove(0);
        let launch = jobs
            .launch(&job, &spec, Tool::Claude, Vec::new(), &account, None)
            .unwrap();
        let argv: Vec<&str> = launch.argv.iter().filter_map(|a| a.to_str()).collect();
        let at = argv.iter().position(|a| *a == "--tools").unwrap();
        let tools: Vec<&str> = argv[at + 1].split(',').collect();
        for tool in &tools {
            assert!(CLAUDE_TOOLS.contains(tool), "{tool}");
        }
        for tool in MCP_RESOURCE_TOOLS
            .iter()
            .chain(&["Read", "Bash", "Skill", "Task"])
        {
            assert!(tools.contains(tool), "{tool}: {tools:?}");
        }
        for tool in [
            "Monitor",
            "EnterWorktree",
            "CronCreate",
            "RemoteTrigger",
            "Workflow",
        ] {
            assert!(!tools.contains(&tool), "{tool}: {tools:?}");
        }
        let at = argv.iter().position(|a| *a == "--settings").unwrap();
        let settings: Value = serde_json::from_str(argv[at + 1]).unwrap();
        assert_eq!(settings["disableAllHooks"], true, "{settings}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-14: of what an own run asks obid, what reaches nothing past the session runs at once;
    /// any other tool goes to the job's mode and rules, and a file outside the worktree is refused.
    #[test]
    fn lanes_14_an_own_runs_harmless_tools_run_at_once() {
        let root = std::env::temp_dir().join(format!("obid-lanes-14d-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let jobs = own_jobs(&root);
        let worktree = root.join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        let reach = |tool: &str, input: Value| jobs.own_reach("j", tool, &input, &worktree);
        for tool in ["Skill", "ToolSearch", "TaskStop", "TodoWrite"] {
            assert_eq!(reach(tool, json!({})), Some(Answer::Allow), "{tool}");
        }
        assert_eq!(reach("Monitor", json!({"command": "ls"})), None);
        let secret = json!({"file_path": root.join("home/vault/secrets.json")});
        assert!(matches!(reach("Read", secret), Some(Answer::Deny(_))));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-14 (docs/contracts/lanes.md): an own run's Claude Code asks obid before every call of
    /// a built-in or MCP tool, whatever the owner's skills, settings or hooks allow: its
    /// `--settings` name them all as `ask` rules, which beat those.
    #[cfg(target_os = "macos")]
    #[test]
    fn lanes_14_every_call_of_an_own_run_asks_obid() {
        let root = std::env::temp_dir().join(format!("obid-lanes-14-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let jobs = own_jobs(&root);
        let place = jobs.config.home.join("conversations/c");
        let [room, temp, login] = ["room", "home", "login"].map(|d| place.join(d));
        for dir in [&room, &temp] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let claude = jobs.accounts(Tool::Claude).remove(0);
        let launch = jobs
            .conversation_own_launch(
                &claude,
                &room,
                &temp,
                &login,
                Vec::new(),
                Vec::new(),
                place.join("log"),
            )
            .unwrap();
        let at = launch.argv.iter().position(|a| a == "--settings").unwrap();
        let settings: Value = serde_json::from_str(launch.argv[at + 1].to_str().unwrap()).unwrap();
        // LANES-17: none of the owner's hooks, in a conversation as in a job.
        assert_eq!(settings["disableAllHooks"], true, "{settings}");
        let asked = &settings["permissions"]["ask"];
        for tool in CLAUDE_TOOLS.iter().chain(&["mcp__*"]) {
            assert!(
                asked
                    .as_array()
                    .is_some_and(|a| a.iter().any(|t| t == tool)),
                "{tool}: {asked}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-14: of what lies outside an own run's working folder obid lets it reach only Claude
    /// Code's own places: its commands' temp folder and its account's `projects/<that folder>` to
    /// read, and in Plan mode a plan in `plans/`; never another folder, a relative path a link
    /// in the room could lead out of, a link in those places, or a plan that isn't one.
    #[test]
    fn lanes_14_an_own_run_reaches_only_claude_codes_own_places() {
        let root = std::env::temp_dir().join(format!("obid-lanes-14b-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let [cwd, temp, folder, outside] =
            ["work tree", "temp", "account", "outside"].map(|d| root.join(d));
        let name: String = cwd
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let saved = folder.join("projects").join(&name).join("s/tool-results");
        for dir in [&cwd, &temp, &saved, &folder.join("plans"), &outside] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(outside.join("secret"), "s").unwrap();
        std::os::unix::fs::symlink(&outside, temp.join("link")).unwrap();
        std::os::unix::fs::symlink(&outside, cwd.join("out")).unwrap();
        std::os::unix::fs::symlink(outside.join("secret"), folder.join("plans/linked.md")).unwrap();
        std::fs::hard_link(outside.join("secret"), folder.join("plans/also.md")).unwrap();
        let folders = vec![folder.clone()];
        let place = |tool: &str, path: &Path, plan: bool| {
            let key = if tool == "Grep" { "path" } else { "file_path" };
            let input = json!({ key: path });
            claude_place(tool, &input, &cwd, Some(&temp), &folders, plan)
        };
        assert!(place(
            "Read",
            &temp.join("claude-501/x/tasks/t.output"),
            false
        ));
        assert!(place("Read", &saved.join("r.txt"), false));
        assert!(place("Grep", &folder.join("projects").join(&name), false));
        assert!(place("Write", &folder.join("plans/a-plan.md"), true));
        for (tool, path, plan) in [
            ("Read", outside.join("secret"), false),
            ("Read", temp.join("link/secret"), false),
            ("Read", folder.join("projects/-other/r.txt"), false),
            ("Read", folder.join("settings.json"), false),
            ("Edit", temp.join("x"), false),
            ("Write", folder.join("plans/a-plan.md"), false),
            ("Write", folder.join("plans/linked.md"), true),
            ("Write", folder.join("plans/also.md"), true),
            ("Write", folder.join("plans/a.json"), true),
            ("Write", folder.join("plans/../settings.md"), true),
        ] {
            assert!(!place(tool, &path, plan), "{tool} {}", path.display());
        }
        // Claude Code reads it as the working folder's `out/secret`, the link's: not the temp's.
        assert!(!place("Read", Path::new("out/secret"), false));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-14, measured on this Mac's real Claude Code: a skill whose `allowed-tools` grants
    /// Write, a hook that answers `allow`, and an allow rule for an MCP tool each let a call through
    /// without asking, unless obid's `ask` rules are there; then it asks, and with no one to answer
    /// (`-p`) nothing happens. Every built-in tool its `init` event lists is one obid's rules name
    /// (`NotebookRead` is no tool of 2.1.302's). Nothing of the login is read or printed.
    /// `cargo test -p obid --lib lanes_14_a_skill -- --ignored`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "runs the real Claude Code on this Mac's own login"]
    fn lanes_14_a_skills_grant_still_asks_obid() {
        use std::io::{BufRead, Write};
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-lanes-14c-{}", uuid::Uuid::new_v4()));
        let [work, outside] = ["work", "outside"].map(|d| root.join(d));
        let skill = work.join(".claude/skills/probe");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: probe\ndescription: Writes any file the user names.\nallowed-tools: Edit, Write, Read\n---\nWrite the file the user names with the Write tool.\n",
        )
        .unwrap();
        // A hook that allows every call, and an MCP server whose one tool leaves a file when called.
        let script = |name: &str, text: String| {
            let path = root.join(name);
            std::fs::write(&path, text).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let hook = script(
            "allow.sh",
            "#!/bin/sh\necho '{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"allow\"}}'\n".into(),
        );
        let called = outside.join("mcp-called");
        let server = script(
            "mcp.pl",
            format!(
                r#"#!/usr/bin/perl
                $| = 1;
                while (<STDIN>) {{
                  next unless /"id":\s*(\d+)/; my $id = $1; my $r = "{{}}";
                  if (/"initialize"/) {{ $r = '{{"protocolVersion":"2025-06-18","capabilities":{{"tools":{{}},"resources":{{}}}},"serverInfo":{{"name":"probe","version":"1"}}}}' }}
                  elsif (/"tools\/list"/) {{ $r = '{{"tools":[{{"name":"echo","description":"Echoes.","inputSchema":{{"type":"object"}}}}]}}' }}
                  elsif (/"tools\/call"/) {{ open(my $f, ">", "{}"); close $f; $r = '{{"content":[{{"type":"text","text":"ok"}}]}}' }}
                  print "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":$r}}\n";
                }}
"#,
                called.display()
            ),
        );
        let mcp = root.join("mcp.json");
        let servers = json!({"mcpServers": {"probe": {"command": server}}});
        std::fs::write(&mcp, servers.to_string()).unwrap();
        // obid lets its harmless tools run at once (`OWN_QUIET`); with no one to answer here,
        // `--allowedTools` stands in for that answer, and they're left out of the `ask` rules.
        let quiet = |t: &&&str| !Jobs::OWN_QUIET.contains(t);
        let ask: Vec<&str> = CLAUDE_TOOLS
            .iter()
            .filter(quiet)
            .chain(&["mcp__*"])
            .copied()
            .collect();
        // What Claude Code did with `settings` and `prompt`, and the built-in tools it had.
        let run = |settings: Value, prompt: &str, tools: Option<&str>| -> Vec<String> {
            let mut command = std::process::Command::new("claude");
            command
                .current_dir(&work)
                .args([
                    "-p",
                    "--no-session-persistence",
                    "--setting-sources",
                    "project",
                ])
                .args(["--permission-mode", "default", "--model", "sonnet"])
                .args([
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--allowedTools",
                ])
                .arg(Jobs::OWN_QUIET.join(","))
                .args(tools.map(|tools| ["--tools", tools]).into_iter().flatten())
                .args(["--strict-mcp-config", "--mcp-config", mcp.to_str().unwrap()])
                .args(["--settings", &settings.to_string()])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped());
            for (name, _) in std::env::vars() {
                if name.starts_with("CLAUDE") || name.starts_with("ANTHROPIC_") {
                    command.env_remove(name);
                }
            }
            let mut child = command.spawn().unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(prompt.as_bytes())
                .unwrap();
            let mut tools = Vec::new();
            for line in std::io::BufReader::new(child.stdout.take().unwrap()).lines() {
                let line: Value = serde_json::from_str(&line.unwrap()).unwrap_or_default();
                if line["subtype"] == "init" {
                    let names = line["tools"].as_array().cloned().unwrap_or_default();
                    tools = names
                        .iter()
                        .filter_map(|t| t.as_str().map(str::to_owned))
                        .collect();
                }
            }
            child.wait().unwrap();
            tools
        };
        let with = |settings: Value, asked: bool| match asked {
            true => {
                let mut settings = settings;
                settings["permissions"]["ask"] = json!(ask);
                settings
            }
            false => settings,
        };
        let write = |name: &str| {
            format!(
                "Use the Write tool to create {} containing the word x. If refused, stop.",
                outside.join(name).display()
            )
        };
        let hooked = json!({"hooks": {"PreToolUse": [{"matcher": "Write",
            "hooks": [{"type": "command", "command": hook}]}]}});
        let allowed = json!({"permissions": {"allow": ["mcp__probe__echo"]}});
        for asked in [false, true] {
            let tag = if asked { "asked" } else { "granted" };
            let skill = format!(
                "Invoke the probe skill with the Skill tool, then: {}",
                write(tag)
            );
            let tools = run(with(json!({}), asked), &skill, None);
            for tool in tools.iter().filter(|t| !t.starts_with("mcp__")) {
                assert!(
                    CLAUDE_TOOLS.contains(&tool.as_str()),
                    "{tool} skips obid's rules"
                );
            }
            assert_eq!(outside.join(tag).exists(), !asked, "a skill's grant, {tag}");
            let hook = format!("hook-{tag}");
            run(with(hooked.clone(), asked), &write(&hook), None);
            assert_eq!(
                outside.join(&hook).exists(),
                !asked,
                "a hook's allow, {tag}"
            );
            let _ = std::fs::remove_file(&called);
            run(
                with(allowed.clone(), asked),
                "Call the MCP tool mcp__probe__echo once. If refused, stop.",
                None,
            );
            assert_eq!(called.exists(), !asked, "an MCP allow rule, {tag}");
        }
        // LANES-17: neither a hook in the settings nor a skill's runs under obid's settings, which
        // outrank the project's `disableAllHooks: false` as they do the user's; and with an own
        // job's `--tools` the skills, the MCP tools and their resources stay.
        let fired = [outside.join("settings-hook"), outside.join("skill-hook")];
        let touch =
            |at: &Path| json!([{"type": "command", "command": format!("touch {}", at.display())}]);
        let settings = json!({"disableAllHooks": false,
            "hooks": {"PreToolUse": [{"matcher": "Read", "hooks": touch(&fired[0])}]}});
        std::fs::write(work.join(".claude/settings.json"), settings.to_string()).unwrap();
        let hooked = work.join(".claude/skills/hooked");
        std::fs::create_dir_all(&hooked).unwrap();
        let skill_hook = format!(
            "---\nname: hooked\ndescription: Reads a file.\nhooks:\n  PreToolUse:\n    - matcher: \"Read\"\n      hooks:\n        - type: command\n          command: \"touch {}\"\n---\nRead the file the user names.\n",
            fired[1].display()
        );
        std::fs::write(hooked.join("SKILL.md"), skill_hook).unwrap();
        std::fs::write(work.join("in.txt"), "in").unwrap();
        let read = "Invoke the hooked skill with the Skill tool, then Read in.txt in this folder.";
        run(json!({}), read, None);
        assert!(
            fired.iter().all(|f| f.exists()),
            "no hook ran: the probe proves nothing"
        );
        fired.iter().for_each(|f| drop(std::fs::remove_file(f)));
        let tools = run(json!({"disableAllHooks": true}), read, Some(OWN_JOB_TOOLS));
        assert!(!fired.iter().any(|f| f.exists()), "a hook ran");
        for tool in ["Skill", "mcp__probe__echo", "ReadMcpResourceTool"] {
            assert!(tools.iter().any(|t| t == tool), "{tool}: {tools:?}");
        }
        assert!(!tools.iter().any(|t| t == "Monitor"), "{tools:?}");
        // The empty memory folder Claude Code makes for its working folder, in the person's own.
        let name: String = (work.canonicalize().unwrap().to_string_lossy().chars())
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let projects = login_home().unwrap().join(".claude/projects");
        let _ = std::fs::remove_dir_all(projects.join(name));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-17: Claude Code's managed settings outrank obid's `disableAllHooks`, so an own run
    /// doesn't start when they'd run a hook (`disableAllHooks: false`, `hooks`, in the managed
    /// file, a drop-in, the organization's remote settings or a managed preferences plist), nor
    /// when one can't be read; with none of them, or only `disableAllHooks: true`, it starts.
    #[test]
    fn lanes_17_managed_settings_that_would_run_hooks_stop_an_own_run() {
        let root = std::env::temp_dir().join(format!("obid-lanes-17-{}", uuid::Uuid::new_v4()));
        let [folder, account] = ["managed", "account"].map(|d| root.join(d));
        let drop_ins = folder.join("managed-settings.d");
        std::fs::create_dir_all(&drop_ins).unwrap();
        std::fs::create_dir_all(&account).unwrap();
        let (file, remote) = (
            folder.join("managed-settings.json"),
            account.join("remote-settings.json"),
        );
        let plist = root.join("com.anthropic.claudecode.plist");
        let plists = [plist.to_str().unwrap()];
        let allowed = || managed_hooks_allowed(&folder, &plists, &remote);
        assert_eq!(allowed(), Ok(()));
        let hook = json!({"PreToolUse": [{"hooks": [{"type": "command", "command": "x"}]}]});
        std::fs::write(
            &file,
            json!({"disableAllHooks": true, "model": "m"}).to_string(),
        )
        .unwrap();
        // Claude Code skips a drop-in whose name starts with a dot.
        std::fs::write(
            drop_ins.join(".off.json"),
            json!({"hooks": hook}).to_string(),
        )
        .unwrap();
        assert_eq!(allowed(), Ok(()));
        let refused = |doc: &Path, set: Value, key: &str| {
            std::fs::write(doc, set.to_string()).unwrap();
            let why = allowed().unwrap_err();
            assert!(
                why.contains(&doc.display().to_string()) && why.contains(key),
                "{why}"
            );
            std::fs::remove_file(doc).unwrap();
        };
        refused(&file, json!({"disableAllHooks": false}), "disableAllHooks");
        refused(
            &drop_ins.join("10-team.json"),
            json!({"hooks": hook}),
            "hooks",
        );
        refused(&remote, json!({"hooks": hook}), "hooks");
        // A drop-in that's a link to a file, as Claude Code reads it.
        let linked = root.join("linked.json");
        std::fs::write(&linked, json!({"hooks": hook}).to_string()).unwrap();
        std::os::unix::fs::symlink(&linked, drop_ins.join("30.json")).unwrap();
        assert!(allowed().unwrap_err().contains("30.json"));
        std::fs::remove_file(drop_ins.join("30.json")).unwrap();
        // MANAGED-1: a plugin managed settings turn on (its hooks still run), a policy helper.
        let plugins = json!({"enabledPlugins": {"off@org": false, "team-policy@org": true}});
        refused(&file, plugins, "enabledPlugins");
        refused(
            &file,
            json!({"policyHelper": {"path": "/usr/local/bin/p"}}),
            "policyHelper",
        );
        refused(
            &drop_ins.join("20.json"),
            json!({"policyHelpers": []}),
            "policyHelpers",
        );
        // Only what runs hooks: a variable named like one, a plugin left off, `allowManagedHooksOnly`
        // (it turns no hook on) and a folder named `*.json` in the drop-ins, which Claude Code
        // skips, don't stop it.
        let env = json!({"env": {"WEBHOOK_URL": "u"}, "enabledPlugins": {"hookify@org": false},
            "allowManagedHooksOnly": true});
        std::fs::write(&file, env.to_string()).unwrap();
        std::fs::create_dir_all(drop_ins.join("folder.json")).unwrap();
        assert_eq!(allowed(), Ok(()));
        std::fs::remove_file(&file).unwrap();
        // One that isn't JSON, or can't be read, stops it too.
        std::fs::write(&file, "{not json").unwrap();
        assert!(allowed().is_err());
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir_all(&file).unwrap();
        assert!(allowed().is_err());
        std::fs::remove_dir(&file).unwrap();
        #[cfg(target_os = "macos")]
        {
            let json = root.join("plist.json");
            std::fs::write(&json, json!({"hooks": hook}).to_string()).unwrap();
            let made = std::process::Command::new("/usr/bin/plutil")
                .args(["-convert", "xml1", "-o"])
                .args([&plist, &json])
                .status()
                .unwrap();
            assert!(made.success());
            let why = allowed().unwrap_err();
            assert!(
                why.contains("com.anthropic.claudecode.plist") && why.contains("hooks"),
                "{why}"
            );
            std::fs::remove_file(&plist).unwrap();
        }
        assert_eq!(allowed(), Ok(()));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-15: an own conversation's file tools never read or change the stored threads,
    /// every conversation's (`conversations/<id>.json`), nor the next one's id.
    #[test]
    fn lanes_15_an_own_conversation_never_reaches_the_stored_threads() {
        let root = std::env::temp_dir().join(format!("obid-lanes-15-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let jobs = own_jobs(&root);
        let conversations = jobs.config.home.join("conversations");
        std::fs::create_dir_all(&conversations).unwrap();
        let denied = jobs.conversation_denials(&conversations.join("c/room"));
        for name in ["*.json", "next-id"] {
            let path = conversations.join(name);
            for tool in ["Read", "Edit"] {
                let rule = format!("{tool}(/{})", path.display());
                assert!(denied.contains(&rule), "{rule}: {denied:?}");
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
