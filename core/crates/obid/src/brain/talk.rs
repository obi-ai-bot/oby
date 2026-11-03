//! Which account a conversation talks through (docs/contracts/conversation-agent.md): the owner's
//! choice for it (`talkAccount` on `PATCH /v1/conversations/{id}`), else the setting's (on `PUT
//! /v1/settings/jobs`), else Auto, whichever connected account has room. When that one can't run a
//! turn, the turn moves to another account of any tool, and says so; `accountFailover` off keeps it
//! to one. A ChatGPT login stays out of that: no turn moves on to or off one without the owner's
//! click ([`Apart`]). `conversation.agent` in the configuration is only the tool Auto looks at first.

use axum::http::StatusCode;

use crate::agent::Tool;
use crate::api::{ApiError, Shared};
use crate::config::Account;

/// The owner's word for "whichever has room".
pub const AUTO: &str = "auto";

/// The most accounts one turn tries, and the most of one tool among them: past a full tool the
/// turn reaches another.
const MOST: usize = 3;
const MOST_OF_A_TOOL: usize = 2;

/// Whether `account` is one a conversation may be told to talk through: `auto`, or an account
/// there is, on a tool that can talk here.
pub fn check(shared: &Shared, account: &str) -> Result<(), ApiError> {
    if account == AUTO {
        return Ok(());
    }
    let unprocessable = |why: String| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why);
    let (tool, found) = crate::accounts::find(&shared.config.agents.get(), account)
        .ok_or_else(|| unprocessable(format!("There's no account {account}.")))?;
    if !tools(shared).contains(&tool) {
        return Err(unprocessable(format!(
            "{} · {} can't talk here: obid can't hold {} to its egress proxy on this machine.",
            tool.name(),
            found.name,
            tool.name()
        ))
        .because("talk-needs-egress-box"));
    }
    Ok(())
}

/// The tools that can talk here, in the order Auto looks at them: the configured one, then the
/// others. Cursor comes last, whatever its usage reads, and only where the box holds it to obid's
/// egress proxy (crate::brain::harness_cursor).
fn tools(shared: &Shared) -> Vec<Tool> {
    let first = shared.config.conversation.agent;
    let mut tools = vec![first];
    tools.extend(Tool::ALL.into_iter().filter(|t| *t != first));
    if !shared.jobs.confines_egress() {
        tools.retain(|t| *t != Tool::Cursor);
    }
    tools
}

/// The account the owner chose for `conversation`: its own choice, else the setting; none is Auto,
/// and so is a choice whose account has gone.
pub(super) fn chosen(shared: &Shared, conversation: &str) -> Option<String> {
    let id = shared
        .conversations
        .talk_account(conversation)
        .or_else(|| crate::settings::load(&shared.config.home).talk_account)?;
    (id != AUTO && crate::accounts::find(&shared.config.agents.get(), &id).is_some()).then_some(id)
}

/// The accounts a turn tries, in order: [`order`] over those signed in and not full, of every tool
/// that can talk. `on`: the account the conversation's session is on, while it has room.
pub(super) fn accounts(
    shared: &Shared,
    chosen: Option<&str>,
    on: Option<&str>,
) -> Vec<(Tool, Account)> {
    let mut tools = tools(shared);
    // Moving off the chosen account, one of its own tool comes first.
    if let Some((tool, _)) =
        chosen.and_then(|id| crate::accounts::find(&shared.config.agents.get(), id))
    {
        tools.sort_by_key(|t| *t != tool);
    }
    let apart = chatgpt_apart(shared, chosen, on);
    let lists = tools
        .into_iter()
        .map(|tool| (tool, shared.jobs.thinking_accounts(tool)))
        .collect();
    let on = on.filter(|on| shared.jobs.has_room(on));
    keep_apart(
        order(chosen, on, lists, shared.jobs.account_failover(), |id| {
            shared.jobs.has_room(id)
        }),
        &apart,
    )
}

