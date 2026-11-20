//! When obid calls off a request it sent another Oby (docs/contracts/obi-requests.md, OBIREQ-1 to
//! 6): the rules alone, on what obid already keeps. It reads jobs and exchanges and never writes
//! either; [`super::link::Link::call_off`] sends the `cancel` and marks the exchange.

use crate::job::{Job, Status};

use super::exchange::Exchange;

/// Why obid called a request off, in the log's words.
pub const STOPPED: &str = "the owner stopped its job";
pub const PR_ENDED: &str = "its pull request merged or closed";
pub const HEAD_MOVED: &str = "its pull request moved past the commit it asked about";
pub const ASKED_AGAIN: &str = "obid asked again about the pull request's new head";
pub const TURN_STOPPED: &str = "the owner stopped the turn that asked it";
pub const BY_OWNER: &str = "the owner called it off";

/// Whether `exchange` is a request of this Oby's that is still open and a `cancel` calls off
/// (protocol.md section 4, rule 10): sent or accepted, with nothing that ends it sent, received or
/// waiting to be sent.
pub fn open(exchange: &Exchange) -> bool {
    exchange.direction == "outgoing"
        && obi_protocol::CANCELLABLE_TYPES.contains(&exchange.kind.as_str())
        && matches!(exchange.state.as_str(), "sent" | "accepted")
        && !exchange.ended()
}

/// Whether `exchange` is a review request about pull request `number` of `repo`.
fn about(exchange: &Exchange, repo: &str, number: u64) -> bool {
    let body = &exchange.request["body"];
    exchange.kind == "review_request"
        && body["repo"]
            .as_str()
            .is_some_and(|r| r.eq_ignore_ascii_case(repo))
        && body["pr"].as_u64() == Some(number)
}

/// The open requests about `job`'s pull request that its end calls off, each with why, oldest
/// first (OBIREQ-1, 3): every one asked before the owner stopped it; every one once GitHub read it
/// merged or closed; and every one about a commit other than its head, once obid pushed another or
/// GitHub showed another after it was asked. What obid read or pushed before a request was asked
/// never calls it off, so a stale read never takes a request about the head obid just pushed.
pub fn due_for_job<'a>(
    job: &Job,
    exchanges: impl Iterator<Item = &'a Exchange>,
) -> Vec<(String, &'static str)> {
    let Some(number) = job
        .published
        .as_deref()
        .and_then(|url| job.pull_request_number(url))
    else {
        return Vec::new();
    };
    let stopped = (job.status == Status::Stopped)
        .then_some(job.finished)
        .flatten();
    let read = job.pull_request.as_ref();
    let pushed = job.last_push();
    let mut due: Vec<(i64, String, &'static str)> = exchanges
        .filter(|e| open(e) && about(e, &job.repo, number))
        .filter_map(|e| {
            let asked = e.created;
            let head = e.request["body"]["head_sha"].as_str().unwrap_or("");
            let read_since = read.filter(|pr| pr.checked_at >= asked);
            let why = if stopped.is_some_and(|at| asked <= at) {
                STOPPED
            } else if read_since.is_some_and(|pr| matches!(pr.state.as_str(), "MERGED" | "CLOSED"))
            {
                PR_ENDED
            } else if read_since
                .and_then(|pr| pr.head.as_deref())
                .is_some_and(|now| now != head)
                || pushed.is_some_and(|p| p.date >= asked && p.id != head)
            {
                HEAD_MOVED
            } else {
                return None;
            };
            Some((asked, e.correlation_id.clone(), why))
        })
        .collect();
    due.sort();
    due.into_iter().map(|(_, cid, why)| (cid, why)).collect()
}

/// The open requests for an approval of pull request `number` of `repo` at a commit other than
/// `head` (OBIREQ-2): what a new request about `head` replaces.
pub fn other_heads<'a>(
    exchanges: impl Iterator<Item = &'a Exchange>,
    repo: &str,
    number: u64,
    head: &str,
) -> Vec<String> {
    exchanges
        .filter(|e| {
            open(e)
                && about(e, repo, number)
                && super::approval::asked(&e.request["body"])
                && e.request["body"]["head_sha"] != head
        })
        .map(|e| e.correlation_id.clone())
        .collect()
}

