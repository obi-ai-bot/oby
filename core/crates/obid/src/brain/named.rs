//! What the owner's own words name, once their conversation read text Oby didn't write
//! (docs/contracts/task-conversations.md, TASKCONV-26 to TASKCONV-33): their signed words then act
//! at once only on the pull request, issue, task or card they name, and what they didn't name
//! isn't done, its answer saying so in one line. Only their words this turn count
//! ([`owners_words`]), never what Oby read or a tool's arguments. Words no app of theirs signed
//! still make a Confirm card.

use crate::job::Job;
use crate::sphere::inbox::Item;

/// What a call that is the owner's to decide does on a turn that isn't their own word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    /// Done at once, as on their word.
    Now,
    /// Not done: its answer is [`skipped`].
    Skip,
    /// A Confirm card, which their click answers.
    Confirm,
}

/// `signed`: an app of the owner's signed the turn's words; `read_outside`: the turn or its
/// conversation read text Oby didn't write; `named`: their words name what the call acts on.
pub(crate) fn act_on(signed: bool, read_outside: bool, named: bool) -> Act {
    if !signed {
        return Act::Confirm;
    }
    if read_outside && !named {
        return Act::Skip;
    }
    Act::Now
}

/// Whether the owner's words settle a waiting card: they say its answer (`says_answer`), and they
/// name its target (`by_target`) or it is the one card of its kind waiting in the conversation.
pub(crate) fn card_named(by_target: bool, says_answer: bool, waiting_here: u32) -> bool {
    says_answer && (by_target || waiting_here == 1)
}

/// The owner's own words in `said`: past [`super::OBY_ASKED`], the question they answer is Oby's.
pub(crate) fn owners_words(said: &str) -> &str {
    said.split(super::OBY_ASKED).next().unwrap_or(said)
}

/// What a word is trimmed of at both ends: quotes and punctuation around it.
const AROUND: &str = "`'\"“”‘’,;:.!?()[]<>";

/// Whether `words` name `repo`'s pull request or issue `number`: by its link (`…/<repo>/pull/12`),
/// as `<repo>#12`, or as a word whose run of digits is it (`#12`, `PR12`); never `#123`, `v120`
/// or another repository's link or `owner/name#12`.
pub(crate) fn names_number(words: &str, repo: &str, number: u64) -> bool {
    let number = number.to_string();
    let repo = repo.to_ascii_lowercase();
    let whole = |rest: &str| !rest.starts_with(|c: char| c.is_ascii_digit());
    words
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| AROUND.contains(c)))
        .any(|word| {
            let lower = word.to_ascii_lowercase();
            if ["/pull/", "/pulls/", "/issues/"]
                .iter()
                .any(|k| lower.contains(k))
            {
                return ["pull", "pulls", "issues"].iter().any(|kind| {
                    let link = format!("{repo}/{kind}/{number}");
                    lower.match_indices(&link).any(|(at, _)| {
                        (at == 0 || lower[..at].ends_with('/')) && whole(&lower[at + link.len()..])
                    })
                });
            }
            if let Some((path, digits)) = lower.split_once('#')
                && path
                    .split_once('/')
                    .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('/'))
                && !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
            {
                return path == repo && digits == number;
            }
            word.split(|c: char| !c.is_ascii_digit())
                .any(|run| run == number)
        })
}

/// Whether `words` say `phrase`: its words, in order and next to each other, case aside. "merge
/// it" says Merge but not "Merge, and remember"; "allow it" says Allow but not "Always allow".
pub(crate) fn says_words(words: &str, phrase: &str) -> bool {
    let split = |text: &str| -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect()
    };
    let (words, phrase) = (split(words), split(phrase));
    !phrase.is_empty()
        && words
            .windows(phrase.len())
            .any(|run| run == phrase.as_slice())
}

/// Whether `words` name `job`: its task name, its own pull request, or the pull request or issue
/// it is about.
pub(crate) fn names_job(words: &str, job: &Job) -> bool {
    let own = job
        .published
        .as_deref()
        .and_then(|url| crate::github::pull_request_number(url, &job.repo));
    says_words(words, &job.short_title())
        || [own, job.pr, job.issue]
            .into_iter()
            .flatten()
            .any(|number| names_number(words, &job.repo, number))
}

/// Whether `words` name a card's target: its job, or for a card with none, its title or the
/// pull request a Confirm is about (`owner/name#3`).
pub(crate) fn names_card(words: &str, item: &Item, job: Option<&Job>) -> bool {
    if let Some(job) = job {
        return names_job(words, job);
    }
    let about = item
        .confirm
        .as_ref()
        .and_then(|c| c.target.split_once('#'))
        .and_then(|(repo, number)| Some((repo, number.parse::<u64>().ok()?)));
    says_words(words, &item.title)
        || about.is_some_and(|(repo, number)| names_number(words, repo, number))
}

/// Whether `words` name a merge of `repo`'s pull request `pr`: they say "merge", and its number or
/// link or its job's task name (`job`). "Summarize #3" names #3 but no merge of it.
pub(crate) fn names_merge(words: &str, repo: &str, pr: u64, job: Option<&str>) -> bool {
    says_words(words, "merge")
        && (names_number(words, repo, pr) || job.is_some_and(|title| says_words(words, title)))
}

