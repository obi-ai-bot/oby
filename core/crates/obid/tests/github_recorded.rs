//! GitHub as obid uses it today, recorded (tests/github_recorded/README.md): each test drives one
//! GitHub path through obid's API, on a scratch obid with the recording `gh` (tests/fakes/gh) first
//! on its `PATH`, and compares (a) every `gh` call obid made for it and (b) every answer of obid's
//! that carries GitHub's data with the recordings kept in tests/github_recorded/<path>/. These are
//! the baseline a move of GitHub behind another interface must reproduce exactly.

mod support;

#[path = "github_recorded/recorder.rs"]
mod recorder;

#[path = "github_recorded/sign_in.rs"]
mod sign_in;

#[path = "github_recorded/work_feed.rs"]
mod work_feed;

#[path = "github_recorded/issue_task.rs"]
mod issue_task;

#[path = "github_recorded/publish.rs"]
mod publish;

#[path = "github_recorded/noreply.rs"]
mod noreply;

#[path = "github_recorded/merge.rs"]
mod merge;

#[path = "github_recorded/autofix.rs"]
mod autofix;

#[path = "github_recorded/sphere_approval.rs"]
mod sphere_approval;

#[path = "github_recorded/review_submit.rs"]
mod review_submit;
