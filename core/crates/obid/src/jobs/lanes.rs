//! Which lane a job's agent runs in (docs/contracts/lanes.md): the owner's work in its CLI's own
//! sandbox on the account's own folder and login, another Oby's or an untrusted job's in Oby's box.

use super::*;

impl Jobs {
    /// Whether `job`'s `tool` run is in the own lane (LANES-1): every Claude Code and Codex job of
    /// the owner's, unless another Oby asked for it ([`Job::obi`]) or it's marked untrusted
    /// ([`Job::untrusted`]); the other marks don't move it (the owner's decision of 2026-11-09).
    /// Oby's own review stays boxed, as `review_may_run` expects, and so does a Codex account obid
    /// signed in to ChatGPT (`Jobs::launch`: no run gets its tokens). A Claude Code run gets
    /// [`own_sandbox_env`] and [`own_sandbox_settings`], a Codex one [`Jobs::own_codex`], and
    /// neither an egress proxy (LANES-3, LANES-6, LANES-7); every other run is boxed with a copy
    /// of its login. `claudeOwnSandbox` doesn't change it.
    pub(crate) fn own_lane(job: &Job, tool: Tool) -> bool {
        tool != Tool::Cursor && job.obi.is_none() && !job.untrusted && job.review_of.is_none()
    }

    /// What an own Codex run gets in `env` (LANES-7): the account's own folder as its `CODEX_HOME`
    /// (`~/.codex` for an account with no `configDir`), its login there as it is, which Codex
    /// refreshes: no copy, no refresh token taken out. git is pinned to the job's own git folders,
    /// as an own Claude Code run's is ([`own_sandbox_env`]), whatever a command writes in the
    /// worktree's. Returns the `-c` settings of its sandbox (the thread asks for `workspace-write`,
    /// crate::codex): no network, whatever the account's `config.toml` says, since a command can
    /// read the login's refresh token there; writable, besides the worktree and its `TMPDIR`, only
    /// the git folders a commit writes (the worktree's own, the clone's objects and refs) and
    /// `/private/tmp`.
    pub(super) fn own_codex(
        account: &crate::config::Account,
        spec: &Spec,
        env: &mut Vec<(&'static str, OsString)>,
    ) -> Result<Vec<OsString>, String> {
        let home = account.config_dir.clone();
        let home = home
            .or_else(|| login_home().map(|home| home.join(".codex")))
            .ok_or("obid can't find Codex's own folder.")?;
        let real = spec.canonical();
        let (Some(admin), Some(common)) = (real.git_admin, real.git_common) else {
            return Err("obid has no git folder for this job's Codex.".into());
        };
        let roots = [
            &real.worktree,
            &admin,
            &common.join("objects"),
            &common.join("refs"),
            Path::new("/private/tmp"),
        ];
        let roots: Option<Vec<&str>> = roots.iter().map(|root| root.to_str()).collect();
        let roots = roots.ok_or("obid can't name this job's folders to Codex's sandbox.")?;
        let settings = [
            format!("sandbox_workspace_write.writable_roots={}", json!(roots)),
            "sandbox_workspace_write.network_access=false".into(),
        ];
        let settings = settings.into_iter().flat_map(|s| ["-c".into(), s.into()]);
        let settings = settings.collect();
        env.extend([
            ("CODEX_HOME", home.into_os_string()),
            ("GIT_DIR", admin.into_os_string()),
            ("GIT_COMMON_DIR", common.into_os_string()),
            ("GIT_WORK_TREE", real.worktree.into_os_string()),
        ]);
        Ok(settings)
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
    /// Codex's is own too (LANES-7); Cursor stays boxed.
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
        assert!(Jobs::own_lane(&owners, Tool::Codex));
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