/// Where a ChatGPT login stands in a turn's choice. Nobody moves on to or off one without the owner's
/// click (docs/contracts/chatgpt-sign-in.md): whatever holds the account it's on, the turn doesn't go
/// elsewhere by itself.
#[derive(Debug, PartialEq, Eq)]
enum Apart {
    /// The account it stays on, the owner's choice or the session's: this or nothing.
    On(String),
    /// None of them: it's chosen or on something else, or something else is here.
    Off,
    /// There's nothing else on this Mac, so Auto is the first of them.
    OnlyThese,
}

fn on_chatgpt(id: &str, shared: &Shared) -> Option<bool> {
    let (_, account) = crate::accounts::find(&shared.config.agents.get(), id)?;
    Some(crate::chatgpt::marker_of(&account).is_some())
}

fn chatgpt_apart(shared: &Shared, chosen: Option<&str>, on: Option<&str>) -> Apart {
    // The owner's choice is their click; else the session's, full or not, as it stands now.
    if let Some((id, chatgpt)) = chosen
        .into_iter()
        .chain(on)
        .find_map(|id| on_chatgpt(id, shared).map(|chatgpt| (id, chatgpt)))
    {
        return if chatgpt {
            Apart::On(id.to_owned())
        } else {
            Apart::Off
        };
    }
    nothing_else(shared)
}

/// With no account chosen or under way: whether anything but a ChatGPT login is on this Mac, of any
/// tool, whether or not it can run now. A full one, or one whose token is ending, is still there:
/// that isn't the owner's word to use their plan.
fn nothing_else(shared: &Shared) -> Apart {
    let home = crate::jobs::login_home();
    let another = Tool::ALL.into_iter().any(|tool| {
        shared.jobs.accounts(tool).iter().any(|account| {
            crate::chatgpt::marker_of(account).is_none()
                && crate::jobs::has_login(tool, account, home.as_deref())
        })
    });
    if another {
        Apart::Off
    } else {
        Apart::OnlyThese
    }
}

fn keep_apart(order: Vec<(Tool, Account)>, apart: &Apart) -> Vec<(Tool, Account)> {
    keep_chatgpt_apart(order, apart, |account| {
        crate::chatgpt::marker_of(account).is_some()
    })
}

fn keep_chatgpt_apart(
    order: Vec<(Tool, Account)>,
    apart: &Apart,
    on_chatgpt: impl Fn(&Account) -> bool,
) -> Vec<(Tool, Account)> {
    match apart {
        Apart::On(id) => order.into_iter().filter(|(_, a)| a.id == *id).collect(),
        Apart::Off => order.into_iter().filter(|(_, a)| !on_chatgpt(a)).collect(),
        Apart::OnlyThese => order.into_iter().take(1).collect(),
    }
}

/// The chosen account first, else the one the session is on, then whichever has room: those of its
/// tool before others, one held back (`room`) after any with room, and Cursor, whose turn marks
/// the conversation for good, last, whatever its usage reads. A chosen account that can't run is
/// moved past only while failover is on; with it off, a turn tries one account.
fn order(
    chosen: Option<&str>,
    on: Option<&str>,
    lists: Vec<(Tool, Vec<Account>)>,
    failover: bool,
    room: impl Fn(&str) -> bool,
) -> Vec<(Tool, Account)> {
    let rank = |tool: Tool, account: &Account| match tool {
        Tool::Cursor => 2,
        _ if room(&account.id) => 0,
        _ => 1,
    };
    let mut all: Vec<(Tool, Account)> = lists
        .into_iter()
        .flat_map(|(tool, accounts)| accounts.into_iter().map(move |a| (tool, a)))
        .collect();
    all.sort_by_key(|(tool, account)| rank(*tool, account));
    let at = |all: &[(Tool, Account)], id: &str| all.iter().position(|(_, a)| a.id == id);
    if !failover && chosen.is_some_and(|id| at(&all, id).is_none()) {
        return Vec::new();
    }
    let first = chosen
        .and_then(|id| at(&all, id))
        .or_else(|| on.and_then(|id| at(&all, id)));
    if let Some(first) = first {
        let first = all.remove(first);
        all.sort_by_key(|(tool, account)| (rank(*tool, account), *tool != first.0));
        all.insert(0, first);
    }
    let mut tried: Vec<(Tool, Account)> = Vec::new();
    for (tool, account) in all {
        if tried.iter().filter(|(t, _)| *t == tool).count() < MOST_OF_A_TOOL {
            tried.push((tool, account));
        }
    }
    tried.truncate(if failover { MOST } else { 1 });
    tried
}

