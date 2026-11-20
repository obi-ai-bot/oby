//! Which lane a job's agent runs in (docs/contracts/lanes.md): the owner's work in its CLI's own
//! sandbox on the account's own folder and login, another Oby's or an untrusted job's in Oby's box.

use super::*;

impl Jobs {
    /// A deleted job's worktree and folders, once a sweep ended its own runs' leftovers (LANES-16).
    pub(super) async fn remove_folders(&self, clone: &Path, id: &str) {
        let worktree = self.worktree_path(id);
        let (temp, login) = (self.temp_path(id), self.login_path(id));
        if crate::conn::sweep_records(&temp, &worktree, &login, true) {
            super::remove_worktree(clone, &worktree).await;
            let _ = std::fs::remove_dir_all(temp);
            let _ = std::fs::remove_dir_all(login);
        }
    }

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
    /// with a copy of its login. `claudeOwnSandbox` doesn't change it. A Codex job of the owner's is
    /// own the same way, in every mode and on every OS, with Codex's own sandbox and approvals as
    /// its mode sets them (LANES-18, the owner's "Yes, match T3", `codex::own::policy`), but on a
    /// ChatGPT login ([`launches_own`]). Cursor stays boxed.
    pub(crate) fn own_lane(job: &Job, tool: Tool) -> bool {
        tool != Tool::Cursor
            && job.obi.is_none()
            && !job.untrusted
            && job.review_of.is_none()
            && !(job.read_only() && job.someone_elses().is_some())
    }

    /// Where a run reaches obid's own servers (Oby's tools, Tetrix), from obid's address for agents
    /// (`Connections::api_url`): an own run is on the host, so on loopback, never at `10.0.2.2`,
    /// slirp4netns's host, which only Linux's box reaches (LANES-26). A boxed run, and every run on
    /// macOS, keeps the address as it is.
    pub(crate) fn agent_api(api: &str, own: bool) -> String {
        match api.strip_prefix("http://10.0.2.2:") {
            Some(port) if own => format!("http://127.0.0.1:{port}"),
            _ => api.to_owned(),
        }
    }

    /// Whether `tool` on `account` reaches its model only through obid's relay (a ChatGPT login),
    /// so its run never leaves Oby's box, in any lane.
    pub(crate) fn relayed(tool: Tool, account: &crate::config::Account) -> bool {
        tool == Tool::Codex && crate::chatgpt::marker_of(account).is_some()
    }

    /// Whether job `id`'s agent had a network obid's proxy didn't see: a run of it, or of the job it
    /// follows up, launched in the own lane ([`Job::ran_unproxied`]), or Claude Code or Codex ran it,
    /// now or before a move ([`Job::moves`]), in the own lane, where no proxy marks a site it
    /// reached `reached`. Until a command's sites do (LANES-6), its check gets no declared secrets,
    /// as a job whose agent reached past the package registries gets none. A Codex job on a ChatGPT
    /// login, boxed in any mode (`Jobs::launch`), counts too: fail closed.
    pub(super) fn ran_unproxied(&self, id: &str) -> bool {
        self.get(id).is_some_and(|job| {
            job.ran_unproxied
                || [Tool::Claude, Tool::Codex].into_iter().any(|tool| {
                    let name = tool.name();
                    let ran = job.agent == tool.id()
                        || job
                            .moves
                            .iter()
                            .any(|m| m.from.starts_with(name) || m.to.starts_with(name));
                    ran && Self::own_lane(&job, tool)
                })
        })
    }

    /// Whether job `id`'s own Codex run must end its turn: its mode now lets it go less far than
    /// the one it launched in ([`Job::own_mode`]), so its turn ends and its round goes on under the
    /// new mode's settings (LANES-18: an own run asks obid only what its launch's settings ask, so
    /// nothing else would hold it to Ask or Plan).
    pub(super) fn narrowed_own_run(&self, id: &str, tool: Tool) -> bool {
        let narrowed = |job: Job| {
            let launched = job.own_mode.as_deref().and_then(Mode::parse);
            job.own_lane
                && launched.is_some_and(|launched| {
                    codex::own::policy(launched)
                        .narrowed_by(&codex::own::policy(Self::mode_of(&job)))
                })
        };
        tool == Tool::Codex && self.get(id).is_some_and(narrowed)
    }

    /// Job `id`'s run launches in the lane `own` says, in `mode`, and an own one had no proxy
    /// (LANE-3a): in the job's file before it starts, or it doesn't start.
    pub(super) fn launched(&self, id: &str, own: bool, mode: Mode) -> Result<(), String> {
        let mark = |j: &mut Job| {
            (j.own_lane, j.ran_unproxied) = (own, j.ran_unproxied || own);
            j.own_mode = own.then(|| mode.as_str().to_owned());
        };
        // The job its update saved says so, or an own run doesn't start (gone, or unsaved: unkept).
        if self.update(id, mark).is_some_and(|job| job.ran_unproxied) || !own {
            return Ok(());
        }
        let e = self.unkept(id).unwrap_or_else(|| "the job is gone".into());
        Err(format!(
            "obid couldn't save that this run has no proxy ({e}), so it didn't start."
        ))
    }

