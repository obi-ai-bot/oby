//! The model and effort a job runs on (#1574, AGENTS-6), on its account as it is now, and what a
//! change of tool does to them (CLIVERSIONS-6). It owns no state of its own: it writes only the
//! job's `model`, `model_from_account`, `effort`, `effort_named` and `agent_version`. An effort the
//! owner's chip named (docs/contracts/composer.md, COMPOSER-5) holds over the account's, on its
//! own tool only.

use super::*;

/// A job waiting for room goes to `tool`: a model of another tool's gives way to the new tool's
/// own, as a move's does, and so does a named effort; its version goes to the next round's read
/// (CLIVERSIONS-6).
pub(super) fn on_tool(job: &mut Job, tool: Tool) {
    if job.agent != tool.id() {
        job.agent = tool.id().into();
        job.model = None;
        job.effort_named = None;
        job.agent_version = None;
        job.model_from_account = false;
    }
}

/// The model and effort `job` runs on, on its account as it is now (#1574): a model it named
/// itself stays; else its account's, as Settings › Agents set them (`tuning`, by account); neither,
/// the tool's own. An effort the owner's chip named stays (COMPOSER-5); else its account's. Again
/// whenever it changes account. `offered` is that account's own list, empty while none was read
/// since obid started (`ModelLists::to_check`): the saved model stands.
pub(super) fn tune(
    job: &mut Job,
    tuning: &BTreeMap<String, crate::settings::Tuning>,
    offered: &[crate::model_lists::Offered],
) {
    let kept = tuning.get(&job.account).cloned().unwrap_or_default();
    if job.model.is_none() || job.model_from_account {
        let model = kept.offered_model(offered);
        job.model_from_account = model.is_some();
        job.model = model.map(str::to_owned);
    }
    job.effort = (job.effort_named.clone()).or_else(|| kept.effort(job.complex).map(str::to_owned));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// COMPOSER-5: the chip's effort holds over the account's, until the job changes tool.
    #[test]
    fn composer_5_a_named_effort_holds_over_the_accounts() {
        let mut job = Job::new(
            "j".into(),
            "o/r".into(),
            "build".into(),
            "x".into(),
            "claude".into(),
            "a".into(),
        );
        let tuning = BTreeMap::from([(
            "a".to_owned(),
            crate::settings::Tuning {
                model: None,
                effort: Some("low".into()),
                complex_effort: None,
            },
        )]);
        tune(&mut job, &tuning, &[]);
        assert_eq!(job.effort.as_deref(), Some("low"));
        job.effort_named = Some("max".into());
        tune(&mut job, &tuning, &[]);
        assert_eq!(job.effort.as_deref(), Some("max"));
        on_tool(&mut job, Tool::Codex);
        tune(&mut job, &tuning, &[]);
        assert_eq!(job.effort.as_deref(), Some("low"));
    }
}
