//! Cursor as a conversation's agent (docs/contracts/conversation-agent.md, docs/conversation-harness.md,
//! step 4). The turn runs as the Claude Code and Codex sessions do ([`super::harness`]: the same
//! room, box, retries, events, scrubbing and rule for every permission), over ACP
//! ([`crate::acp`]): one session per conversation, picked up with `session/load`, with Oby's MCP
//! server and obid's Tetrix server in `session/new`.
//!
//! What Cursor doesn't let obid do is why it differs. Its own reads and edits never ask, and no flag
//! takes its tools away (docs/cursor.md, "Edits are not gated"), so:
//! - a turn on Cursor counts from its start as having read something untrusted, and marks the
//!   conversation for the owner's secrets: what obid can't see it reading, it can't call clean;
//! - its only way off the Mac is obid's egress proxy, to Cursor's own services and nothing else
//!   ([`crate::jobs::Jobs::reach_now`]): what it read, its own login copy too, can go only there.
//!   Those services do search the web for it (crate::acp, `search`), so one that's been talked into
//!   it can carry what it read out in a search, and obid can't gate that (the contract's known
//!   limits). Where the box can't hold it to the proxy, it doesn't talk (macOS's Seatbelt does);
//! - its commands ask, and obid's rule refuses them all (`oby_shell` is how a conversation runs one).

use serde_json::{Value, json};

use super::harness::{Place, orientation};
use crate::agent::Launch;
use crate::api::Shared;

/// What a conversation's egress door is called: `conversation-<id>`, never a job's id.
pub const DOOR: &str = "conversation-";

/// Why the conversation counts as having read something it can't tell.
pub(super) const MARK: &str = "it talked on Cursor, whose own reads obid can't see";

/// What the orientation adds on Cursor, in the first prompt of its session: ACP has no system prompt.
const ON_CURSOR: &str = "\n\nYou run on Cursor here. Your own shell is refused: run commands with oby_shell. You reach only Cursor's own services, not the web: say so when that stops you. Oby can't see what your own tools read, so this conversation counts as having read something untrusted: what the owner asks through Oby's tools may become a Confirm card they answer with one click.";

/// The first prompt of a Cursor session: what its session has no other way of being told, then
/// `first`.
pub(super) fn first_prompt(shared: &Shared, tetrix: bool, first: &str) -> Result<String, String> {
    let told = crate::vault::for_agent(&format!("{}{ON_CURSOR}", orientation(shared, tetrix)))?;
    Ok(format!("{told}\n\n{first}"))
}

/// The conversation's door.
pub(super) fn door(conversation: &str) -> String {
    format!("{DOOR}{conversation}")
}

/// The `cursor-agent acp` to run for one attempt of the turn, and what its session is started with:
/// `resume`'s when there is one, Oby's server at `url` and Tetrix's at `tetrix` with the turn's
/// `token`, which goes over its stdin and never in an argument, its environment or a file of obid's.
#[allow(clippy::too_many_arguments)]
pub(super) fn launch(
    shared: &Shared,
    account: &crate::config::Account,
    place: &Place,
    conversation: &str,
    url: &str,
    tetrix: Option<&str>,
    token: &str,
    resume: &Option<String>,
) -> Result<(Launch, crate::acp::Options), String> {
    crate::secrets::known().learn(
        &format!("conversation {} MCP token", place.folder.display()),
        token,
    );
    let launch = shared.jobs.conversation_cursor_launch(
        account,
        &place.room,
        &place.temp,
        &place.cursor,
        vec![place.repos.clone()],
        place.folder.join("agent.log"),
        &door(conversation),
    )?;
    let mut options = options(&account.name, url, tetrix, token, resume);
    let offered = shared
        .jobs
        .model_lists
        .for_account(crate::agent::Tool::Cursor, &account.id);
    options.model = Some(crate::settings::account_model(
        &shared.config.home,
        crate::agent::Tool::Cursor,
        &account.id,
        &offered,
    ));
    Ok((launch, options))
}

fn options(
    account: &str,
    url: &str,
    tetrix: Option<&str>,
    token: &str,
    resume: &Option<String>,
) -> crate::acp::Options {
    let server = |name: &str, url: &str| -> Value {
        json!({"type": "http", "name": name, "url": url,
               "headers": [{"name": "Authorization", "value": format!("Bearer {token}")}]})
    };
    let mut mcp = vec![server("oby", url)];
    mcp.extend(tetrix.map(|tetrix| server("tetrix", tetrix)));
    crate::acp::Options {
        tool: "Cursor".into(),
        account: account.to_owned(),
        resume: resume.clone(),
        // A Cursor that can't load its session fails this attempt, and the next starts a new one
        // with the earlier turns handed over; a new one here would get only the owner's last words.
        resume_only: true,
        model: None,
        plan: false,
        plan_mode: "plan".into(),
        agent_mode: "agent".into(),
        mcp,
        quiet_load: true,
    }
}
