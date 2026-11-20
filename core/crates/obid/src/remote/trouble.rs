//! What went wrong with ssh, from what it printed, in words the person can act on
//! (docs/contracts/remote.md, COMPUTERS-5). A host key is always the person's call: Oby says how to
//! check and accept it, or that another machine may be answering, and connects to neither.

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "trouble", rename_all = "camelCase")]
pub enum SshTrouble {
    /// The host's key isn't in the person's `known_hosts`: accepting it is their call.
    HostKeyUnknown,
    /// The host's key changed: someone else may be answering.
    HostKeyChanged,
    /// No key of theirs (file or agent) is accepted; Oby never asks for a password.
    NoKey,
    Unreachable,
    ForwardFailed,
    Other {
        said: String,
    },
}

pub fn trouble(stderr: &str) -> SshTrouble {
    let s = stderr.to_ascii_lowercase();
    if s.contains("remote host identification has changed")
        || s.contains("host key for") && s.contains("has changed")
    {
        SshTrouble::HostKeyChanged
    } else if s.contains("host key verification failed")
        || s.contains("no matching host key")
        || s.contains("is not known")
    {
        SshTrouble::HostKeyUnknown
    } else if s.contains("permission denied") {
        SshTrouble::NoKey
    } else if s.contains("forwarding failed")
        || s.contains("cannot listen")
        || s.contains("address already in use")
    {
        SshTrouble::ForwardFailed
    } else if s.contains("connection refused")
        || s.contains("timed out")
        || s.contains("no route to host")
        || s.contains("could not resolve")
        || s.contains("network is unreachable")
        || s.contains("connection closed")
        || s.contains("broken pipe")
    {
        SshTrouble::Unreachable
    } else {
        SshTrouble::Other {
            said: stderr
                .trim()
                .chars()
                .filter(|c| !c.is_control() || *c == '\n')
                .take(300)
                .collect(),
        }
    }
}

impl SshTrouble {
    pub fn say(&self, destination: &str) -> String {
        match self {
            SshTrouble::HostKeyUnknown => format!(
                "{destination}'s host key isn't one you've accepted. Check it, then run `ssh {destination}` once in a terminal to accept it."
            ),
            SshTrouble::HostKeyChanged => format!(
                "{destination}'s host key changed: another machine may be answering. Oby won't connect until you check it and fix your known_hosts."
            ),
            SshTrouble::NoKey => format!(
                "{destination} accepts none of your SSH keys (Oby uses your keys and agent, never a password)."
            ),
            SshTrouble::Unreachable => format!("{destination} didn't answer over SSH."),
            SshTrouble::ForwardFailed => "ssh couldn't hold the tunnel's port on this Mac.".into(),
            SshTrouble::Other { said } => format!("ssh to {destination}: {said}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computers_5_ssh_trouble_says_what_to_do() {
        assert_eq!(
            trouble("Host key verification failed.\r\n"),
            SshTrouble::HostKeyUnknown
        );
        assert_eq!(
            trouble("@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@"),
            SshTrouble::HostKeyChanged
        );
        assert_eq!(
            trouble("oby@box: Permission denied (publickey)."),
            SshTrouble::NoKey
        );
        assert_eq!(
            trouble("ssh: connect to host box port 22: Connection refused"),
            SshTrouble::Unreachable
        );
        assert_eq!(
            trouble("Error: remote port forwarding failed"),
            SshTrouble::ForwardFailed
        );
        assert!(SshTrouble::HostKeyUnknown.say("box").contains("ssh box"));
        assert!(
            SshTrouble::HostKeyChanged
                .say("box")
                .contains("won't connect")
        );
        assert!(SshTrouble::NoKey.say("box").contains("never a password"));
        let SshTrouble::Other { said } = trouble("weird\u{1b}[31m thing") else {
            panic!("other")
        };
        assert!(!said.contains('\u{1b}'));
    }
}
