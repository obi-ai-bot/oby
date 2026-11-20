//! The person's own `ssh`, as a tunnel and to run `obid pair-computer`, nothing else
//! (docs/contracts/remote.md, COMPUTERS-4). Oby never parses `~/.ssh/config` (ssh reads it, and
//! `ssh -G` says what it made of an alias), never asks for a password or a code (keys and the
//! agent only), never accepts a host key (the person's `known_hosts` decides), and installs
//! nothing over it. The classes of T3 Code's SSH fixes this avoids by design are in
//! tools/obid-spec/workflows/lab/remote/README.md, "What's portable".

use std::path::PathBuf;
use std::process::Stdio;

use serde::Serialize;

#[derive(Clone, Debug)]
pub struct Ssh {
    pub program: PathBuf,
    /// `-F`: only for a test. In use it's `None`, and ssh reads the person's own configuration.
    pub config: Option<PathBuf>,
    /// The environment ssh runs with; `None` keeps obid's own.
    pub env: Option<Vec<(String, String)>>,
}

/// A destination as the person typed it: an alias or `user@host`. Never an option (a leading
/// `-` is how `ssh -oProxyCommand=…` gets in), never whitespace or a control character.
pub fn check_destination(destination: &str) -> Result<(), String> {
    let ok = !destination.is_empty()
        && destination.len() <= 255
        && !destination.starts_with('-')
        && destination
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-@:[]%".contains(c));
    if ok {
        Ok(())
    } else {
        Err(format!("{destination:?} isn't an SSH host or alias"))
    }
}

impl Ssh {
    /// What every call takes: no password or code prompt, no alias's RemoteCommand or TTY taking
    /// the session over, no shared ControlMaster (a slow reconnect never replaces a healthy
    /// forward, and nothing outlives obid), and a lost link noticed in 45 seconds.
    fn base(&self) -> Vec<String> {
        let mut args = vec![];
        if let Some(config) = &self.config {
            args.extend(["-F".into(), config.display().to_string()]);
        }
        for o in [
            "BatchMode=yes",
            "ConnectTimeout=10",
            "RemoteCommand=none",
            "RequestTTY=no",
            "ControlMaster=no",
            "ControlPath=none",
            "ServerAliveInterval=15",
            "ServerAliveCountMax=3",
            "LogLevel=ERROR",
        ] {
            args.extend(["-o".into(), o.into()]);
        }
        args
    }

    /// Runs `command` on the other computer, with none of the alias's forwards. `command` is one
    /// plain path and its plain words, so whatever login shell that computer has (fish, on a Mac
    /// proven) parses it the same.
    pub fn command_args(&self, destination: &str, command: &str) -> Result<Vec<String>, String> {
        check_destination(destination)?;
        if command.is_empty()
            || !command
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._-= ".contains(c))
        {
            return Err(format!("{command:?} isn't a plain command"));
        }
        let mut args = self.base();
        args.extend(
            [
                "-o",
                "ClearAllForwardings=yes",
                "-T",
                "--",
                destination,
                command,
            ]
            .map(String::from),
        );
        Ok(args)
    }

    /// The tunnel: this Mac's `127.0.0.1:<local>` to the other computer's own
    /// `127.0.0.1:<remote>`, and nothing else. ssh exits if it can't hold the forward.
    pub fn tunnel_args(
        &self,
        destination: &str,
        local: u16,
        remote: u16,
    ) -> Result<Vec<String>, String> {
        check_destination(destination)?;
        let mut args = self.base();
        args.extend(
            [
                "-o",
                "ExitOnForwardFailure=yes",
                "-n",
                "-N",
                "-T",
                "-L",
                &format!("127.0.0.1:{local}:127.0.0.1:{remote}"),
                "--",
                destination,
            ]
            .map(String::from),
        );
        Ok(args)
    }

    /// `ssh -G`: what ssh itself makes of an alias.
    pub fn resolve_args(&self, destination: &str) -> Result<Vec<String>, String> {
        check_destination(destination)?;
        let mut args = self.base();
        args.extend(["-G", "--", destination].map(String::from));
        Ok(args)
    }