/// Whether `words` name `action` (`comment`, `approve`, `request_changes`, `close`, `label`) on
/// `repo`'s issue or pull request `number`: they say the action and its number or link. "Comment
/// on #3" names no approval of it.
pub(crate) fn names_write(words: &str, action: &str, repo: &str, number: u64) -> bool {
    says_words(words, &action.replace('_', " ")) && names_number(words, repo, number)
}

/// The answer when a call is [`Act::Skip`]: `what` isn't done, and how to have it done.
pub(crate) fn skipped(what: &str) -> String {
    format!(
        "Not done: {what}. This conversation read text Oby didn't write, so Oby does only what your own words this turn name: say it again with its number, link or task name."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taskconv_28_a_number_names_only_its_whole_self() {
        assert!(names_number("merge #12", "o/r", 12));
        assert!(!names_number("merge #12", "o/r", 123));
        assert!(!names_number("merge #12", "o/r", 1));
        assert!(names_number("PR12, please", "o/r", 12));
        assert!(names_number("PR 12", "o/r", 12));
        assert!(!names_number("v120", "o/r", 12));
        let link = "see https://github.com/o/r/pull/12/files";
        assert!(names_number(link, "o/r", 12));
        assert!(names_number(link, "O/R", 12));
        assert!(!names_number(link, "x/y", 12));
        assert!(!names_number("https://github.com/foo/r/pull/12", "o/r", 12));
        assert!(names_number("x/y#12", "x/y", 12));
        assert!(!names_number("x/y#12", "o/r", 12));
        assert!(!names_number("https://github.com/o/r/pull/123", "o/r", 12));
        assert!(names_number(
            "(https://github.com/o/r/issues/12).",
            "o/r",
            12
        ));
        assert!(!names_number("merge it", "o/r", 12));
    }

    #[test]
    fn taskconv_31_words_after_oby_asked_are_not_the_owners() {
        let said = format!(
            "yes{}Merge o/r#3? The user answered: Merge #3)",
            super::super::OBY_ASKED
        );
        assert_eq!(owners_words(&said), "yes");
        assert!(!names_number(owners_words(&said), "o/r", 3));
        assert_eq!(owners_words("merge #3"), "merge #3");
    }

    #[test]
    fn taskconv_27_a_merge_or_write_is_named_by_its_action_and_its_number() {
        assert!(names_merge("Merge #3", "o/r", 3, None));
        assert!(names_merge(
            "merge Add a greeting",
            "o/r",
            3,
            Some("Add a greeting")
        ));
        assert!(!names_merge("Summarize the comments on #3", "o/r", 3, None));
        assert!(!names_merge("Merge #33", "o/r", 3, None));
        assert!(names_write("Approve #3", "approve", "o/r", 3));
        assert!(names_write(
            "request changes on PR 3",
            "request_changes",
            "o/r",
            3
        ));
        assert!(!names_write("Comment on #3", "approve", "o/r", 3));
        assert!(!names_write("Approve it", "approve", "o/r", 3));
    }

    #[test]
    fn says_words_says_a_whole_phrase_in_order() {
        assert!(says_words("merge it", "Merge"));
        assert!(!says_words("merge it", "Merge, and remember"));
        assert!(says_words("allow it", "Allow"));
        assert!(!says_words("allow it", "Always allow"));
        assert!(says_words("OK, approve the plan.", "Approve"));
        assert!(says_words("start the Fix Login job", "Fix login"));
        assert!(!says_words("fix it", "Fix login"));
        assert!(!says_words("anything", ""));
    }

    #[test]
    fn card_named_needs_the_answer_and_the_card() {
        assert!(card_named(false, true, 1));
        assert!(card_named(true, true, 2));
        assert!(card_named(true, true, 0));
        assert!(!card_named(false, true, 2));
        assert!(!card_named(false, true, 0));
        assert!(!card_named(true, false, 1));
    }

    #[test]
    fn act_on_skips_only_an_unnamed_call_after_a_read() {
        assert_eq!(act_on(true, true, false), Act::Skip);
        assert_eq!(act_on(true, true, true), Act::Now);
        assert_eq!(act_on(true, false, false), Act::Now);
        assert_eq!(act_on(false, true, true), Act::Confirm);
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of the rule's two decisions, every input from `kani::any()`.

    use super::*;

    #[kani::proof]
    fn taskconv_27_an_unnamed_call_after_a_read_is_never_done() {
        let (read, named): (bool, bool) = (kani::any(), kani::any());
        assert_eq!(act_on(true, true, false), Act::Skip);
        assert_eq!(act_on(true, read, true), Act::Now);
        assert_eq!(act_on(true, false, named), Act::Now);
    }

    #[kani::proof]
    fn taskconv_30_unsigned_words_always_confirm() {
        let (read, named): (bool, bool) = (kani::any(), kani::any());
        assert_eq!(act_on(false, read, named), Act::Confirm);
    }

    #[kani::proof]
    fn taskconv_29_a_card_is_named_by_its_target_or_as_the_only_one() {
        let (by_target, says_answer, waiting_here): (bool, bool, u32) =
            (kani::any(), kani::any(), kani::any());
        let named = card_named(by_target, says_answer, waiting_here);
        if !says_answer {
            assert!(!named);
        }
        if !by_target && waiting_here != 1 {
            assert!(!named);
        }
        if says_answer && (waiting_here == 1 || by_target) {
            assert!(named);
        }
    }
}
