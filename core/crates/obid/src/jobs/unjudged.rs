//! Why a failed check judged nothing, so the job ends for the owner's Publish by hand: a check
//! that says so (PUBLISHING-22) or the machine's failure, not the code's (PUBLISHING-17).

/// The job's reason when its check judged nothing, for the owner's Publish by hand (PUBLISHING-8).
pub(super) fn unchecked(why: impl std::fmt::Display) -> String {
    format!(
        "Not published: {why}, so nothing counts as checked. Read its change, then Publish it by hand."
    )
}

/// Why the failed check of `command` judged nothing, from its `output`: a line that starts
/// `couldn't judge: <why>` (trimmed) first, its words cleaned and cut on a second line, since
/// the first is obid's own (job_report hands it to the brain as such); else PUBLISHING-17's signs.
pub(super) fn why(command: &str, output: &str) -> Option<String> {
    let judge = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("couldn't judge: "));
    if let Some(why) = judge {
        let why = crate::github_work::clean(why, 200);
        return Some(unchecked("the check couldn't judge this change") + "\n" + &why);
    }
    super::environment_signature(command, output).map(|label| {
        unchecked(format!(
            "{label}, which is this Mac's doing, not the change's"
        ))
    })
}
