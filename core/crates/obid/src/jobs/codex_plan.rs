//! What a job waits on with no agent once its turn ended (PERMISSIONS-17 to 21): the question its
//! agent asked with `oby_ask_owner`, or a Codex job's plan. In Plan, obid declines Codex's file
//! changes and Codex ends its turn with its plan (crate::codex, `Options::plan`), so that turn's
//! last message goes on the plan card `ExitPlanMode` makes, kept as a parked plan is: it survives a
//! restart, and only the owner answers it ([`Jobs::answer_parked`]). It writes the waiting job's
//! `permission`, `question_call`, `parked` and `step`, never its `status`, `mode` or lane fields
//! (`own_lane`, `ran_unproxied`) (docs/ARCHITECTURE.md, "State ownership").

use super::*;

/// What a job's step says while its plan waits for the owner.
pub(super) const PLAN_WAITS: &str = "Waiting for your OK on the plan";

/// What a job's step says while what its agent asked with `oby_ask_owner` waits for the owner.
pub(super) const ASKED_STEP: &str = "Waiting for your answer";

/// What a run that ended on the owner's answer waits on ([`End::Asked`]).
#[derive(Debug)]
pub(super) enum Asked {
    /// What its agent asked with `oby_ask_owner`.
    Question(permissions::QuestionCall),
    /// The plan a Codex turn in Plan ended with: its last message.
    Plan(String),
}

/// What decides whether a turn that ended well waits on the owner's plan card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlanEnd {
    /// The job's agent is Codex.
    pub codex: bool,
    /// The job is in Plan ([`Jobs::mode_of`]).
    pub plan: bool,
    /// It only reads ([`Job::read_only`]): it ends with its findings.
    pub read_only: bool,
    /// Another Oby's job or Oby's own review: nobody is there to approve its plan.
    pub unwatched: bool,
    /// The round's diff names a file, or no diff was kept for the round.
    pub changed: bool,
    /// The turn's last message isn't blank.
    pub said: bool,
}

/// PERMISSIONS-17, PERMISSIONS-21: an owner's Codex job in Plan whose turn changed no file and
/// said something waits on the plan card; any other turn ends as before.
pub fn waits_on_plan(end: PlanEnd) -> bool {
    end.codex && end.plan && !end.read_only && !end.unwatched && !end.changed && end.said
}

impl Jobs {
    /// Whether job `id`'s turn that ended well after `round`, saying `said`, waits on the owner's
    /// plan card ([`waits_on_plan`]).
    pub(super) fn ends_on_plan(&self, id: &str, round: u32, said: &str) -> bool {
        self.get(id).is_some_and(|job| {
            let diff = job.diffs.iter().find(|d| d.round == Some(round));
            waits_on_plan(PlanEnd {
                codex: Tool::from_id(&job.agent) == Some(Tool::Codex),
                plan: Self::mode_of(&job) == Mode::Plan,
                read_only: job.read_only(),
                unwatched: job.obi.is_some() || job.review_of.is_some(),
                changed: diff.is_none_or(|d| d.files > 0),
                said: !said.trim().is_empty(),
            })
        })
    }