    /// LANES-18: a Codex thread is kept in its lane's own `CODEX_HOME` (the account's folder, or
    /// the box's), so a job whose last run was in the other lane than its next goes on in a new
    /// session, told so: what it's told, or none when its session can go on.
    pub(super) fn other_lane(&self, id: &str, tool: Tool) -> Option<String> {
        let job = self.get(id).filter(|job| job.session.is_some())?;
        let (_, account) = self.job_account(&job).ok()?;
        if tool != Tool::Codex || job.own_lane == launches_own(&job, tool, &account) {
            return None;
        }
        self.update(id, |job| {
            job.session = None;
            job.changed("Started a new session: it moved in or out of Oby's box".into());
        });
        Some(continue_message(
            "it now runs in another sandbox, in a new session with none of the last one's history. What was done so far is on this branch: read its log and diff first",
            None,
        ))
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

    /// obid's places an own Claude Code run's file tools never reach, whatever the owner's
    /// settings allow (`own_sandbox_settings`' deny rules): its vault, its keys, its API token, the
    /// runs' login folders and the accounts obid added. No agent gets a vault secret.
    pub(crate) fn secret_places(&self) -> Vec<PathBuf> {
        let home = &self.config.home;
        ["vault", "keys", "api-token", "logins", "accounts"]
            .map(|name| home.join(name))
            .into()
    }

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
        let spec = self.room_spec(room, temp, frozen);
        let mut extra = own_sandbox_env(account, &spec, login)?;
        extra.extend(process::CLAUDE_RUN_SWITCHES.map(|name| (name, "1".into())));
        // Off as `--restricted` had it: what a turn read never lands in the account's memory.
        extra.push((process::NO_AUTO_MEMORY, "1".into()));
        let settings = own_sandbox_settings(&spec, login, &extra, &self.secret_places())?;
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

    /// An own Codex conversation's thread (crate::brain::harness_codex, LANES-13), as an own Codex
    /// job's run (LANES-18): unboxed by obid, on `account`'s own folder as `CODEX_HOME` with the
    /// person's `HOME` ([`own_run_env`]) and the owner's shell's environment, its shell on under
    /// Codex's own sandbox and approvals as `mode` sets them (`codex::own::policy`), in `room`.
    /// `flags` come before `app-server`.
    pub fn conversation_own_codex_launch(
        &self,
        account: &crate::config::Account,
        room: &Path,
        temp: &Path,
        mode: Mode,
        flags: Vec<OsString>,
        stderr: PathBuf,
    ) -> Result<Launch, String> {
        let sandbox =
            (self.sandbox.as_ref()).map_err(|why| format!("obid can't run Codex: {why}"))?;
        let bin = (self.agent_bin(Tool::Codex)).ok_or("obid can't find Codex's binary.")?;
        let spec = self.room_spec(room, temp, Vec::new());
        let mut extra = process::no_publish_env(temp);
        extra.extend(own_run_env(Tool::Codex, account, &spec, temp)?);
        let argv: Vec<OsString> = std::iter::once(bin.into_os_string())
            .chain(
                self.conversation_codex_policy(room, mode)
                    .settings()
                    .into_iter()
                    .map(OsString::from),
            )
            .chain(flags)
            .chain(["app-server".into()])
            .collect();
        let mut env = process::sandbox_env(&process::tools_path(), temp, &extra);
        let owners = process::login_env().map(|env| owners_for(Tool::Codex, env));
        process::with_owners(&mut env, &owners.unwrap_or_default());
        Ok(Launch {
            own_sandbox: true,
            group: None,
            argv: sandbox.unboxed(&argv),
            env,
            cwd: room.to_path_buf(),
            stderr,
        })
    }

    /// What an own conversation's commands reach (LANES-12, LANES-13): they write `room` and
    /// `temp`, but not `frozen` in the room, nor the room's own settings, MCP servers and
    /// instructions; they read nothing of obid's home, the person's or any account's folder.
    fn room_spec(&self, room: &Path, temp: &Path, mut frozen: Vec<PathBuf>) -> Spec {
        let real_room = room.canonicalize().unwrap_or_else(|_| room.to_path_buf());
        frozen.extend(Self::ROOM_CONFIG.map(|name| real_room.join(name)));
        let mut hidden = vec![self.config.home.clone()];
        hidden.extend(person_homes());
        hidden.extend(self.config.agents.hidden_folders(&self.config.home));
        let home = std::slice::from_ref(&self.config.home);
        let read_only = sandbox::tool_roots(&process::tools_path(), &hidden, home);
        Spec {
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
        }
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
        let mut rules = Vec::new();
        for path in self.conversation_secret_places(room) {
            let path = path.to_string_lossy();
            for tool in ["Read", "Edit"] {
                rules.push(format!("{tool}(/{path})"));
                rules.push(format!("{tool}(/{path}/**)"));
            }
        }
        rules
    }

    /// What an own conversation whose room is `room` never reads ([`Self::conversation_denials`]),
    /// as named and where it leads, a `*` matching one name.
    fn conversation_secret_places(&self, room: &Path) -> Vec<PathBuf> {
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
        paths
    }

    /// An own Codex job's settings in `mode` (LANES-18): its commands never read obid's secret
    /// places, the ones an own Claude Code job's file tools are denied ([`Self::secret_places`]),
    /// as named and where they lead (LANES-27; in Bypass Codex has no sandbox to deny them in).
    pub(crate) fn own_codex_policy(&self, mode: Mode) -> codex::own::Policy {
        let places = self.secret_places();
        let real = places.iter().filter_map(|p| p.canonicalize().ok());
        let all: Vec<PathBuf> = places.iter().cloned().chain(real).collect();
        codex::own::policy(mode).denying(codex_denied(&all))
    }

    /// An own Codex conversation's settings in `mode`, its room `room` (LANES-13): its commands
    /// never read what an own Claude Code conversation's file tools are denied (LANES-27).
    pub(crate) fn conversation_codex_policy(&self, room: &Path, mode: Mode) -> codex::own::Policy {
        let places = self.conversation_secret_places(room);
        codex::own::policy(mode).denying(codex_denied(&places))
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

/// `places` as an own Codex run's profile denies them (LANES-27): each path, and everything under
/// it, which Codex's sandbox needs said for a name with a `*` in it (codex-cli 0.163.0, measured).
fn codex_denied(places: &[PathBuf]) -> Vec<String> {
    (places.iter().map(|p| p.to_string_lossy()))
        .flat_map(|p| [p.to_string(), format!("{p}/**")])
        .collect()
}

/// Whether `job`'s run of `tool` on `account` leaves Oby's box: in the own lane, but never a Codex
/// run on a ChatGPT login, which reaches OpenAI only through obid's relay.
pub(super) fn launches_own(job: &Job, tool: Tool, account: &crate::config::Account) -> bool {
    Jobs::own_lane(job, tool) && !Jobs::relayed(tool, account)
}

/// What an own run of `tool` on `account` gets over a boxed run's environment: Claude Code's
/// ([`own_sandbox_env`], nothing of the owner's refused, LANES-22); Codex's, the account's
/// own folder as `CODEX_HOME` (its `configDir`, else `~/.codex`), with no login copy: Codex reads
/// and refreshes its own login, config, skills and MCP servers there (LANES-18), and the person's
/// own `HOME`, as Claude Code's: Codex and the MCP servers it starts run outside its sandbox, so
/// never with a `HOME` its commands write.
pub(super) fn own_run_env(
    tool: Tool,
    account: &crate::config::Account,
    spec: &Spec,
    login: &Path,
) -> Result<Vec<(&'static str, OsString)>, String> {
    if tool != Tool::Codex {
        return own_sandbox_env(account, spec, login);
    }
    let home = login_home().ok_or("obid can't find the home Codex's login is in.")?;
    let folder = account.config_dir.clone().unwrap_or(home.join(".codex"));
    Ok(vec![
        ("CODEX_HOME", folder.into_os_string()),
        ("HOME", home.clone().into_os_string()),
        ("CFFIXED_USER_HOME", home.into_os_string()),
    ])
}

/// The owner's login shell's environment (`env`, LANES-4) for an own run of `tool`: Claude Code's
/// gateway (`ANTHROPIC_*`, its `CLAUDE_CODE_*` switches) is Claude Code's alone, so a Codex run gets
/// no `CLAUDE*` or `ANTHROPIC_*` of it, as no other agent does (`process::agent_withheld`).
pub(super) fn owners_for(tool: Tool, env: Vec<(OsString, OsString)>) -> Vec<(OsString, OsString)> {
    let claudes = |name: &OsString| {
        let name = name.to_string_lossy();
        name.starts_with("CLAUDE") || name.starts_with("ANTHROPIC_")
    };
    env.into_iter()
        .filter(|(name, _)| tool == Tool::Claude || !claudes(name))
        .collect()
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

    /// LANES-26 (docs/contracts/lanes.md): an own run reaches obid's servers on loopback; a boxed
    /// one keeps the box's host address, and macOS's loopback address stays as it is.
    #[test]
    fn lanes_26_an_own_run_reaches_obid_on_loopback() {
        let linux = "http://10.0.2.2:7777";
        assert_eq!(Jobs::agent_api(linux, true), "http://127.0.0.1:7777");
        assert_eq!(
            Jobs::agent_api(&format!("{linux}/v1/agent/mcp/c"), true),
            "http://127.0.0.1:7777/v1/agent/mcp/c"
        );
        assert_eq!(Jobs::agent_api(linux, false), linux);
        let mac = "http://127.0.0.1:7777";
        assert_eq!(Jobs::agent_api(mac, true), mac);
        assert_eq!(Jobs::agent_api(mac, false), mac);
    }

    /// LANES-1 (docs/contracts/lanes.md): another Oby's job, one marked `untrusted` and Oby's own
    /// review are foreign; one marked only `foreign` or `reached` is the owner's, and so own.
    /// Cursor stays boxed for now, and Codex but in Bypass (LANES-18).
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
    /// keep its file tools off `repos/`, the room's own settings, MCP servers and CLAUDE.md, and
    /// obid's secret places, on the account's own folder with no proxy of obid's and its
    /// auto-memory off.
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
        let deny = &settings["permissions"]["deny"];
        // repos/ and the room's own settings: neither its commands nor its tools write them.
        let mut frozen = vec![repos.clone()];
        frozen.extend(Jobs::ROOM_CONFIG.map(|name| room.join(name)));
        for path in &frozen {
            let path = path.to_str().unwrap();
            assert!(listed(deny, &format!("Edit(/{path}/**)")), "{path}: {deny}");
        }
        // Its `login` folder, with the MCP configuration Claude Code starts servers from outside
        // any box: its Edit and Write tools never write there.
        assert!(
            listed(deny, &format!("Edit(/{}/**)", login.display())),
            "{deny}"
        );
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
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-23 (docs/contracts/lanes.md, the owner's decision of 2026-11-16): an own
    /// conversation's Claude Code starts whatever the owner's user settings hold (hooks, more
    /// folders, commands out of its sandbox, a bare `Read` allow, any `CLAUDE_*` in their `env`),
    /// and obid's `--settings` turn none of the owner's sandbox, hooks or permissions off or on:
    /// they only add deny rules for obid's secret places and pin obid's own variables.
    #[cfg(target_os = "macos")]
    #[test]
    fn lanes_23_an_own_conversation_starts_whatever_the_owners_settings_hold() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-lanes-22-{}", uuid::Uuid::new_v4()));
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
        let [room, temp, login] = ["room", "home", "login"].map(|d| place.join(d));
        for dir in [&room, &temp] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let owners = json!({
            "hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "true"}]}]},
            "permissions": {"allow": ["Read", "Edit(//etc/**)"],
                            "additionalDirectories": ["/tmp"]},
            "sandbox": {"enabled": false, "excludedCommands": ["git"]},
            "env": {"CLAUDE_CODE_MANAGED_SETTINGS_PATH": "/tmp/x.json", "CLAUDE_FOO": "1"},
        });
        std::fs::write(account.join("settings.json"), owners.to_string()).unwrap();
        let claude = jobs.accounts(Tool::Claude).remove(0);
        let launch = jobs
            .conversation_own_launch(
                &claude,
                &room,
                &temp,
                &login,
                Vec::new(),
                vec!["-p".into()],
                place.join("agent.log"),
            )
            .unwrap();
        let argv: Vec<String> = launch
            .argv
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let at = argv.iter().position(|a| a == "--settings").unwrap();
        let settings: Value = serde_json::from_str(&argv[at + 1]).unwrap();
        for key in ["sandbox", "disableAllHooks", "hooks"] {
            assert!(settings[key].is_null(), "{key}: {settings}");
        }
        assert!(settings["permissions"]["ask"].is_null(), "{settings}");
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        let vault = format!("Read(/{}/vault/**)", home.display());
        assert!(
            deny.iter().any(|r| r.as_str() == Some(&vault)),
            "{settings}"
        );
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
        // An own run starts only for a job obid keeps, its no-proxy mark saved (LANE-3a).
        jobs.insert(job.clone()).unwrap();
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
        // No proxy saw what it reached, so its check gets no declared secrets, nor does an own
        // Codex job's, in any mode (LANES-18); a boxed job's is judged as before.
        let mut boxed = job.clone();
        boxed.id = "boxed".into();
        boxed.untrusted = true;
        let mut codex = job.clone();
        codex.id = "codex".into();
        codex.agent = "codex".into();
        codex.mode = Some("ask".into());
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
        assert!(jobs.ran_unproxied("codex"));
        spec.accounts.push(root.join("account"));
        let check = jobs.check_spec("own", &spec);
        assert!(!check.network);
        assert!(!check.proxied);
        assert!(check.loopback);
        assert!(check.accounts.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-18 (the owner's "Yes, match T3"): the owner's Codex job leaves Oby's box in every mode
    /// and of every kind, a build too, and so does the run a plan card's answer resumes; as a
    /// review, another Oby's or marked `untrusted` it stays boxed. On every OS.
    #[test]
    fn lanes_18_the_owners_codex_job_runs_own_in_every_mode() {
        let owners = Job::new(
            "j".into(),
            "o/r".into(),
            "investigate".into(),
            "Look into it".into(),
            "codex".into(),
            "a".into(),
        );
        let lane = |mark: &dyn Fn(&mut Job)| {
            let mut job = owners.clone();
            mark(&mut job);
            Jobs::own_lane(&job, Tool::Codex)
        };
        assert!(lane(&|_| {}));
        for mode in ["bypass", "ask", "acceptEdits", "plan", "sandbox"] {
            assert!(lane(&|job| job.mode = Some(mode.into())), "{mode}");
        }
        for kind in COMMITTING {
            assert!(lane(&|job| job.kind = kind.into()), "{kind}");
        }
        assert!(lane(&|job| {
            job.parked = Some(permissions::Parked {
                tool: "ExitPlanMode".into(),
                input: json!({"plan": "Plan: add plan.txt"}),
                answered: true,
                allowed: true,
                ended_turn: true,
            })
        }));
        // MERGING-36: the owner's review is own until its code is someone else's.
        assert!(lane(&|job| job.kind = "review".into()));
        assert!(!lane(&|job| job.untrusted = true));
        assert!(!lane(&|job| job.review_of = Some("k".into())));
        assert!(!lane(&|job| {
            job.obi = Some(crate::job::ObiRef {
                correlation_id: "c".into(),
                from: "obi-ernesto".into(),
                from_owner: "ernesto".into(),
                request_id: "r".into(),
            })
        }));
        assert!(!Jobs::own_lane(&owners, Tool::Cursor));
    }

    /// LANES-18: of the owner's login shell's environment, an own Codex run gets everything Claude
    /// Code's gets but Claude Code's gateway, which is Claude Code's alone.
    #[test]
    fn lanes_18_an_own_codex_run_gets_none_of_claude_codes_gateway() {
        let env: Vec<(OsString, OsString)> = [
            ("ANTHROPIC_BASE_URL", "https://gateway.example.com"),
            ("CLAUDE_CODE_USE_BEDROCK", "1"),
            ("OPENAI_BASE_URL", "https://openai.example.com"),
        ]
        .map(|(n, v)| (n.into(), v.into()))
        .into();
        assert_eq!(owners_for(Tool::Codex, env.clone()), env[2..]);
        assert_eq!(owners_for(Tool::Claude, env.clone()), env);
    }

    /// LANES-18 (the owner's "Yes, match T3"): an own Codex job runs unboxed by obid, with no proxy
    /// of obid's, on its account's own folder with no login copy and the person's own `HOME`, under
    /// Codex's own sandbox and approvals as its mode sets them (Bypass: no sandbox, Codex asks
    /// nobody; Ask: a sandbox that writes nothing) and nothing of obid's: no profile, trust, hooks
    /// switch, `notify` or commands' environment. A run on a ChatGPT login stays boxed. LANE-3a: its
    /// check gets no declared secrets, and neither does it once its mode is Ask, nor its follow-up
    /// in Ask: the run that had no proxy is remembered. A mode narrower than its launch's ends it.
    #[tokio::test]
    async fn lanes_18_an_own_codex_job_runs_on_its_modes_settings() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-lanes-18-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let session = root.join("session");
        std::fs::write(&session, "#!/bin/sh\nshift 2\nexec \"$@\"\n").unwrap();
        std::fs::set_permissions(&session, std::fs::Permissions::from_mode(0o755)).unwrap();
        let [account, other] = ["account", "other"].map(|d| root.join(d));
        for dir in [&account, &other] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let config = crate::config::Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": root.join("home"), "owner": "o",
                "repos": [{"repo": "o/r", "remote": "https://example.com/o/r.git"}],
                "agents": {"codex": {"bin": "/usr/bin/true",
                    "accounts": [{"id": "a", "name": "A", "configDir": account},
                                 {"id": "b", "name": "B", "configDir": other}]}}})
            .to_string(),
        )
        .unwrap();
        let open = || {
            let (events, sandbox) = (crate::events::Events::new(), Sandbox::detect(&session));
            Jobs::open(config.clone(), events, sandbox).unwrap()
        };
        let jobs = open();
        let job = Job::new(
            "own".into(),
            "o/r".into(),
            "investigate".into(),
            "Look into it".into(),
            "codex".into(),
            "a".into(),
        );
        let asks_as = |id: &str| Job {
            id: id.into(),
            mode: Some("ask".into()),
            ..job.clone()
        };
        let asks = asks_as("asks");
        let mut unsaved = job.clone();
        unsaved.id = "unsaved".into();
        for job in [&job, &asks, &unsaved] {
            jobs.lock().jobs.insert(job.id.clone(), job.clone());
        }
        let [worktree, admin, common, temp] =
            ["worktree", "admin", "common", "temp"].map(|d| root.join(d));
        for dir in [&worktree, &admin, &common, &temp] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let spec = jobs.spec(&worktree, &admin, &common, &temp, true, Tool::Codex);
        let [codex, chatgpt] = <[_; 2]>::try_from(jobs.accounts(Tool::Codex)).unwrap();
        // Every Codex job of the owner's counts as having had no proxy, as a Claude Code one does.
        assert!(jobs.ran_unproxied("asks"));
        std::fs::write(
            other.join(crate::chatgpt::MARKER),
            r#"{"signed":true,"planUse":true,"expiresAt":0,"refreshedAt":0}"#,
        )
        .unwrap();
        assert!(launches_own(&job, Tool::Codex, &codex));
        assert!(!launches_own(&job, Tool::Codex, &chatgpt));
        let launch = jobs
            .launch(&job, &spec, Tool::Codex, Vec::new(), &codex, None)
            .unwrap();
        assert!(launch.own_sandbox);
        let argv: Vec<String> = launch
            .argv
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        // In a session of its own and no box of obid's: Codex itself follows the session's `--`.
        assert_eq!(
            argv[1..4],
            [sandbox::SESSION, "--", "/usr/bin/true"],
            "{argv:?}"
        );
        let set = |key: &str| {
            argv.windows(2)
                .find(|w| w[0] == "-c" && w[1].starts_with(&format!("{key}=")))
                .map(|w| w[1][key.len() + 1..].to_owned())
        };
        // Its mode's settings (Bypass: T3's full access) and nothing of obid's own: no profile,
        // trust, hooks, `notify` or environment for its commands.
        let bypass = codex::own::policy(Mode::Bypass).settings();
        assert_eq!(
            set("approval_policy").as_deref(),
            bypass[1].strip_prefix("approval_policy=")
        );
        assert_eq!(
            set("default_permissions").as_deref(),
            Some("\":danger-full-access\"")
        );
        for gone in [
            "sandbox_mode",
            "features.hooks",
            "notify",
            "projects",
            "shell_environment_policy.set",
            "permissions.oby_own.filesystem",
        ] {
            assert_eq!(set(gone), None, "{gone}: {argv:?}");
        }
        assert!(!argv.iter().any(|a| a.contains("oby_own")), "{argv:?}");
        // The same job in Ask is own too, on Ask's settings: a sandbox that writes nothing, in
        // obid's profile, whose commands never read obid's secret places, as named and where they
        // lead (LANES-27).
        let asking = jobs
            .launch(&asks, &spec, Tool::Codex, Vec::new(), &codex, None)
            .unwrap();
        assert!(asking.own_sandbox);
        let asked: Vec<String> = (asking.argv.iter())
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        for want in [
            "default_permissions=\"oby\"",
            "permissions.oby.extends=\":read-only\"",
        ] {
            assert!(asked.iter().any(|a| a == want), "{want}: {asked:?}");
        }
        let files = (asked.iter())
            .find_map(|a| a.strip_prefix("permissions.oby.filesystem="))
            .unwrap();
        for place in jobs.secret_places() {
            let named = json!(place.to_str().unwrap()).to_string();
            assert!(
                files.contains(&format!("{named} = \"none\"")),
                "{named}: {files}"
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
        // The account's own folder, no login copy of obid's; Codex's own `HOME` the person's.
        assert_eq!(env("CODEX_HOME"), Some(account.clone().into_os_string()));
        for home in ["HOME", "CFFIXED_USER_HOME"] {
            assert_eq!(env(home), login_home().map(PathBuf::into_os_string));
        }
        assert!(!jobs.login_path("own").join("auth.json").exists());
        for (name, value) in &launch.env {
            if name.to_string_lossy().ends_with("_PROXY") {
                assert_eq!(std::env::var_os(name).as_ref(), Some(value), "{name:?}");
            }
        }
        // No proxy saw it, and no mode it has later forgets that; its follow-up runs its code.
        assert!(jobs.ran_unproxied("own"));
        // Nor does a restart: a snapshot from before the launch (a mode change's, in Ask), saved
        // after it, doesn't undo what its file says, and a run whose mark can't be saved doesn't start.
        jobs.store.save(&asks_as("own")).unwrap();
        assert!(open().ran_unproxied("own"));
        let kept = jobs.config.home.join("jobs");
        std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o500)).unwrap();
        let refused = jobs.launch(&unsaved, &spec, Tool::Codex, Vec::new(), &codex, None);
        std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(refused.is_err_and(|why| why.contains("has no proxy")));
        // In Ask it's still own, and its run launched in Bypass ends its turn.
        assert!(!jobs.narrowed_own_run("own", Tool::Codex));
        jobs.set_mode("own", Mode::Ask).unwrap();
        assert!(Jobs::own_lane(&jobs.get("own").unwrap(), Tool::Codex));
        assert!(jobs.narrowed_own_run("own", Tool::Codex));
        assert!(jobs.ran_unproxied("own"));
        let next = jobs
            .create(NewJob {
                vetted: true,
                ..follow_up(&jobs.get("own").unwrap(), "and the docs")
            })
            .unwrap();
        assert_eq!(Jobs::mode_of(&next), Mode::Ask);
        assert!(jobs.ran_unproxied(&next.id));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-13 (the owner's "Yes, match T3"): an own Codex conversation's thread runs as an own
    /// Codex job's (LANES-18), by the same settings: unboxed, on the account's own folder with the
    /// person's `HOME` and no login copy, in its room, under Codex's own sandbox and approvals as
    /// the mode sets them and nothing of obid's; `app-server` last, after the conversation's own
    /// settings.
    #[test]
    fn lanes_13_an_own_codex_conversation_runs_on_its_modes_settings() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-lanes-13-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let session = root.join("session");
        std::fs::write(&session, "#!/bin/sh\nshift 2\nexec \"$@\"\n").unwrap();
        std::fs::set_permissions(&session, std::fs::Permissions::from_mode(0o755)).unwrap();
        let account = root.join("account");
        let home = root.join("home");
        let place = home.join("conversations/c1");
        let [room, temp] = ["room", "home"].map(|d| place.join(d));
        let repos = room.join("repos");
        for dir in [&account, &temp, &repos] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let config = crate::config::Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": "o", "repos": [],
                "agents": {"codex": {"bin": "/usr/bin/true",
                    "accounts": [{"id": "a", "name": "A", "configDir": account}]}}})
            .to_string(),
        )
        .unwrap();
        let (events, sandbox) = (crate::events::Events::new(), Sandbox::detect(&session));
        let jobs = Jobs::open(config, events, sandbox).unwrap();
        let codex = jobs.accounts(Tool::Codex).remove(0);
        let flags = vec!["-c".into(), "features.apps=false".into()];
        let stderr = place.join("agent.log");
        let launch = jobs
            .conversation_own_codex_launch(&codex, &room, &temp, Mode::AcceptEdits, flags, stderr)
            .unwrap();
        assert!(launch.own_sandbox);
        assert_eq!(launch.cwd, room);
        let argv: Vec<String> = (launch.argv.iter())
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(argv[1..4], [sandbox::SESSION, "--", "/usr/bin/true"]);
        assert_eq!(
            argv[argv.len() - 3..],
            ["-c", "features.apps=false", "app-server"]
        );
        let set = |key: &str| {
            (argv.windows(2))
                .find(|w| w[0] == "-c" && w[1].starts_with(&format!("{key}=")))
                .map(|w| w[1][key.len() + 1..].to_owned())
        };
        assert_eq!(set("approval_policy").as_deref(), Some("\"on-request\""));
        assert_eq!(set("default_permissions").as_deref(), Some("\"oby\""));
        assert_eq!(
            set("permissions.oby.extends").as_deref(),
            Some("\":workspace\"")
        );
        // LANES-27: never obid's vault, nor any conversation's login.
        let files = set("permissions.oby.filesystem").unwrap();
        for denied in [
            "/vault\" = \"none\"",
            "/conversations/*/login/**\" = \"none\"",
        ] {
            assert!(files.contains(denied), "{denied}: {files}");
        }
        for gone in [
            "features.hooks",
            "notify",
            "projects",
            "shell_environment_policy.set",
        ] {
            assert_eq!(set(gone), None, "{gone}: {argv:?}");
        }
        assert!(!argv.iter().any(|a| a.contains("oby_own")), "{argv:?}");
        let env = |name: &str| {
            (launch.env.iter().rev())
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(env("CODEX_HOME"), Some(account.into_os_string()));
        assert_eq!(env("HOME"), login_home().map(PathBuf::into_os_string));
        assert!(!place.join("config").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-18: an own Codex run whose mode narrows (from the one it launched in) ends its turn and
    /// goes on in the own lane, in its own session. A Codex job whose lane changed (a mark arrived:
    /// its next run is boxed) can't pick its thread up from the account's `CODEX_HOME` in the box,
    /// nor a boxed one from the box's: its next run starts a new session, told why.
    #[test]
    fn lanes_18_a_codex_job_that_changes_lane_starts_a_new_session() {
        let root = std::env::temp_dir().join(format!("obid-lanes-18l-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let config = crate::config::Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": root.join("home"), "owner": "o",
                "agents": {"codex": {"bin": "/usr/bin/true",
                    "accounts": [{"id": "a", "name": "A", "configDir": root.join("account")}]}}})
            .to_string(),
        )
        .unwrap();
        let sandbox = Sandbox::detect(&root.join("session"));
        let jobs = Jobs::open(config, crate::events::Events::new(), sandbox).unwrap();
        let mut job = Job::new(
            "j".into(),
            "o/r".into(),
            "investigate".into(),
            "Look into it".into(),
            "codex".into(),
            "a".into(),
        );
        (job.session, job.own_lane) = (Some("t1".into()), true);
        job.own_mode = Some("acceptEdits".into());
        jobs.lock().jobs.insert(job.id.clone(), job);
        // Bypass lets it go further: it goes on. Plan less far: its turn ends, its session stays.
        assert!(!jobs.narrowed_own_run("j", Tool::Codex));
        jobs.set_mode("j", Mode::Bypass).unwrap();
        assert!(!jobs.narrowed_own_run("j", Tool::Codex));
        jobs.set_mode("j", Mode::Plan).unwrap();
        assert!(jobs.narrowed_own_run("j", Tool::Codex));
        assert!(!jobs.narrowed_own_run("j", Tool::Claude));
        assert_eq!(jobs.other_lane("j", Tool::Codex), None);
        // Marked untrusted, its next run is boxed: a new session, told why.
        jobs.update("j", |job| job.untrusted = true);
        assert_eq!(jobs.other_lane("j", Tool::Claude), None);
        let told = jobs.other_lane("j", Tool::Codex).unwrap();
        assert!(told.contains("another sandbox"), "{told}");
        let job = jobs.get("j").unwrap();
        assert_eq!(job.session, None);
        assert!(job.changes.iter().any(|c| c.what.contains("new session")));
        // A boxed run's session can't go on in the own lane either.
        jobs.update("j", |job| {
            (job.session, job.own_lane, job.untrusted) = (Some("t2".into()), false, false)
        });
        assert!(!jobs.narrowed_own_run("j", Tool::Codex));
        assert!(jobs.other_lane("j", Tool::Codex).is_some());
        assert_eq!(jobs.other_lane("j", Tool::Codex), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-25: a job's agent is started for that job alone. Two own Codex jobs on one account get
    /// launches that share nothing a CLI fixes as it starts: each runs in its own worktree, names
    /// only its own folders in its arguments and environment (its git folders, its `HOME`), and
    /// reaches Oby's tools with
    /// its own broker token. So no Codex started for one could be given the other.
    #[cfg(target_os = "macos")]
    #[test]
    fn lanes_25_a_jobs_agent_is_started_for_that_job_alone() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-lanes-25-{}", uuid::Uuid::new_v4()));
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
                "agents": {"codex": {"bin": "/usr/bin/true",
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
        let codex = jobs.accounts(Tool::Codex).remove(0);
        let launch = |id: &str| {
            let job = Job::new(
                id.into(),
                "o/r".into(),
                "investigate".into(),
                "Look into it".into(),
                "codex".into(),
                "a".into(),
            );
            jobs.insert(job.clone()).unwrap();
            let folder = root.join(id);
            let [worktree, admin, common, temp] =
                ["worktree", "admin", "common", "temp"].map(|d| folder.join(d));
            for dir in [&worktree, &admin, &common, &temp] {
                std::fs::create_dir_all(dir).unwrap();
            }
            let spec = jobs.spec(&worktree, &admin, &common, &temp, true, Tool::Codex);
            let broker = (
                format!("http://127.0.0.1:1/v1/agent/mcp/{id}"),
                format!("token-{id}"),
            );
            let launch = jobs
                .launch(&job, &spec, Tool::Codex, Vec::new(), &codex, Some(&broker))
                .unwrap();
            assert!(launch.own_sandbox, "{id} runs in the own lane");
            (launch, worktree, folder)
        };
        let (one, one_worktree, one_folder) = launch("one");
        let (two, two_worktree, two_folder) = launch("two");
        assert_eq!(one.cwd, one_worktree);
        assert_eq!(two.cwd, two_worktree);
        let named = |launch: &Launch| -> Vec<String> {
            let argv = launch.argv.iter().map(|a| a.to_string_lossy().into_owned());
            let env = launch
                .env
                .iter()
                .map(|(_, v)| v.to_string_lossy().into_owned());
            argv.chain(env).collect()
        };
        for (launch, own, other, id) in [
            (&one, &one_folder, &two_folder, "one"),
            (&two, &two_folder, &one_folder, "two"),
        ] {
            let all = named(launch);
            let mine = |p: &Path| all.iter().any(|v| v.contains(p.to_str().unwrap()));
            assert!(mine(own), "{id}'s launch names its own folders: {all:?}");
            assert!(
                !mine(other),
                "{id}'s launch names another job's folder: {all:?}"
            );
            let token = launch
                .env
                .iter()
                .find(|(name, _)| name == "OBY_MCP_TOKEN")
                .map(|(_, v)| v.to_string_lossy().into_owned());
            assert_eq!(token.as_deref(), Some(format!("token-{id}").as_str()));
        }
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

    /// LANES-14: an own job's Claude Code has only the built-in tools obid answers (`--tools`), its
    /// MCP servers' resources among them, never one obid doesn't know; and (LANES-17, LANES-23)
    /// obid's `--settings` leave the owner's hooks and sandbox as they set them.
    #[cfg(target_os = "macos")]
    #[test]
    fn lanes_17_an_own_job_has_only_the_tools_obid_answers() {
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
        // An own run starts only for a job obid keeps, its no-proxy mark saved (LANE-3a).
        jobs.insert(job.clone()).unwrap();
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
        for key in ["disableAllHooks", "sandbox"] {
            assert!(settings[key].is_null(), "{key}: {settings}");
        }
        // LANES-16, RUNTAG-1a: the run is still tagged, for an owner's sandbox to keep it in.
        let tagged = launch
            .env
            .iter()
            .any(|(name, _)| name == crate::process::RUN_TAG);
        assert!(tagged, "no run tag in its env");
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

    /// LANES-16, measured on this Mac's real Claude Code: a command in its own sandbox, with an own
    /// run's temporary folder and `env.sh`, leaves a `setsid` process; obid's sweep finds and ends
    /// it, so Claude Code's real sandbox matches what `end_sandboxed` looks for. Nothing of the
    /// login is read or printed. `cargo test -p obid --lib lanes_16_a_real -- --ignored`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "runs the real Claude Code on this Mac's own login"]
    fn lanes_16_a_real_leftover_ends_with_its_run() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("obid-lanes-16r-{}", uuid::Uuid::new_v4()));
        let [work, temp, login] = ["work", "temp", "login"].map(|d| root.join(d));
        for dir in [&work, &temp, &login] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let [work, temp, login] = [work, temp, login].map(|d| d.canonicalize().unwrap());
        let env_file = login.join("env.sh");
        std::fs::write(&env_file, "").unwrap();
        // Its tag, as obid makes one (crate::jobs, `own_sandbox_env`).
        let tag = temp.with_file_name("temp.run-test");
        std::fs::write(&tag, "").unwrap();
        let pid_file = temp.join("pid");
        // The leftover, killed however the test ends (crate::process::tests::Started).
        let mut started = crate::process::tests::Started {
            children: Vec::new(),
            file: Some(pid_file.clone()),
        };
        let settings = json!({"sandbox": {"enabled": true, "failIfUnavailable": true,
            "allowUnsandboxedCommands": false,
            "filesystem": {"allowWrite": [temp, tag], "denyWrite": [login]}}});
        let mut command = std::process::Command::new("claude");
        command
            .current_dir(&work)
            .args(["-p", "--no-session-persistence", "--setting-sources", ""])
            .args([
                "--strict-mcp-config",
                "--permission-mode",
                "default",
                "--model",
                "sonnet",
            ])
            .args([
                "--allowedTools",
                "Bash",
                "--settings",
                &settings.to_string(),
            ])
            .args(["--tools", "Bash"])
            .env(crate::process::CLAUDE_TMPDIR, &temp)
            .env(crate::process::RUN_TAG, &tag)
            .env("CLAUDE_ENV_FILE", &env_file)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null());
        for (name, _) in std::env::vars() {
            if name.starts_with("CLAUDE") || name.starts_with("ANTHROPIC_") {
                command.env_remove(name);
            }
        }
        let mut child = command.spawn().unwrap();
        let prompt = format!(
            "Run exactly this one command with the Bash tool, not in the background, then stop: perl -e 'use POSIX; setsid(); exec q(sleep), 60' > /dev/null 2>&1 & echo $! > {}",
            pid_file.display()
        );
        child
            .stdin
            .take()
            .unwrap()
            .write_all(prompt.as_bytes())
            .unwrap();
        // Claude Code itself is the guard's too, and waited for two minutes at most.
        started.children.push(child);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while started.children[0].try_wait().unwrap().is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "Claude Code didn't finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let alive = || nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok();
        assert!(alive(), "no leftover: the probe proves nothing");
        assert!(crate::process::end_sandboxed(&temp, &work, &login, &tag).unwrap() >= 1);
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            !alive(),
            "Claude Code's own sandbox doesn't match obid's sweep"
        );
        let name: String = (work.to_string_lossy().chars())
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let projects = login_home().unwrap().join(".claude/projects");
        let _ = std::fs::remove_dir_all(projects.join(name));
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