/// The account a new conversation's first turn would talk through now.
pub fn next(shared: &Shared) -> Option<String> {
    let chosen = crate::settings::load(&shared.config.home).talk_account;
    let chosen = chosen.filter(|id| {
        id != AUTO && crate::accounts::find(&shared.config.agents.get(), id).is_some()
    });
    accounts(shared, chosen.as_deref(), None)
        .into_iter()
        .next()
        .map(|(_, account)| account.id)
}

/// The account a turn nobody watches runs on (a supervisor's, an event's): the first with room of
/// Claude Code and Codex, whose own tools obid can take away. Never Cursor, whose can't be: such a
/// turn reads only its job.
pub(super) fn unattended(shared: &Shared) -> Option<(Tool, Account)> {
    let tools = unwatched(tools(shared));
    let apart = nothing_else(shared);
    let lists = tools
        .into_iter()
        .map(|tool| (tool, shared.jobs.thinking_accounts(tool)))
        .collect();
    keep_apart(
        order(None, None, lists, true, |id| shared.jobs.has_room(id)),
        &apart,
    )
    .into_iter()
    .next()
}

fn unwatched(mut tools: Vec<Tool>) -> Vec<Tool> {
    tools.retain(|tool| *tool != Tool::Cursor);
    tools
}

/// The account the turn is off, having answered on `answered`: the one that failed this turn, else
/// the chosen one, else the one its session was on. [`crate::conversation::Conversations::moved_off`]
/// keeps it, so the turn says so once.
pub(super) fn off(
    failed: Option<&str>,
    chosen: Option<&str>,
    on: Option<&str>,
    answered: &str,
) -> Option<String> {
    failed
        .or(chosen)
        .or(on)
        .filter(|was| *was != answered)
        .map(str::to_owned)
}

/// What a turn adds when it reached Cursor without the owner choosing it: what stays with the
/// conversation from then on (super::harness_cursor). Words for Sangam to change
/// (docs/contracts/conversation-agent.md, proposed).
pub(super) const ON_CURSOR_UNASKED: &str =
    "On Cursor, what you ask through my tools needs your OK from now on in this conversation.";

/// Why a turn has no account to try, in one line: with failover off, why the chosen one can't run;
/// else what holds each signed-in account back, every tool's; none signed in, why the first has none.
pub(super) fn why_none(shared: &Shared, chosen: Option<&str>) -> String {
    let now = obi_protocol::time::now();
    let held = |tool: Tool, account: &Account| {
        // OpenAI's limit names no time, and the hour obid inferred isn't one to say.
        if crate::chatgpt::marker_of(account).is_some()
            && shared
                .jobs
                .usage
                .get(&account.id)
                .full_until_at(now)
                .is_some()
        {
            return format!(
                "{} · {}: {}",
                tool.name(),
                account.name,
                crate::chatgpt_relay::LIMIT_WORDS
            );
        }
        format!(
            "{} · {} {}",
            tool.name(),
            account.name,
            shared
                .jobs
                .usage
                .get(&account.id)
                .held(now)
                .unwrap_or_else(|| "can't run now".into())
        )
    };
    if !shared.jobs.account_failover()
        && let Some((tool, account)) =
            chosen.and_then(|id| crate::accounts::find(&shared.config.agents.get(), id))
    {
        let signed_in = shared.jobs.signed_in_accounts(tool);
        return if signed_in.iter().any(|a| a.id == account.id) {
            held(tool, &account)
        } else {
            format!("{} · {} isn't signed in", tool.name(), account.name)
        };
    }
    let tools = tools(shared);
    let said: Vec<String> = tools
        .iter()
        .flat_map(|tool| {
            let accounts = shared.jobs.signed_in_accounts(*tool);
            accounts
                .into_iter()
                .map(|a| held(*tool, &a))
                .collect::<Vec<_>>()
        })
        .collect();
    if said.is_empty() {
        return shared.jobs.none_signed_in(tools[0]);
    }
    said.join("; ")
}