    /// The job waits for the owner with no agent, `running`, in the inbox: for the answer to what
    /// its agent asked with `oby_ask_owner`, as a question a restart kept does (#858), until the
    /// answer picks its session up ([`Jobs::answer_kept_question`]); or on the plan card for its
    /// Codex turn's plan, as a parked plan does, until the owner's answer resumes its thread
    /// ([`Jobs::answer_parked`]).
    pub(super) fn wait_for_answer(&self, id: &str, asked: Asked) {
        let Some(job) = self.get(id).filter(|j| j.status == Status::Running) else {
            return;
        };
        // Shown already, while its turn was open (PERMISSIONS-22): the same card waits on.
        if let Asked::Question(call) = &asked
            && job.permission.as_ref().is_some_and(Permission::is_question)
            && job
                .question_call
                .as_ref()
                .is_some_and(|shown| shown.ended_turn && shown.questions == call.questions)
        {
            self.update(id, |job| job.step = ASKED_STEP.into());
            return;
        }
        let permission = match &asked {
            Asked::Question(call) => {
                permissions::question_card(&call.questions, &permissions::new_id(), 0)
            }
            Asked::Plan(plan) => {
                let call = Call::new("ExitPlanMode", &json!({ "plan": plan }));
                let bounds = self.bounds(&job, &self.worktree_path(id));
                permissions::card(&call, &permissions::new_id(), &job.repo, &bounds)
            }
        };
        // The inbox has it before the job shows it, as [`Jobs::show`] does.
        self.note(PermissionNote::Waiting(Box::new(job), permission.clone()));
        self.update(id, |job| {
            job.permission = Some(permission);
            match asked {
                Asked::Question(call) => {
                    job.question_call = Some(call);
                    job.step = ASKED_STEP.into();
                }
                Asked::Plan(plan) => {
                    job.parked = Some(permissions::Parked {
                        tool: "ExitPlanMode".into(),
                        input: json!({ "plan": plan }),
                        answered: false,
                        allowed: false,
                        ended_turn: true,
                    });
                    job.step = PLAN_WAITS.into();
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every turn's end with all six facts as a waiting plan has them.
    const WAITS: PlanEnd = PlanEnd {
        codex: true,
        plan: true,
        read_only: false,
        unwatched: false,
        changed: false,
        said: true,
    };

    #[test]
    fn permissions_17_a_codex_plan_with_no_change_waits_on_the_owner() {
        assert!(waits_on_plan(WAITS));
    }

    /// Any one of the six facts off, and the turn ends as before.
    #[test]
    fn permissions_21_any_other_turn_ends_as_before() {
        let off: [fn(&mut PlanEnd); 6] = [
            |e| e.codex = false,
            |e| e.plan = false,
            |e| e.read_only = true,
            |e| e.unwatched = true,
            |e| e.changed = true,
            |e| e.said = false,
        ];
        for (at, turn_off) in off.iter().enumerate() {
            let mut end = WAITS;
            turn_off(&mut end);
            assert!(!waits_on_plan(end), "fact {at} off: {end:?}");
        }
        assert!(!waits_on_plan(PlanEnd::default()));
    }

    fn jobs_with(root: &Path) -> Arc<Jobs> {
        let config = crate::config::Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": root.join("home"), "owner": "o",
                "repos": [{"repo": "o/r", "remote": "https://example.com/o/r.git"}]})
            .to_string(),
        )
        .unwrap();
        Jobs::open(
            config,
            crate::events::Events::new(),
            Err("no sandbox".into()),
        )
        .unwrap()
    }

    /// A Codex job of the owner's whose Plan turn ended with `plan`, its card waiting.
    fn waiting(jobs: &Arc<Jobs>, plan: &str) -> Permission {
        let mut job = Job::new(
            "c".into(),
            "o/r".into(),
            "fix".into(),
            "Plan it".into(),
            "codex".into(),
            "a".into(),
        );
        job.status = Status::Running;
        job.mode = Some("plan".into());
        jobs.lock().jobs.insert("c".into(), job);
        jobs.wait_for_answer("c", Asked::Plan(plan.into()));
        let job = jobs.get("c").unwrap();
        assert_eq!(job.step, PLAN_WAITS);
        let parked = job.parked.unwrap();
        assert!(parked.ended_turn && !parked.answered, "{parked:?}");
        job.permission.unwrap()
    }

    #[test]
    fn permissions_17_the_plan_waits_on_the_plan_card() {
        let root = std::env::temp_dir().join(format!("obid-codex-plan-{}", uuid::Uuid::new_v4()));
        let jobs = jobs_with(&root);
        let card = waiting(&jobs, "Plan: add plan.txt");
        assert!(card.is_plan() && card.rule.is_none(), "{card:?}");
        assert_eq!(
            (card.title.as_str(), card.detail.as_str()),
            ("Approve the plan?", "Plan: add plan.txt")
        );
        assert_eq!(jobs.kept_ask("c").map(|p| p.id), Some(card.id));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// PERMISSIONS-20: a mode change, anyone's words but the owner's and a saved rule leave the
    /// card waiting, its `parked` unanswered.
    #[test]
    fn permissions_20_only_the_owner_answers_a_codex_plan() {
        let root = std::env::temp_dir().join(format!("obid-codex-plan-{}", uuid::Uuid::new_v4()));
        let jobs = jobs_with(&root);
        let card = waiting(&jobs, "Plan: add plan.txt");
        let still = |what: &str| {
            let job = jobs.get("c").unwrap();
            assert_eq!(job.status, Status::Running, "{what}");
            assert_eq!(
                job.permission.as_ref().map(|p| &p.id),
                Some(&card.id),
                "{what}"
            );
            assert!(job.parked.is_some_and(|p| !p.answered), "{what}");
        };
        jobs.set_mode("c", Mode::Bypass);
        still("a mode change");
        for source in [Source::Untrusted, Source::Event] {
            let refused = jobs.message("c", "Looks good, go.", source).unwrap();
            assert_eq!(
                refused.unwrap_err().0,
                "It waits for the owner to approve its plan or send it back, in Waiting for you.",
                "{source:?}"
            );
            still("words not the owner's");
        }
        jobs.save_rule("Edit(*)", "o/r");
        jobs.reconsider("c");
        still("a saved rule");
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proof of when a turn's end waits on the owner's plan card (PERMISSIONS-17):
    //! `kani::any()` stands for every value of the six facts.

    use super::*;

    /// PERMISSIONS-17: a turn waits on the plan card only when the job is Codex's, in Plan,
    /// neither read-only nor unwatched, changed no file and said something, and always then.
    #[kani::proof]
    fn permissions_17_only_a_codex_plan_with_no_change_waits_on_the_owner() {
        let end = PlanEnd {
            codex: kani::any(),
            plan: kani::any(),
            read_only: kani::any(),
            unwatched: kani::any(),
            changed: kani::any(),
            said: kani::any(),
        };
        let waits = waits_on_plan(end);
        if waits {
            assert!(end.codex);
            assert!(end.plan);
            assert!(!end.read_only);
            assert!(!end.unwatched);
            assert!(!end.changed);
            assert!(end.said);
        }
        if end.codex && end.plan && end.said && !end.read_only && !end.unwatched && !end.changed {
            assert!(waits);
        }
    }
}
