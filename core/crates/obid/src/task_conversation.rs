//! Every task has a conversation (docs/contracts/task-conversations.md; Wellington, 2026-10-30).
//! A task started anywhere but a conversation (the Tasks view's `POST /v1/jobs`, GitHub work) gets
//! one of its own, named after it: the owner's words that started it, when they're the task's, its
//! card under them, and Oby's line saying it started. Every follow-up of the task is in the same
//! conversation, since a follow-up carries its job's on ([`crate::jobs::follow_up`]). So what the
//! owner tells the task, Oby's answer and the follow-up's card stay together, in one place.
//!
//! A task kept from before this had none: the first time the owner writes to it from its page
//! ([`of`]), its chain (the task, the one it follows up, and theirs) gets one then.

use crate::api::Shared;
use crate::jobs::{NewJob, Refused};

/// The owner's words that started a task, for its conversation's first turn, and why they aren't
/// on obid's allowlist when they aren't (crate::secretspec: words no app of the owner's signed).
pub struct Words<'a> {
    pub text: &'a str,
    pub foreign: Option<String>,
}

/// Starts the task `new`, in a conversation of its own when it has none and follows nothing up
/// (TASKCONV-1, TASKCONV-2). `words`: the owner's, when they're the task's; `foreign`: why the
/// task's own title isn't the owner's words alone (a task from GitHub), which marks the
/// conversation. A conversation that can't be made leaves the task started without one, said in
/// the log: starting the task matters more.
pub fn create(
    shared: &Shared,
    new: NewJob,
    words: Option<Words>,
    foreign: Option<String>,
) -> Result<crate::job::Job, Refused> {
    let alone = new.conversation.is_none() && new.follow_up_of.is_none();
    let job = shared.jobs.create(new)?;
    if !alone {
        return Ok(job);
    }
    Ok(start(shared, &job, words, foreign).unwrap_or(job))
}

/// A task just started elsewhere (GitHub work: crate::github_work), in a conversation of its own
/// when it has none and follows nothing up, as [`create`] does. `foreign`: why its title isn't the
/// owner's own words. The task as it is when the conversation can't be made.
pub fn give(shared: &Shared, job: crate::job::Job, foreign: Option<String>) -> crate::job::Job {
    if job.conversation.is_some() || job.follow_up_of.is_some() {
        return job;
    }
    start(shared, &job, None, foreign).unwrap_or(job)
}

/// The conversation of the job `id`, made now when neither it nor any job of its chain has one
/// that's still there (TASKCONV-4): what its page's words go into. `None` for a job obid doesn't
/// have, or a conversation that can't be made.
pub fn of(shared: &Shared, id: &str) -> Option<String> {
    let job = shared.jobs.get(id)?;
    // ponytail: walks every job kept, once per page's words to a task with no conversation; an
    // index of follow-ups if that ever shows
    let all = shared.jobs.list(usize::MAX);
    let parent = |j: &crate::job::Job| {
        j.follow_up_of
            .as_deref()
            .and_then(|p| all.iter().find(|x| x.id == p))
    };
    // the chain's first task: what the task follows up, and what that one follows up
    let mut root = &job;
    let mut seen = 0;
    while let Some(up) = parent(root) {
        root = up;
        seen += 1;
        if seen > all.len() {
            break;
        }
    }
    let rooted = |j: &crate::job::Job| {
        let mut at = j;
        for _ in 0..=all.len() {
            if at.id == root.id {
                return true;
            }
            match parent(at) {
                Some(up) => at = up,
                None => return false,
            }
        }
        false
    };
    let chain: Vec<&crate::job::Job> = std::iter::once(root)
        .chain(all.iter().filter(|j| j.id != root.id && rooted(j)))
        .collect();
    let live = |c: &Option<String>| {
        c.as_deref()
            .filter(|c| shared.conversations.exists(c))
            .map(str::to_owned)
    };
    let conversation = match live(&job.conversation)
        .or_else(|| chain.iter().find_map(|j| live(&j.conversation)))
    {
        Some(conversation) => conversation,
        None => {
            // Oby's words only (the owner's own aren't kept with a task this old): no words from
            // outside, unless the first task's title isn't the owner's own (GitHub's).
            let foreign = (!root.vetted)
                .then(|| format!("the title of job {}, which came from outside Oby", root.id));
            let root = root.clone();
            start(shared, &root, None, foreign)?.conversation?
        }
    };
    for j in chain.iter().filter(|j| live(&j.conversation).is_none()) {
        shared.jobs.join_conversation(&j.id, &conversation, None);
    }
    Some(conversation)
}

/// A new conversation named after `job`, with the owner's `words` and Oby's line naming it, and
/// the job in it. `None`, said in the log, when the conversation can't be made.
fn start(
    shared: &Shared,
    job: &crate::job::Job,
    words: Option<Words>,
    foreign: Option<String>,
) -> Option<crate::job::Job> {
    let conversation = match shared.conversations.start_for(&job.short_title()) {
        Ok(conversation) => conversation,
        Err(e) => {
            tracing::warn!("job {}: no conversation of its own: {e:?}", job.id);
            return None;
        }
    };
    let turn = words.and_then(|w| {
        shared
            .conversations
            .append(
                &conversation,
                "user",
                w.text.trim().to_owned(),
                vec![],
                w.foreign,
            )
            .ok()
            .map(|turn| turn.id)
    });
    let joined = shared
        .jobs
        .join_conversation(&job.id, &conversation, turn)?;
    let line = crate::brain::actions::started_line(shared, std::slice::from_ref(&joined));
    if let Err(e) =
        shared
            .conversations
            .append(&conversation, "oby", line, vec![joined.id.clone()], foreign)
    {
        tracing::warn!(
            "job {}: its conversation {conversation} wasn't told it started: {e:?}",
            job.id
        );
    }
    Some(joined)
}
