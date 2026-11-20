//! The message box's chips (docs/contracts/composer.md, COMPOSER-4 to 7): the mode, tool, model
//! and effort the owner set for the tasks their words start, sent as `defaults` on
//! `POST /v1/plans`. They are the jobs `oby_dispatch` and `oby_offer` plan from those words: the
//! brain still picks the tool where the words name one, and a model and effort hold only on the
//! tool they were picked for. They live on the words' turn ([`super::turn::Words`]) and nowhere
//! else: obid never writes them to Settings or to an account.

use axum::http::StatusCode;
use serde::Deserialize;

use crate::api::ApiError;
use crate::jobs::NewJob;

/// `defaults` on `POST /v1/plans`: each key left out is the owner's usual (Settings' mode,
/// routing's tool, the account's model and effort).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    pub mode: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

impl Defaults {
    /// COMPOSER-6: a mode of the four the box offers, a tool obid knows, a model id obid passes
    /// on as one argument, and an effort of that tool's; a model or effort only with its tool.
    pub fn checked(self) -> Result<Self, ApiError> {
        let refused = |why: &str| Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why));
        if let Some(mode) = &self.mode
            && !["ask", "acceptEdits", "plan", "bypass"].contains(&mode.as_str())
        {
            return refused("defaults.mode is ask, acceptEdits, plan or bypass.");
        }
        let tool = match self.agent.as_deref() {
            None => None,
            Some(id) => match crate::agent::Tool::from_id(id) {
                Some(tool) => Some(tool),
                None => return refused("defaults.agent isn't a tool obid knows."),
            },
        };
        if (self.model.is_some() || self.effort.is_some()) && tool.is_none() {
            return refused("defaults.model and defaults.effort come with defaults.agent.");
        }
        if let Some(model) = &self.model
            && !crate::settings::valid_model(model)
        {
            return refused("defaults.model isn't a model id obid can pass on.");
        }
        if let (Some(effort), Some(tool)) = (&self.effort, tool)
            && !crate::settings::efforts(tool).contains(&effort.as_str())
        {
            return refused("defaults.effort isn't one of that tool's efforts.");
        }
        Ok(self)
    }

    /// Whether the owner's own chip asked for Bypass on words an app of theirs signed: their word,
    /// so the job starts with no Confirm (COMPOSER-4).
    pub fn owners_bypass(defaults: Option<&Self>, signed: bool) -> bool {
        signed && defaults.is_some_and(|d| d.mode.as_deref() == Some("bypass"))
    }

    /// COMPOSER-4 and 5: the job the brain planned, under the chips. The chip's mode is the job's
    /// (Bypass only on signed words, else as the brain's Bypass is: the owner's default or none);
    /// the chip's tool is the job's where the brain left it `auto`; its model and effort hold when
    /// the job is on that tool.
    pub fn apply(
        defaults: Option<&Self>,
        new: NewJob,
        signed: bool,
        bypass_widens: bool,
    ) -> NewJob {
        let Some(d) = defaults else { return new };
        let mode = match d.mode.as_deref() {
            // Unsigned words (a script's, oby-mcp's) don't widen trust by a chip: the brain's
            // mode stands. Where Bypass is the owner's default, the chip names that default, so
            // the mode is left out as `planned_job` leaves it, and nothing waits for a Confirm
            // the default itself wouldn't (#2543).
            Some("bypass") if !signed && bypass_widens => new.mode,
            Some("bypass") if !signed => None,
            Some(mode) => Some(mode.to_owned()),
            None => new.mode,
        };
        let auto = new
            .agent
            .as_deref()
            .is_none_or(|a| a.eq_ignore_ascii_case("auto"));
        let agent = match (&d.agent, auto) {
            (Some(chip), true) => Some(chip.clone()),
            _ => new.agent.clone(),
        };
        let on_chip = d.agent.is_some() && agent == d.agent;
        NewJob {
            mode,
            model: if on_chip {
                d.model.clone().or(new.model.clone())
            } else {
                new.model.clone()
            },
            effort: if on_chip { d.effort.clone() } else { None },
            agent,
            ..new
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chips(v: serde_json::Value) -> Defaults {
        serde_json::from_value(v).unwrap()
    }

    fn planned(agent: &str, mode: Option<&str>) -> NewJob {
        NewJob {
            agent: Some(agent.into()),
            mode: mode.map(str::to_owned),
            ..NewJob::default()
        }
    }

    /// COMPOSER-4: the chip's mode is the job's, over the brain's; Bypass only on signed words
    /// unless Bypass is the owner's default already.
    #[test]
    fn composer_4_the_chips_mode_is_the_jobs() {
        let plan = chips(serde_json::json!({"mode": "plan"}));
        let job = Defaults::apply(
            Some(&plan),
            planned("auto", Some("acceptEdits")),
            true,
            true,
        );
        assert_eq!(job.mode.as_deref(), Some("plan"));
        let bypass = chips(serde_json::json!({"mode": "bypass"}));
        let job = Defaults::apply(Some(&bypass), planned("auto", None), true, true);
        assert_eq!(job.mode.as_deref(), Some("bypass"));
        assert!(Defaults::owners_bypass(Some(&bypass), true));
        // Unsigned words (a script's, oby-mcp's) don't widen trust by a chip.
        let job = Defaults::apply(Some(&bypass), planned("auto", None), false, true);
        assert_eq!(job.mode, None);
        assert!(!Defaults::owners_bypass(Some(&bypass), false));
        // Where Bypass is the owner's default, it's the default either way: left out, so the
        // job takes it with no Confirm.
        let job = Defaults::apply(Some(&bypass), planned("auto", None), false, false);
        assert_eq!(job.mode, None);
        // No chips: the brain's plan as it was.
        let job = Defaults::apply(None, planned("codex", Some("ask")), true, true);
        assert_eq!(
            (job.agent.as_deref(), job.mode.as_deref()),
            (Some("codex"), Some("ask"))
        );
    }

    /// COMPOSER-5: the chip's tool where the brain left it auto, its model and effort only on it.
    #[test]
    fn composer_5_the_chips_model_and_effort_hold_on_their_tool() {
        let opus = chips(
            serde_json::json!({"agent": "claude", "model": "claude-opus-5-5", "effort": "high"}),
        );
        let job = Defaults::apply(Some(&opus), planned("auto", None), true, true);
        assert_eq!(job.agent.as_deref(), Some("claude"));
        assert_eq!(job.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(job.effort.as_deref(), Some("high"));
        // The words named Codex: the brain's tool stands, and Claude's model and effort don't go with it.
        let job = Defaults::apply(Some(&opus), planned("codex", None), true, true);
        assert_eq!(job.agent.as_deref(), Some("codex"));
        assert_eq!((job.model, job.effort), (None, None));
        // Named the same tool: the chip's model and effort hold.
        let job = Defaults::apply(Some(&opus), planned("claude", None), true, true);
        assert_eq!(job.effort.as_deref(), Some("high"));
    }

    /// COMPOSER-6: what obid takes, and what it refuses.
    #[test]
    fn composer_6_defaults_are_checked() {
        for ok in [
            serde_json::json!({}),
            serde_json::json!({"mode": "acceptEdits"}),
            serde_json::json!({"agent": "codex"}),
            serde_json::json!({"agent": "claude", "model": "claude-sonnet-5-5", "effort": "low"}),
        ] {
            assert!(chips(ok.clone()).checked().is_ok(), "{ok}");
        }
        for refused in [
            serde_json::json!({"mode": "sandbox"}),
            serde_json::json!({"mode": "yolo"}),
            serde_json::json!({"agent": "gemini-cli-9"}),
            serde_json::json!({"model": "claude-opus-5-5"}),
            serde_json::json!({"agent": "claude", "model": "--dangerous"}),
            serde_json::json!({"agent": "claude", "effort": "ludicrous"}),
        ] {
            assert!(chips(refused.clone()).checked().is_err(), "{refused}");
        }
        assert!(serde_json::from_value::<Defaults>(serde_json::json!({"account": "x"})).is_err());
    }
}