/// The open questions conversation `conversation` asked since `since` (OBIREQ-4): what the turn
/// under way there asked.
pub fn asked_in_turn<'a>(
    exchanges: impl Iterator<Item = &'a Exchange>,
    conversation: &str,
    since: i64,
) -> Vec<String> {
    exchanges
        .filter(|e| {
            open(e)
                && e.kind == "question"
                && e.asked_in.as_deref() == Some(conversation)
                && e.created >= since
        })
        .map(|e| e.correlation_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::github::PullRequest;
    use crate::publish::Outgoing;

    const URL: &str = "https://github.com/acme/app/pull/12";

    fn request(cid: &str, created: i64, body: Value) -> Exchange {
        let request = json!({"id": cid, "correlation_id": cid, "type": "review_request",
                             "expires_at": "2099-01-01T00:00:00Z", "body": body});
        let mut e = Exchange::new("outgoing", request, "obi-zoe".into(), Some("zoe".into()));
        e.state = "sent".into();
        e.created = created;
        e
    }

    fn review(cid: &str, created: i64, head: &str) -> Exchange {
        request(
            cid,
            created,
            json!({"goal": "G", "repo": "Acme/App", "pr": 12, "head_sha": head,
                   "approve_on_host": true}),
        )
    }

    fn job(status: Status) -> Job {
        let mut job = Job::new(
            "j1".into(),
            "acme/app".into(),
            "fix".into(),
            "Fix it".into(),
            "claude".into(),
            "a".into(),
        );
        job.status = status;
        job.published = Some(URL.into());
        job
    }

    fn read(state: &str, head: &str, at: i64) -> PullRequest {
        PullRequest {
            state: state.into(),
            review_decision: None,
            base: "dev".into(),
            merged_at: None,
            checked_at: at,
            head: Some(head.into()),
        }
    }

    fn due(job: &Job, exchanges: &[Exchange]) -> Vec<(String, &'static str)> {
        due_for_job(job, exchanges.iter())
    }

    /// OBIREQ-1: a stopped job calls off what it asked before it stopped, and only that.
    #[test]
    fn obireq_1_a_stopped_job_calls_off_what_it_asked_before() {
        let asked = [review("a", 100, "h1"), review("b", 300, "h1")];
        let mut stopped = job(Status::Stopped);
        stopped.finished = Some(200);
        assert_eq!(due(&stopped, &asked), [("a".to_owned(), STOPPED)]);
        assert!(
            due(&job(Status::Running), &asked).is_empty(),
            "still running"
        );
        let mut unpublished = stopped.clone();
        unpublished.published = None;
        assert!(due(&unpublished, &asked).is_empty(), "no pull request");
        let mut other = review("c", 100, "h1");
        other.request["body"]["pr"] = json!(13);
        assert!(due(&stopped, &[other]).is_empty(), "another pull request");
    }

    /// OBIREQ-3: a pull request GitHub read merged or closed after the request calls it off.
    #[test]
    fn obireq_3_a_merged_or_closed_pull_request_calls_off() {
        let asked = [review("a", 100, "h1")];
        for state in ["MERGED", "CLOSED"] {
            let mut ended = job(Status::Done);
            ended.pull_request = Some(read(state, "h1", 150));
            assert_eq!(due(&ended, &asked), [("a".to_owned(), PR_ENDED)], "{state}");
            ended.pull_request = Some(read(state, "h1", 50));
            assert!(
                due(&ended, &asked).is_empty(),
                "{state} read before it was asked"
            );
        }
        let mut open = job(Status::Done);
        open.pull_request = Some(read("OPEN", "h1", 150));
        assert!(due(&open, &asked).is_empty());
    }

    /// OBIREQ-2: a head that moved past the request's, by GitHub's read or obid's own push since
    /// it was asked, calls it off; a read or push from before it doesn't.
    #[test]
    fn obireq_2_a_head_that_moved_since_calls_off() {
        let asked = [review("a", 100, "h1")];
        let mut moved = job(Status::Done);
        moved.pull_request = Some(read("OPEN", "h2", 150));
        assert_eq!(due(&moved, &asked), [("a".to_owned(), HEAD_MOVED)]);
        moved.pull_request = Some(read("OPEN", "h0", 90));
        assert!(due(&moved, &asked).is_empty(), "an older read");
        let mut push = Outgoing::new("push", "acme/app", "h2", Some(&moved.branch));
        push.date = 120;
        moved.outgoing.push(push.clone());
        assert_eq!(due(&moved, &asked), [("a".to_owned(), HEAD_MOVED)]);
        push.date = 90;
        moved.outgoing = vec![push];
        assert!(due(&moved, &asked).is_empty(), "a push before it was asked");
    }

    /// OBIREQ-5: what already ended, or came in, is never called off.
    #[test]
    fn obireq_5_only_an_open_outgoing_request_is_called_off() {
        let mut stopped = job(Status::Stopped);
        stopped.finished = Some(200);
        let mut ended = Vec::new();
        for state in ["done", "declined", "expired", "cancelled", "failed"] {
            let mut e = review(state, 100, "h1");
            e.state = state.into();
            ended.push(e);
        }
        let mut waiting = review("waiting", 100, "h1");
        waiting.outbox.push(json!({"type": "cancel"}));
        ended.push(waiting);
        let mut incoming = review("incoming", 100, "h1");
        incoming.direction = "incoming".into();
        ended.push(incoming);
        assert!(due(&stopped, &ended).is_empty());
        let mut accepted = review("accepted", 100, "h1");
        accepted.state = "accepted".into();
        assert!(open(&accepted));
    }

    /// OBIREQ-2: what a new request about `head` replaces: only approvals asked at another head.
    #[test]
    fn obireq_2_only_requests_at_another_head_are_replaced() {
        let mut plain = review("plain", 100, "h1");
        plain.request["body"]["approve_on_host"] = json!(false);
        let asked = [review("old", 100, "h1"), review("new", 100, "h2"), plain];
        assert_eq!(other_heads(asked.iter(), "acme/app", 12, "h2"), ["old"]);
        assert!(other_heads(asked.iter(), "acme/app", 13, "h2").is_empty());
    }

    /// OBIREQ-4: the questions a stopped turn asked: its conversation's, since it began.
    #[test]
    fn obireq_4_a_turn_calls_off_only_its_own_questions() {
        let question = |cid: &str, created: i64, conversation: Option<&str>| {
            let request = json!({"id": cid, "correlation_id": cid, "type": "question",
                                 "body": {"text": "Q"}});
            let mut e = Exchange::new("outgoing", request, "obi-zoe".into(), None);
            e.state = "sent".into();
            e.created = created;
            e.asked_in = conversation.map(str::to_owned);
            e
        };
        let asked = [
            question("this", 100, Some("c1")),
            question("before", 90, Some("c1")),
            question("other", 100, Some("c2")),
            question("thread", 100, None),
        ];
        assert_eq!(asked_in_turn(asked.iter(), "c1", 95), ["this"]);
    }
}
