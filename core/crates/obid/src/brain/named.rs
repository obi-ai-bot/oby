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

/// What a sentence's marks are: one right before a capital letter ends a sentence even with no
/// space after it ("Merge #4.Never merge #3.").
const MARKS: &str = ".!?;。！？";

/// `word` cut after each of [`MARKS`] a capital letter follows: a link's dots stay in it.
fn cut(word: &str) -> Vec<&str> {
    let mut pieces = vec![];
    let mut start = 0;
    let mut chars = word.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if MARKS.contains(c) && chars.peek().is_some_and(|(_, next)| next.is_uppercase()) {
            pieces.push(&word[start..at + c.len_utf8()]);
            start = at + c.len_utf8();
        }
    }
    pieces.push(&word[start..]);
    pieces
}

/// The plain sentences of `words`: those that ask nothing (no `?` in any word) and say no "not",
/// "cannot", "never" or "don't" (any `n't`, its apostrophe typed any way, and in any piece of a word, as
/// `#4.not`). A sentence ends at a word ending in `.`, `!`, `;` or holding a `?`, quotes and
/// brackets aside, at one of those right before a capital letter, or at a line's end. "Should I
/// merge #3?" and "don´t merge #3" are none (the owner's answer of 2026-11-15).
fn plain_sentences(words: &str) -> impl Iterator<Item = String> + '_ {
    const CLOSING: &str = "'\"”’)]>";
    let asks = |word: &&str| word.contains(['?', '？']);
    let denies = |word: &&str| {
        let word = word
            .replace(['’', '‘', '´', 'ʼ', '`', '′'], "'")
            .to_lowercase();
        word.split(|c: char| !c.is_alphanumeric() && c != '\'')
            .map(|piece| piece.trim_matches('\''))
            .any(|piece| {
                ["not", "never", "dont", "cannot"].contains(&piece) || piece.ends_with("n't")
            })
    };
    let ends = move |word: &&str| {
        asks(word)
            || word
                .trim_end_matches(|c: char| CLOSING.contains(c))
                .ends_with(['.', '!', ';', '。', '！'])
    };
    words.lines().flat_map(move |line| {
        let words: Vec<&str> = line.split_whitespace().flat_map(cut).collect();
        let plain: Vec<String> = words
            .split_inclusive(ends)
            .filter(|sentence| !sentence.iter().any(asks) && !sentence.iter().any(denies))
            .map(|sentence| sentence.join(" "))
            .collect();
        plain
    })
}

/// Whether one plain sentence of `words` says `phrase` and also holds what `about` looks for:
/// "Merge #4. Don't merge #3." says the merge of #4 only.
fn says_of(words: &str, phrase: &str, about: impl Fn(&str) -> bool) -> bool {
    plain_sentences(words).any(|sentence| says_words(&sentence, phrase) && about(&sentence))
}

/// What an owner says to continue a job, and to change one (`oby_continue_job`, `oby_change_job`).
pub(crate) const CONTINUE: [&str; 4] = ["continue", "try again", "resume", "go on"];
pub(crate) const CHANGE: [&str; 4] = ["change", "move", "put", "switch"];

