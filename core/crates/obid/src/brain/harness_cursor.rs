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
//! - its commands ask, and obid's rule refuses them all: a boxed conversation runs none (CONVO-2).
//!
//! The owner's own conversation on macOS runs in the own lane instead (docs/contracts/lanes.md,
//! LANES-28): unboxed, on the account's own folder and login, no proxy, its commands run as the
//! owner's user under the owner's own Cursor settings, its file tools denied obid's secret places by
//! the room's `.cursor/cli.json` (`Jobs::conversation_own_cursor_launch`). Its reads stay unseen, so
//! it's marked as the boxed one is.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::harness::{Place, orientation};
use crate::agent::Launch;
use crate::api::Shared;
use crate::helpers::Told;

/// What a conversation's egress door is called: `conversation-<id>`, never a job's id.
pub const DOOR: &str = "conversation-";

/// Why the conversation counts as having read something it can't tell.
pub(super) const MARK: &str = "it talked on Cursor, whose own reads obid can't see";

/// What the orientation adds on Cursor, in the first prompt of its session: ACP has no system prompt.
const ON_CURSOR: &str = "\n\nYou run on Cursor here. Your own shell is refused: you run no commands. You reach only Cursor's own services, not the web: say so when that stops you. Oby can't see what your own tools read, so this conversation counts as having read something untrusted: what the owner asks through Oby's tools may become a Confirm card they answer with one click.";

/// [`ON_CURSOR`] for the owner's own conversation (LANES-28): its shell is its own.
const ON_OWN_CURSOR: &str = "\n\nYou run on Cursor here, in the owner's own lane: your shell runs as the owner's user, under their own Cursor settings, and Oby's secret files are kept from your file tools. Oby can't see what your own tools read, so this conversation counts as having read something untrusted: what the owner asks through Oby's tools may become a Confirm card they answer with one click.";

/// The first prompt of a Cursor session: what its session has no other way of being told, a
/// `helper`'s section among it, then `first`; an `own` one hears that its shell is its own.
pub(super) fn first_prompt(
    shared: &Shared,
    tetrix: bool,
    helper: Option<&Told>,
    first: &str,
    own: bool,
) -> Result<String, String> {
    let oriented = orientation(shared, tetrix, helper);
    let on = if own { ON_OWN_CURSOR } else { ON_CURSOR };
    let told = crate::vault::for_agent(&format!("{oriented}{on}"))?;
    Ok(format!("{told}\n\n{first}"))
}

/// The conversation's door.
pub(super) fn door(conversation: &str) -> String {
    format!("{DOOR}{conversation}")
}

/// The `cursor-agent acp` to run for one attempt of the turn, and what its session is started with:
/// `resume`'s when there is one, Oby's server at `url` and Tetrix's at `tetrix` with the turn's
/// `token`, which goes over its stdin and never in an argument, its environment or a file of obid's.
/// An `own` one runs in the own lane (LANES-28), with no door.
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
    own: bool,
) -> Result<(Launch, crate::acp::Options), String> {
    // Its sockets go under its folder: the login copy's, or an own one's account folder (LANES-28).
    let folder = match own {
        true => (account.config_dir.clone())
            .or_else(crate::jobs::login_home)
            .ok_or("obid can't find the home Cursor's login is in.")?,
        false => place.cursor.clone(),
    };
    let temp = temp_for(&folder, conversation)?;
    crate::secrets::known().learn(
        &format!("conversation {} MCP token", place.folder.display()),
        token,
    );
    let stderr = place.folder.join("agent.log");
    let launch = match own {
        true => shared
            .jobs
            .conversation_own_cursor_launch(account, &place.room, &temp, stderr)?,
        false => shared.jobs.conversation_cursor_launch(
            account,
            &place.room,
            &temp,
            &place.cursor,
            vec![place.repos.clone()],
            stderr,
            &door(conversation),
        )?,
    };
    let mut options = options(&account.name, url, tetrix, token, resume);
    let offered = shared
        .jobs
        .model_lists
        .to_check(crate::agent::Tool::Cursor, &account.id);
    options.model = Some(crate::settings::account_model(
        &shared.config.home,
        crate::agent::Tool::Cursor,
        &account.id,
        &offered,
    ));
    Ok((launch, options))
}

/// Where a conversation's Cursor keeps its sockets when its data folder (under obid's home) is too
/// long for them (CONVERSATION-AGENT-41): `TMPDIR/.cursor`, the next place Cursor tries, so its
/// `TMPDIR` is a folder of the conversation's own in the Mac's per-user temporary folder (the
/// user's alone, and short), never the conversation's `home`, which is longer still. Past it Cursor
/// takes `/tmp/.cursor`, which its box denies, and stops as it starts (cursor-agent 2026.11.13).
pub(super) fn temp_of(conversation: &str) -> PathBuf {
    let short: String = conversation
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    std::env::temp_dir().join(format!("oby-c-{short}"))
}

/// [`temp_of`], made fresh for an attempt (none of an earlier one's sockets or logs, never through
/// a link left in its place, the user's alone): or, saying why, a conversation whose Cursor would
/// find no room for its sockets there either.
fn temp_for(cursor: &Path, conversation: &str) -> Result<PathBuf, String> {
    let room = crate::acp::cursor::SOCKET_ROOM;
    let temp = temp_of(conversation);
    let data = crate::acp::cursor::data_dir(cursor).as_os_str().len();
    let spare = temp.join(".cursor").as_os_str().len();
    if data > room && spare > room {
        return Err(format!(
            "Cursor has no room for its sockets (its folder under obid's home would be {data} characters, and in the Mac's temporary folder {spare}, both past {room}), so a conversation doesn't run on it: give obid a shorter home"
        ));
    }
    // A link in its place goes, not what it leads to.
    let _ = std::fs::remove_dir_all(&temp);
    crate::store::make_private_dir(&temp)
        .map_err(|e| format!("obid can't make Cursor's temporary folder: {e}"))?;
    if !crate::sandbox::private_folder(&temp, nix::unistd::getuid().as_raw()) {
        return Err(
            "Cursor's temporary folder isn't obid's alone, so a conversation doesn't run on it"
                .into(),
        );
    }
    Ok(temp)
}

/// The turn ended: its Cursor's temporary folder goes.
pub(super) fn forget_temp(conversation: &str) {
    let _ = std::fs::remove_dir_all(temp_of(conversation));
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
        // A conversation's agent gets the browser with the rest of BROWSER-17.
        browser: None,
        quiet_load: true,
    }
}