    /// The process for `args`: stdin closed unless the caller opens it, killed when dropped.
    pub fn command(&self, args: &[String]) -> tokio::process::Command {
        let mut c = tokio::process::Command::new(&self.program);
        c.args(args).stdin(Stdio::null()).kill_on_drop(true);
        if let Some(env) = &self.env {
            c.env_clear();
            for (k, v) in env {
                c.env(k, v);
            }
        }
        c
    }
}

/// The host, user and port `ssh -G` resolved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Resolved {
    pub hostname: String,
    pub user: String,
    pub port: u16,
}

pub fn parse_resolved(out: &str) -> Option<Resolved> {
    let mut hostname = None;
    let mut user = None;
    let mut port = None;
    for line in out.lines() {
        match line.split_once(' ') {
            Some(("hostname", v)) => hostname = Some(v.trim().to_string()),
            Some(("user", v)) => user = Some(v.trim().to_string()),
            Some(("port", v)) => port = v.trim().parse().ok(),
            _ => {}
        }
    }
    Some(Resolved {
        hostname: hostname?,
        user: user?,
        port: port?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ssh() -> Ssh {
        Ssh {
            program: "ssh".into(),
            config: None,
            env: None,
        }
    }

    #[test]
    fn computers_4_an_ssh_destination_is_never_an_option() {
        assert!(check_destination("-oProxyCommand=touch /tmp/x").is_err());
        assert!(check_destination("box; rm").is_err());
        assert!(check_destination("box\n").is_err());
        assert!(check_destination("").is_err());
        assert!(check_destination(&"a".repeat(256)).is_err());
        assert!(check_destination("oby@box.local").is_ok());
        assert!(check_destination("oby@[fe80::1%en0]").is_ok());
        assert!(ssh().tunnel_args("-p22", 1, 2).is_err());
        assert!(ssh().command_args("-x", "true").is_err());
        assert!(ssh().resolve_args("-G").is_err());
    }

    #[test]
    fn computers_4_ssh_is_a_tunnel_only_with_keys_and_no_shared_master() {
        let args = ssh().tunnel_args("box", 50000, 8810).unwrap();
        let dash = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(
            &args[dash + 1..],
            ["box"],
            "the destination is last, after --"
        );
        for want in [
            "BatchMode=yes",
            "ExitOnForwardFailure=yes",
            "RemoteCommand=none",
            "RequestTTY=no",
            "ControlMaster=no",
            "ControlPath=none",
            "ServerAliveInterval=15",
            "ServerAliveCountMax=3",
            "-N",
            "127.0.0.1:50000:127.0.0.1:8810",
        ] {
            assert!(args.iter().any(|a| a == want), "{want} in {args:?}");
        }
        assert!(!args.iter().any(|a| a == "-F"), "no config of Oby's own");
        assert!(
            !args.iter().any(|a| a.contains("StrictHostKeyChecking")),
            "the host key is the person's known_hosts' call"
        );
        let run = ssh()
            .command_args("box", "/opt/oby/obid pair-computer")
            .unwrap();
        assert!(run.iter().any(|a| a == "ClearAllForwardings=yes"));
        assert_eq!(run.last().unwrap(), "/opt/oby/obid pair-computer");
        assert!(ssh().command_args("box", "obid; rm -rf ~").is_err());
        assert!(ssh().command_args("box", "obid $(id)").is_err());
        assert!(ssh().command_args("box", "").is_err());
    }

    #[test]
    fn computers_4_ssh_g_is_read_for_host_user_and_port_only() {
        let r = parse_resolved("user oby\nhostname 10.0.0.4\nport 2222\nidentityfile ~/.ssh/x\n");
        assert_eq!(
            r,
            Some(Resolved {
                hostname: "10.0.0.4".into(),
                user: "oby".into(),
                port: 2222
            })
        );
        assert_eq!(parse_resolved("user oby\n"), None);
    }
}