/// Whether one plain sentence of `words` says one of `verbs` ([`CONTINUE`], [`CHANGE`]) and names
/// `job`: "how is Fix login going?" names Fix login, but no continuing of it.
pub(crate) fn names_job_to(words: &str, verbs: &[&str], job: &Job) -> bool {
    verbs
        .iter()
        .any(|verb| says_of(words, verb, |sentence| names_job(sentence, job)))
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

/// Whether `words` say, card answer and target in one plain sentence, `answer` to `item` named by
/// its target ([`names_card`]): "Allow Fix login. Don't allow Fix signup." allows only Fix login's.
/// After an outside read a card has no other way to be named, the only one waiting included ("Drop
/// it after a read", the owner's answer of 2026-11-15): "Allow me to explain." allows nothing.
pub(crate) fn names_card_answer(words: &str, answer: &str, item: &Item, job: Option<&Job>) -> bool {
    says_of(words, answer, |sentence| names_card(sentence, item, job))
}

/// Whether one plain sentence of `words` says "merge" and names `repo`'s pull request `pr`, by
/// its number or link or its job's task name (`job`). "Summarize #3" names #3 but no merge of it.
pub(crate) fn names_merge(words: &str, repo: &str, pr: u64, job: Option<&str>) -> bool {
    says_of(words, "merge", |sentence| {
        names_number(sentence, repo, pr) || job.is_some_and(|title| says_words(sentence, title))
    })
}

/// Whether `words` name `action` (`comment`, `approve`, `request_changes`, `close`, `label`) on
/// `repo`'s issue or pull request `number`: one plain sentence says the action and its number or
/// link. "Comment on #3" names no approval of it.
pub(crate) fn names_write(words: &str, action: &str, repo: &str, number: u64) -> bool {
    says_of(words, &action.replace('_', " "), |sentence| {
        names_number(sentence, repo, number)
    })
}

/// The answer when a call is [`Act::Skip`]: `what` isn't done, and how to have it done.
pub(crate) fn skipped(what: &str) -> String {
    format!(
        "Not done: {what}. This conversation read text Oby didn't write, so Oby does only what your own words this turn name: say it again plainly, with its number, link or task name."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `words` say `phrase` in a plain sentence.
    fn says_plainly(words: &str, phrase: &str) -> bool {
        plain_sentences(words).any(|sentence| says_words(&sentence, phrase))
    }

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
    fn taskconv_27_negated_or_asked_words_say_no_action() {
        for words in [
            "don't merge #3",
            "Don’t merge #3",
            "do not merge #3",
            "never merge #3",
            "should I merge #3?",
            "Not merge #3 yet",
            "we won't merge #3",
            "don´t merge #3",
            "donʼt merge #3",
            "don`t merge #3",
            "Merge #4.Not #3.",
            "don‘t merge #3",
            "We cannot merge #3 yet.",
            "Merge #4. I cannot merge #3.",
        ] {
            assert!(!names_merge(words, "o/r", 3, None), "{words}");
        }
        assert!(!names_merge("not merging #3 yet", "o/r", 3, None));
        assert!(!names_merge("Merge #4.Never merge #3.", "o/r", 3, None));
        assert!(names_merge("Merge #4.Never merge #3.", "o/r", 4, None));
        assert!(names_merge("Merge #4. I cannot merge #3.", "o/r", 4, None));
        assert!(names_merge("Untie the knot and merge #3.", "o/r", 3, None));
        assert!(!names_merge("Merge #4!Never merge #3", "o/r", 3, None));
        assert!(names_merge("merge #3", "o/r", 3, None));
        assert!(names_merge("Should I? Yes. Merge #3.", "o/r", 3, None));
        assert!(names_merge(
            "merge https://github.com/o/r/pull/3",
            "o/r",
            3,
            None
        ));
        assert!(!names_merge(
            "merge https://github.com/o/r/pull/3?",
            "o/r",
            3,
            None
        ));
        assert!(!names_write("don't approve #3", "approve", "o/r", 3));
        assert!(!says_plainly("don't allow it", "Allow"));
        assert!(!says_plainly("allow it?", "Allow"));
        assert!(says_plainly("allow it", "Allow"));
    }

    #[test]
    fn taskconv_27_the_action_and_its_target_share_one_plain_sentence() {
        assert!(!names_merge("Merge #4. Don't merge #3.", "o/r", 3, None));
        assert!(names_merge("Merge #4. Don't merge #3.", "o/r", 4, None));
        assert!(!names_merge("Merge it. It's #3?", "o/r", 3, None));
        for asked in [
            "Should I merge #3?!",
            "Should I merge #3?, or close it",
            "merge #3?.",
            "merge #3？",
        ] {
            assert!(!names_merge(asked, "o/r", 3, None), "{asked}");
        }
        assert!(!names_write(
            "Approve #4. Don't approve #3.",
            "approve",
            "o/r",
            3
        ));
        let job = Job {
            title: Some("Fix login".into()),
            repo: "o/r".into(),
            ..Job::default()
        };
        let other = "Continue Fix signup. Don't continue Fix login.";
        assert!(!names_job_to(other, &CONTINUE, &job));
        let card = Item {
            title: "Fix login".into(),
            ..Item::default()
        };
        let both = "Allow Fix signup. Don't allow Fix login.";
        assert!(!names_card_answer(both, "Allow", &card, None));
        assert!(names_card_answer("Allow Fix login.", "Allow", &card, None));
    }

    #[test]
    fn taskconv_29_a_card_is_answered_only_by_a_plain_sentence_that_names_it() {
        let merging = Job {
            title: Some("Fix the footer".into()),
            repo: "o/r".into(),
            published: Some("https://github.com/o/r/pull/3".into()),
            ..Job::default()
        };
        let card = Item {
            kind: "merge".into(),
            title: "Ready to merge: Fix the footer".into(),
            ..Item::default()
        };
        let merge = |words| names_card_answer(words, "Merge", &card, Some(&merging));
        assert!(!merge("Merge #4. Don't merge #3."));
        assert!(!merge("merge it"));
        assert!(merge("merge #3"));
        assert!(merge("Merge Fix the footer."));
        let asking = Job {
            title: Some("Fix login".into()),
            repo: "o/r".into(),
            ..Job::default()
        };
        let permission = Item {
            kind: "permission".into(),
            title: "Fix login".into(),
            ..Item::default()
        };
        let allow = |words| names_card_answer(words, "Allow", &permission, Some(&asking));
        assert!(!allow("Allow me to explain."));
        assert!(!allow("allow it"));
        assert!(allow("Allow Fix login."));
    }

    #[test]
    fn taskconv_33_a_job_is_continued_or_changed_only_when_the_words_say_so() {
        let job = Job {
            title: Some("Fix login".into()),
            repo: "o/r".into(),
            ..Job::default()
        };
        assert!(names_job_to("continue Fix login", &CONTINUE, &job));
        assert!(names_job_to("Fix login: try again.", &CONTINUE, &job));
        assert!(!names_job_to("how is Fix login going?", &CONTINUE, &job));
        assert!(!names_job_to("don't continue Fix login", &CONTINUE, &job));
        assert!(!names_job_to("continue it", &CONTINUE, &job));
        assert!(names_job_to("put Fix login in Plan", &CHANGE, &job));
        assert!(!names_job_to("is Fix login in Plan?", &CHANGE, &job));
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
    //! Kani's proofs of the rule's decision, [`act_on`], every input from `kani::any()`.

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
}