/// The line that says the turn moved: `left` by its name, then where to. Words for Sangam to change
/// (docs/contracts/conversation-agent.md, proposed).
pub(super) fn moved_line(
    shared: &Shared,
    left: &str,
    tool: Tool,
    account: &Account,
) -> Option<String> {
    let (_, was) = crate::accounts::find(&shared.config.agents.get(), left)?;
    let now = obi_protocol::time::now();
    let usage = shared.jobs.usage.get(left);
    let why = if usage.full_until_at(now).is_some() || usage.held(now).is_some() {
        "hit its limit"
    } else {
        "couldn't run"
    };
    Some(format!(
        "{} {why}, so I moved to {} · {}.",
        was.name,
        tool.name(),
        account.name
    ))
}

#[cfg(test)]
mod tests {
    /// Nobody moves on to or off a ChatGPT login without the owner's click: the rule by itself, which
    /// tests/chatgpt_api.rs holds through `accounts()`.
    #[test]
    fn a_chatgpt_login_never_takes_or_gives_a_turn_by_itself() {
        let account = |id: &str| Account {
            id: id.into(),
            name: id.into(),
            config_dir: None,
            payer: None,
        };
        let list = |ids: &[&str]| -> Vec<(Tool, Account)> {
            ids.iter().map(|id| (Tool::Codex, account(id))).collect()
        };
        let ids = |order: Vec<(Tool, Account)>| -> Vec<String> {
            order.into_iter().map(|(_, a)| a.id).collect()
        };
        let plan = |a: &Account| a.id.starts_with("plan");
        let off = |order, apart| ids(keep_chatgpt_apart(order, &apart, plan));
        assert_eq!(off(list(&["plan1", "a", "b"]), Apart::Off), ["a", "b"]);
        assert_eq!(
            off(list(&["a", "plan1"]), Apart::On("plan1".into())),
            ["plan1"]
        );
        // Full, so not in the list: nothing, never the next.
        assert!(off(list(&["a", "plan2"]), Apart::On("plan1".into())).is_empty());
        assert_eq!(off(list(&["plan1", "plan2"]), Apart::OnlyThese), ["plan1"]);
        assert!(off(list(&[]), Apart::OnlyThese).is_empty());
    }

    use super::*;

    fn account(id: &str) -> Account {
        Account {
            id: id.into(),
            name: id.into(),
            config_dir: None,
            payer: None,
        }
    }

    fn lists() -> Vec<(Tool, Vec<Account>)> {
        vec![
            (
                Tool::Claude,
                vec![account("c1"), account("c2"), account("c3")],
            ),
            (Tool::Codex, vec![account("x1")]),
            (Tool::Cursor, vec![account("u1")]),
        ]
    }

    fn ids(order: Vec<(Tool, Account)>) -> Vec<String> {
        order.into_iter().map(|(_, a)| a.id).collect()
    }

    #[test]
    fn conversation_agent_auto_goes_by_the_tools_order_and_keeps_its_session() {
        assert_eq!(
            ids(order(None, None, lists(), true, |_| true)),
            ["c1", "c2", "x1"]
        );
        assert_eq!(
            ids(order(None, Some("c2"), lists(), true, |_| true)),
            ["c2", "c1", "x1"]
        );
    }

