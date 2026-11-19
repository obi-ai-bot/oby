//! The words obid already uses for a change request's state, and the only words any host's states
//! may map onto.
//!
//! These are GitHub's words, because GitHub came first and obid's API is add-only: the iPhone and
//! Apple Watch apps compare `pullRequest.state` and `reviewDecision` to them (docs/api/jobs.md,
//! `core/crates/obid/src/github.rs`). Another host maps its own states onto these; it never adds
//! one. A new word here would be a new value in an enum the Swift apps decode as closed, which
//! AGENTS.md's add-only rule forbids.

/// Defines one closed set of words: an enum, its words in order, and the two conversions.
macro_rules! api_words {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$vmeta:meta])* $variant:ident = $word:literal,)+ }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vmeta])* $variant,)+
        }

        impl $name {
            /// Every value, in the order of [`Self::WORDS`].
            pub const ALL: &'static [$name] = &[$($name::$variant,)+];
            /// Every word, as obid writes it.
            pub const WORDS: &'static [&'static str] = &[$($word,)+];

            /// The word obid writes for it.
            pub fn as_api(self) -> &'static str {
                match self {
                    $($name::$variant => $word,)+
                }
            }

            /// The value for one of [`Self::WORDS`], exactly; none for anything else.
            pub fn from_api(word: &str) -> Option<Self> {
                match word {
                    $($word => Some($name::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

api_words! {
    /// `pullRequest.state` on a job (docs/api/jobs.md). A draft is `OPEN` with
    /// [`MergeState::Draft`]; "rebase required" is `OPEN` with [`MergeState::Behind`].
    ChangeRequestState {
        Open = "OPEN",
        Closed = "CLOSED",
        Merged = "MERGED",
    }
}

api_words! {
    /// `pullRequest.reviewDecision` on a job; none (`null`) while there's no decision yet.
    ReviewDecision {
        ReviewRequired = "REVIEW_REQUIRED",
        ChangesRequested = "CHANGES_REQUESTED",
        Approved = "APPROVED",
    }
}

api_words! {
    /// One reviewer's latest review that counts, as obid's merge reads it (`github::Review`).
    ReviewState {
        Approved = "APPROVED",
        ChangesRequested = "CHANGES_REQUESTED",
        Dismissed = "DISMISSED",
    }
}

api_words! {
    /// A head's checks summed up, as obid's merge reads them (`github::Merging::checks`).
    ChecksState {
        /// Every check passed, was neutral or was skipped.
        Success = "SUCCESS",
        /// One hasn't finished, or nothing is known yet.
        Pending = "PENDING",
        Failure = "FAILURE",
        /// The head has no checks.
        None = "NONE",
    }
}

api_words! {
    /// Whether the change request can merge as it stands (`github::Merging::merge_state`).
    MergeState {
        Clean = "CLEAN",
        HasHooks = "HAS_HOOKS",
        /// Behind its base: "rebase required" on hosts that say so.
        Behind = "BEHIND",
        Blocked = "BLOCKED",
        /// Conflicts with its base.
        Dirty = "DIRTY",
        Draft = "DRAFT",
        Unstable = "UNSTABLE",
        Unknown = "UNKNOWN",
    }
}

api_words! {
    /// Whether its head merges into its base without conflicts (`github::Merging::mergeable`).
    Mergeable {
        Mergeable = "MERGEABLE",
        Conflicting = "CONFLICTING",
        Unknown = "UNKNOWN",
    }
}

/// One of the sets of words above, for the definition's mapping of a host's own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    ChangeRequestState,
    ReviewDecision,
    ReviewState,
    ChecksState,
    MergeState,
    Mergeable,
}

impl Family {
    pub const ALL: [Family; 6] = [
        Family::ChangeRequestState,
        Family::ReviewDecision,
        Family::ReviewState,
        Family::ChecksState,
        Family::MergeState,
        Family::Mergeable,
    ];

    /// The only words a host's state in this family may map onto.
    pub fn api_words(self) -> &'static [&'static str] {
        match self {
            Family::ChangeRequestState => ChangeRequestState::WORDS,
            Family::ReviewDecision => ReviewDecision::WORDS,
            Family::ReviewState => ReviewState::WORDS,
            Family::ChecksState => ChecksState::WORDS,
            Family::MergeState => MergeState::WORDS,
            Family::Mergeable => Mergeable::WORDS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The words are obid's own: each is written in obid's GitHub module as it is here. A word
    /// renamed there, or here, fails this.
    #[test]
    fn every_word_is_one_obid_already_uses() {
        let github = include_str!("../../obid/src/github.rs");
        for family in Family::ALL {
            for word in family.api_words() {
                assert!(
                    github.contains(&format!("\"{word}\"")),
                    "{family:?}: obid never writes {word}"
                );
            }
        }
    }

    /// The states a job's `pullRequest` shows today, and no more: the add-only rule.
    #[test]
    fn the_change_request_states_are_frozen() {
        assert_eq!(ChangeRequestState::WORDS, ["OPEN", "CLOSED", "MERGED"]);
        assert_eq!(
            ReviewDecision::WORDS,
            ["REVIEW_REQUIRED", "CHANGES_REQUESTED", "APPROVED"]
        );
    }

    #[test]
    fn words_convert_both_ways() {
        for family in Family::ALL {
            let words = family.api_words();
            assert!(!words.is_empty());
        }
        for state in MergeState::ALL {
            assert_eq!(MergeState::from_api(state.as_api()), Some(*state));
        }
        assert_eq!(ChangeRequestState::from_api("open"), None);
        assert_eq!(ChangeRequestState::from_api("DRAFT"), None);
    }
}
