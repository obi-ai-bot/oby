//! What a download becomes (BROWSER-13): only a person's own act delivers one; an agent's stays in
//! its profile. It owns no state and moves no file; the driver does what [`download_rule`] says.
//! The lab calls this enum `Download`; here it is `DownloadFate`, apart from the wire's
//! [`crate::browser::contract::Download`].

use super::Actor;

/// What a download becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadFate {
    /// Stays in the agent's profile, reachable only through the agent's own room.
    KeepInProfile,
    /// Handed to the person (their Downloads folder, a card in the app).
    Deliver,
}

/// Only a person's own act delivers a download (the owner's decision of 2026-11-19).
pub fn download_rule(started_by: &Actor) -> DownloadFate {
    match started_by {
        Actor::Person(_) => DownloadFate::Deliver,
        Actor::Agent(_) => DownloadFate::KeepInProfile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_13_only_a_persons_act_delivers_a_download() {
        assert_eq!(
            download_rule(&Actor::Agent("c1".into())),
            DownloadFate::KeepInProfile
        );
        assert_eq!(
            download_rule(&Actor::Person("owner".into())),
            DownloadFate::Deliver
        );
    }
}