    #[test]
    fn conversation_agent_the_chosen_account_comes_first_whatever_the_session_is_on() {
        assert_eq!(
            ids(order(Some("x1"), Some("c1"), lists(), true, |_| true)),
            ["x1", "c1", "c2"]
        );
        assert_eq!(
            ids(order(Some("u1"), None, lists(), true, |_| true)),
            ["u1", "c1", "c2"]
        );
    }

    #[test]
    fn conversation_agent_a_turn_past_a_full_tool_reaches_another() {
        // Two of one tool at most: Claude Code's third account never keeps Codex from its turn.
        assert_eq!(
            ids(order(Some("c3"), None, lists(), true, |_| true)),
            ["c3", "c1", "x1"]
        );
    }

    #[test]
    fn conversation_agent_a_chosen_account_that_cant_run_is_passed_only_with_failover_on() {
        assert_eq!(
            ids(order(Some("gone"), None, lists(), true, |_| true)),
            ["c1", "c2", "x1"]
        );
        assert_eq!(
            ids(order(Some("gone"), None, lists(), false, |_| true)),
            Vec::<String>::new()
        );
        assert_eq!(
            ids(order(Some("x1"), None, lists(), false, |_| true)),
            ["x1"]
        );
        assert_eq!(ids(order(None, None, lists(), false, |_| true)), ["c1"]);
    }

    #[test]
    fn conversation_agent_15_cursor_with_room_and_a_figure_stays_last_on_auto() {
        let lists = || {
            vec![
                (Tool::Cursor, vec![account("u1")]),
                (Tool::Codex, vec![account("x1")]),
            ]
        };
        assert_eq!(
            ids(order(None, None, lists(), true, |_| true)),
            ["x1", "u1"]
        );
        // Only Cursor has room: the one held back still comes first.
        let room = |id: &str| id == "u1";
        assert_eq!(ids(order(None, None, lists(), true, room)), ["x1", "u1"]);
        // The owner's choice still stands.
        assert_eq!(
            ids(order(Some("u1"), None, lists(), true, |_| true)),
            ["u1", "x1"]
        );
    }

    #[test]
    fn conversation_agent_10_a_turn_nobody_watches_never_runs_on_cursor() {
        assert_eq!(unwatched(Tool::ALL.to_vec()), [Tool::Claude, Tool::Codex]);
    }

    #[test]
    fn conversation_agent_auto_goes_where_there_is_room() {
        // Claude Code's are held back: Codex has room, whatever the tools' order.
        let room = |id: &str| !id.starts_with('c');
        assert_eq!(
            ids(order(None, None, lists(), true, room)),
            ["x1", "c1", "c2"]
        );
        // Held back still comes before Cursor, whatever Cursor's usage reads.
        assert_eq!(
            ids(order(None, None, lists(), true, |_| false)),
            ["c1", "c2", "x1"]
        );
        // The chosen one stays first, and off it, its own tool's with room before the rest's.
        let room = |id: &str| id != "c1";
        assert_eq!(
            ids(order(Some("c1"), None, lists(), true, room)),
            ["c1", "c2", "x1"]
        );
    }

    #[test]
    fn conversation_agent_is_off_what_failed_else_what_was_chosen_else_its_session() {
        // Failed this turn.
        assert_eq!(off(Some("c1"), None, None, "x1").as_deref(), Some("c1"));
        // Its session's account can't run now.
        assert_eq!(off(None, None, Some("c1"), "x1").as_deref(), Some("c1"));
        // The chosen one can't run, wherever the session is.
        assert_eq!(off(None, Some("c1"), None, "x1").as_deref(), Some("c1"));
        assert_eq!(
            off(None, Some("c1"), Some("x1"), "x1").as_deref(),
            Some("c1")
        );
        // The owner chose it.
        assert_eq!(off(None, Some("x1"), Some("c1"), "x1"), None);
        // Nothing changed.
        assert_eq!(off(None, None, Some("c1"), "c1"), None);
        assert_eq!(off(None, None, None, "c1"), None);
    }
}
