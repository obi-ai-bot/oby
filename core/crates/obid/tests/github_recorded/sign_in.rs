//! Sign-in status and the reviewer account: `GET /v1/github`, the owner's device sign-in
//! (`POST /v1/github/sign-in`), and the reviewer's (`POST`/`DELETE /v1/github/reviewer/sign-in`).

use serde_json::json;

use crate::support::*;

#[test]
fn github_recorded_sign_in() {
    let (_folder, recording, obid, _) = crate::recorder::recorded_obid("sign-in", "sign_in");
    let mut answers = Vec::new();
    let mut said = |what: &str, answer: Answer| {
        answers.push(json!({"asked": what, "status": answer.status, "body": answer.body}));
    };

    said("GET /v1/github", call(&obid, "GET", "/v1/github", None));
    said(
        "POST /v1/github/sign-in",
        call(&obid, "POST", "/v1/github/sign-in", None),
    );
    recording.raise("approve");
    // The sign-in's gh ends before obid is asked again, so none of the reads below run at its time.
    within(20, "gh's sign-in to end", || {
        recording.ended("auth login").then_some(())
    });
    let signed = within(20, "gh to say it's signed in", || {
        let now = call(&obid, "GET", "/v1/github", None);
        (now.body["signedIn"] == true && now.body["signIn"].is_null()).then_some(now)
    });
    said("GET /v1/github, signed in", signed);

    said(
        "POST /v1/github/reviewer/sign-in",
        call(&obid, "POST", "/v1/github/reviewer/sign-in", None),
    );
    recording.raise("approve-reviewer");
    within(20, "the reviewer's sign-in to end", || {
        recording
            .ended("auth login --hostname github.com --git-protocol https --web --skip-ssh-key --insecure-storage")
            .then_some(())
    });
    let reviewer = within(20, "the reviewer to be signed in", || {
        let now = call(&obid, "GET", "/v1/github", None);
        (now.body["reviewer"]["signedIn"] == true && now.body["reviewer"]["signIn"].is_null())
            .then_some(now)
    });
    said("GET /v1/github, reviewer signed in", reviewer);
    said(
        "DELETE /v1/github/reviewer/sign-in",
        call(&obid, "DELETE", "/v1/github/reviewer/sign-in", None),
    );
    said(
        "GET /v1/github, reviewer signed out",
        call(&obid, "GET", "/v1/github", None),
    );

    recording.assert_calls(&crate::recorder::distinct_reads(
        recording.calls_starting(&["auth"]),
    ));
    recording.assert_answers(&json!(answers));
}
