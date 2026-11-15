//! Helpers (docs/contracts/helpers.md): an assistant of the owner's in one profile, with standing
//! instructions, one tool, the connectors it may use and one conversation, kept in
//! `home/helpers.json` (0600). Its turns run only on its tool, are told its instructions and get
//! only its granted connectors, each decision through [`may_use`]. The app calls them Agents.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::agent::Tool;
use crate::api::{ApiError, Shared, answer};

/// The most helpers obid keeps.
const MOST: usize = 50;
/// Oby's tools that reach GitHub with its own login: the `github` connector.
pub const GITHUB_TOOLS: [&str; 3] = ["oby_github", "oby_github_write", "oby_merge"];
/// Why a call of a connector its helper wasn't given is refused.
pub const NOT_GRANTED: &str =
    "This helper wasn't given that connector: the owner gives it in the helper's settings.";
const NO_HELPER: &str = "There's no such helper.";

/// A connector a helper may be given, in the order its list keeps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Connector {
    Github,
    Tetrix,
    Web,
}

impl Connector {
    pub fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// The connectors a turn was granted, one [`Connector::bit`] each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Granted(u8);

impl Granted {
    pub const NONE: Granted = Granted(0);

    pub fn with(self, connector: Connector) -> Granted {
        Granted(self.0 | connector.bit())
    }
}

/// Whether a turn granted `granted` may use `asked`: every grant decision is this one.
pub fn may_use(granted: Granted, asked: Connector) -> bool {
    granted.0 & asked.bit() != 0
}

/// One helper, as `helpers.json` keeps it and `/v1/helpers` shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Helper {
    pub id: String,
    pub profile: String,
    pub name: String,
    pub color: String,
    pub instructions: String,
    /// Its tool's [`Tool::id`].
    pub tool: String,
    pub connectors: Vec<Connector>,
    pub conversation: String,
    pub created: String,
}

/// What a helper's turn is given as it starts: its grants, and its orientation's section, the
/// vault's secrets taken out.
pub struct Told {
    pub granted: Granted,
    pub section: String,
}

impl Helper {
    pub fn granted(&self) -> Granted {
        (self.connectors.iter()).fold(Granted::NONE, |granted, c| granted.with(*c))
    }

    /// What its turn is given; fails, so no turn starts, when the vault can't be read.
    pub fn told(&self) -> Result<Told, String> {
        let names = serde_json::to_string(&self.connectors).unwrap_or_default();
        let section = format!(
            "\n\nIn this conversation you are {}, a helper the owner made in Oby. The owner's standing instructions for you, for every turn here:\n{}\nOf Oby's connectors (github, tetrix, web) you have {names}: any other is refused.",
            self.name, self.instructions
        );
        let (granted, section) = (self.granted(), crate::vault::for_agent(&section)?);
        Ok(Told { granted, section })
    }
}

#[derive(Deserialize)]
struct Saved {
    helpers: Vec<Helper>,
}

/// What `POST` and `PATCH` take; a key left out stays as it is.
#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    name: Option<String>,
    color: Option<String>,
    instructions: Option<String>,
    tool: Option<String>,
    connectors: Option<Vec<String>>,
    profile: Option<String>,
}

pub struct Helpers {
    file: PathBuf,
    helpers: Mutex<Vec<Helper>>,
    /// The file is there and doesn't read: no helpers, and nothing written over it.
    unread: bool,
    /// The conversations of the helpers deleted since obid started: a turn still in one gets no
    /// connector. A conversation goes with its helper, so no new turn starts in one.
    deleted: Mutex<Vec<String>>,
}

impl Helpers {
    pub fn open(home: &Path) -> Helpers {
        let file = home.join("helpers.json");
        let read = std::fs::read(&file)
            .map_err(|e| (e.kind() != std::io::ErrorKind::NotFound).then(|| e.to_string()))
            .and_then(|b| serde_json::from_slice::<Saved>(&b).map_err(|e| Some(e.to_string())));
        if let Err(Some(why)) = &read {
            let shown = file.display();
            tracing::error!("can't read {shown}: no helpers until it reads ({why})");
        }
        Helpers {
            unread: matches!(read, Err(Some(_))),
            helpers: Mutex::new(read.map(|saved| saved.helpers).unwrap_or_default()),
            file,
            deleted: Mutex::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Helper>> {
        self.helpers.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The helpers to change and [`Self::keep`], unless the file doesn't read.
    fn writable(&self) -> Result<MutexGuard<'_, Vec<Helper>>, ApiError> {
        if self.unread {
            let why = "helpers.json doesn't read, so obid changes no helper until it does.";
            return Err(ApiError::new(StatusCode::SERVICE_UNAVAILABLE, why));
        }
        Ok(self.lock())
    }

    fn keep(&self, helpers: &[Helper]) -> Result<(), ApiError> {
        let saved = serde_json::json!({ "helpers": helpers }).to_string();
        crate::store::write_private(&self.file, saved.as_bytes()).map_err(|e| {
            tracing::error!("can't keep {}: {e}", self.file.display());
            let why = "The helpers couldn't be kept.";
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, why)
        })
    }

    /// The helper whose conversation is `conversation`, as kept now.
    pub fn of(&self, conversation: &str) -> Option<Helper> {
        let helpers = self.lock();
        (helpers.iter().find(|h| h.conversation == conversation)).cloned()
    }

    /// Whether a turn in `conversation` may use `asked` now: any but a helper's not granted it, and
    /// none in a deleted helper's.
    pub fn allows(&self, conversation: &str, asked: Connector) -> bool {
        let deleted = self.deleted.lock().unwrap_or_else(|e| e.into_inner());
        if deleted.iter().any(|d| d == conversation) {
            return false;
        }
        drop(deleted);
        (self.of(conversation)).is_none_or(|h| may_use(h.granted(), asked))
    }

    /// Removes the helper `id`, kept, its conversation from then on a deleted helper's.
    fn remove(&self, id: &str) -> Result<Helper, ApiError> {
        let mut helpers = self.writable()?;
        let at = (helpers.iter().position(|h| h.id == id))
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, NO_HELPER))?;
        let gone = helpers.remove(at);
        if let Err(e) = self.keep(&helpers) {
            helpers.insert(at, gone);
            return Err(e);
        }
        let mut deleted = self.deleted.lock().unwrap_or_else(|e| e.into_inner());
        deleted.push(gone.conversation.clone());
        Ok(gone)
    }
}

fn unprocessable(why: &str) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why)
}

fn parsed(body: &Bytes) -> Result<Body, ApiError> {
    serde_json::from_slice(body)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("That isn't a helper: {e}")))
}

/// `helper` with `body`'s keys, checked.
fn changed(helper: &mut Helper, body: Body) -> Result<(), ApiError> {
    let trimmed = |text: String| text.trim().to_owned();
    helper.name = body.name.map_or(helper.name.clone(), trimmed);
    helper.color = body.color.unwrap_or(helper.color.clone());
    helper.instructions = body
        .instructions
        .map_or(helper.instructions.clone(), trimmed);
    helper.tool = body.tool.unwrap_or(helper.tool.clone());
    if let Some(asked) = body.connectors {
        let connectors = asked
            .iter()
            .map(|c| serde_json::from_value(c.as_str().into()));
        helper.connectors = (connectors.collect::<Result<Vec<Connector>, _>>())
            .map_err(|_| unprocessable("connectors are each github, tetrix or web."))?;
        helper.connectors.sort();
        helper.connectors.dedup();
    }
    let hex = helper.color.strip_prefix('#').unwrap_or_default();
    if !crate::conversation::one_line(&helper.name) {
        Err(unprocessable("name is one line of 1 to 100 characters."))
    } else if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Err(unprocessable("color is # and 6 hex digits, like #3a5a8c."))
    } else if !(1..=4000).contains(&helper.instructions.chars().count()) {
        Err(unprocessable("instructions are 1 to 4,000 characters."))
    } else if Tool::from_id(&helper.tool).is_none() {
        Err(unprocessable("tool is claude, codex or cursor."))
    } else if helper.tool == Tool::Cursor.id() && !helper.connectors.contains(&Connector::Web) {
        let why = "A Cursor helper has the web: obid can't take Cursor's own web search away.";
        Err(unprocessable(why).because("cursor-needs-web"))
    } else {
        Ok(())
    }
}

/// `GET /v1/helpers`: every helper, oldest first.
pub async fn list(State(shared): State<Arc<Shared>>) -> Response {
    answer(StatusCode::OK, &*shared.helpers.lock())
}

/// `POST /v1/helpers`: a helper in a profile, with its conversation.
pub async fn create(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    let body = parsed(&body)?;
    let profile = (shared.profiles.id_of(body.profile.as_deref()))
        .ok_or_else(|| unprocessable("profile is a profile on this Mac."))?;
    let mut helper = Helper {
        id: uuid::Uuid::new_v4().to_string().to_lowercase(),
        profile,
        created: obi_protocol::time::format(obi_protocol::time::now()),
        ..Helper::default()
    };
    changed(&mut helper, body)?;
    let mut helpers = shared.helpers.writable()?;
    if helpers.len() >= MOST {
        return Err(unprocessable("Oby keeps at most 50 helpers."));
    }
    helper.conversation = (shared.conversations).start_helper(&helper.name, &helper.id)?;
    helpers.push(helper.clone());
    if let Err(e) = shared.helpers.keep(&helpers) {
        helpers.pop();
        let _ = shared.conversations.delete_any(&helper.conversation, true);
        return Err(e);
    }
    Ok(answer(StatusCode::CREATED, &helper))
}

/// `PATCH /v1/helpers/{id}`: any of its keys but its profile, which never changes.
pub async fn change(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let body = parsed(&body)?;
    if body.profile.is_some() {
        return Err(unprocessable("A helper's profile never changes."));
    } else if body == Body::default() {
        return Err(unprocessable(
            "Send name, color, instructions, tool or connectors.",
        ));
    }
    let mut helpers = shared.helpers.writable()?;
    let at = (helpers.iter().position(|h| h.id == id))
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, NO_HELPER))?;
    let mut helper = helpers[at].clone();
    changed(&mut helper, body)?;
    let kept = std::mem::replace(&mut helpers[at], helper.clone());
    if let Err(e) = shared.helpers.keep(&helpers) {
        helpers[at] = kept;
        return Err(e);
    }
    Ok(answer(StatusCode::OK, &helper))
}

/// `DELETE /v1/helpers/{id}`: the helper and the turn under way in its conversation, then the
/// conversation as `DELETE /v1/conversations/{id}` deletes one; the jobs it started stay.
pub async fn delete(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
) -> Result<Response, ApiError> {
    let gone = shared.helpers.remove(&id)?;
    shared.turns.cancel(Some(&gone.conversation));
    if let Err(e) = crate::conversation::forget(&shared, &gone.conversation, true).await {
        tracing::warn!("helper {id}'s conversation wasn't deleted: {e:?}");
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn body(value: Value) -> Body {
        serde_json::from_value(value).unwrap()
    }

    fn made(value: Value) -> Result<Helper, ApiError> {
        let mut helper = Helper::default();
        changed(&mut helper, body(value)).map(|()| helper)
    }

    fn valid() -> Value {
        json!({"name": "Inbox helper", "color": "#3a5a8c", "instructions": "Sort my inbox.",
               "tool": "claude", "connectors": ["web", "github", "web"]})
    }

    /// HELPERS-1: a helper's JSON has exactly its keys, its connectors in their order, once each.
    #[test]
    fn helpers_1_a_helper_s_json_has_exactly_its_keys() {
        let helper = made(valid()).unwrap();
        assert_eq!(helper.connectors, [Connector::Github, Connector::Web]);
        let shown = serde_json::to_value(&helper).unwrap();
        let keys: Vec<&str> = shown
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut expected = vec![
            "id",
            "profile",
            "name",
            "color",
            "instructions",
            "tool",
            "connectors",
            "conversation",
            "created",
        ];
        expected.sort();
        let mut keys = keys;
        keys.sort();
        assert_eq!(keys, expected);
        assert_eq!(shown["connectors"], json!(["github", "web"]));
    }

    /// HELPERS-2: each rule's refusal is a 422 that names it, and a good helper passes.
    #[test]
    fn helpers_2_each_refusal_names_its_rule() {
        let with = |key: &str, value: Value| {
            let mut asked = valid();
            asked[key] = value;
            asked
        };
        let mut cursor = with("tool", json!("cursor"));
        cursor["connectors"] = json!(["github"]);
        for (asked, rule) in [
            (with("name", json!("  ")), "name is one line"),
            (with("name", json!("two\nlines")), "name is one line"),
            (with("name", json!("n".repeat(101))), "name is one line"),
            (with("color", json!("blue")), "color is #"),
            (with("color", json!("#3a5a8")), "color is #"),
            (
                with("instructions", json!(" ")),
                "instructions are 1 to 4,000",
            ),
            (
                with("instructions", json!("i".repeat(4001))),
                "instructions are 1 to 4,000",
            ),
            (
                with("tool", json!("gemini")),
                "tool is claude, codex or cursor",
            ),
            (with("connectors", json!(["gmail"])), "connectors are each"),
            (cursor.clone(), "A Cursor helper has the web"),
        ] {
            let refused = made(asked.clone()).unwrap_err();
            assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{asked}");
            assert!(
                refused.message.contains(rule),
                "{asked}: {}",
                refused.message
            );
        }
        assert_eq!(made(cursor).unwrap_err().reason, Some("cursor-needs-web"));
        assert!(made(with("name", json!("n".repeat(100)))).is_ok());
        assert!(made(with("instructions", json!("i".repeat(4000)))).is_ok());
        let unknown = serde_json::from_value::<Body>(json!({"nickname": "x"}));
        assert!(unknown.is_err(), "an unknown key is refused");
        let not_strings = serde_json::from_value::<Body>(json!({"connectors": [1]}));
        assert!(not_strings.is_err(), "connectors are strings");
    }

    /// HELPERS-8: `may_use` is true exactly for a connector in the set, for all 8 sets.
    #[test]
    fn helpers_8_may_use_is_true_exactly_for_what_was_granted() {
        let all = [Connector::Github, Connector::Tetrix, Connector::Web];
        for set in 0..8u8 {
            let held: Vec<Connector> = (0..3)
                .filter(|i| (set >> i) & 1 == 1)
                .map(|i| all[i])
                .collect();
            let granted = held.iter().fold(Granted::NONE, |g, c| g.with(*c));
            for asked in all {
                assert_eq!(
                    may_use(granted, asked),
                    held.contains(&asked),
                    "{held:?} {asked:?}"
                );
            }
        }
    }

    /// HELPERS-7: a deleted helper's conversation gets no connector, not even one it was granted.
    #[test]
    fn helpers_7_a_deleted_helper_s_conversation_gets_no_connector() {
        let home = std::env::temp_dir().join(format!("obid-helpers-7-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let mut granted = valid();
        granted["connectors"] = json!(["github", "tetrix", "web"]);
        let helper = Helper {
            id: "h1".into(),
            conversation: "c1".into(),
            ..made(granted).unwrap()
        };
        let saved = json!({ "helpers": [helper] }).to_string();
        std::fs::write(home.join("helpers.json"), saved).unwrap();
        let helpers = Helpers::open(&home);
        let all = [Connector::Github, Connector::Tetrix, Connector::Web];
        assert!(all.iter().all(|c| helpers.allows("c1", *c)));
        helpers.remove("h1").unwrap();
        assert!(
            all.iter().all(|c| !helpers.allows("c1", *c)),
            "it fails closed"
        );
        assert!(
            helpers.allows("c2", Connector::Github),
            "another conversation is as it was"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// HELPERS-11: a helper's conversation outlives 200 of the owner's, and is never the latest.
    #[test]
    fn helpers_11_the_conversation_stays_and_is_never_the_latest() {
        let home = std::env::temp_dir().join(format!("obid-helpers-11-{}", uuid::Uuid::new_v4()));
        let conversations =
            crate::conversation::Conversations::open(&home, crate::events::Events::new());
        let helper = conversations.start_helper("Inbox helper", "h1").unwrap();
        let mine: Vec<String> = (0..=crate::conversation::CONVERSATIONS)
            .map(|i| conversations.start_for(&format!("Mine {i}")).unwrap())
            .collect();
        assert!(conversations.has(&helper), "it isn't forgotten");
        assert!(
            mine[1..].iter().all(|id| conversations.has(id)),
            "it takes no room"
        );
        conversations
            .append(&helper, "user", "Sort it".into(), vec![], None)
            .unwrap();
        let latest = conversations.latest_id().unwrap();
        assert_ne!(latest, helper, "words with no conversation go elsewhere");
        let refused = conversations.delete(&helper);
        assert!(matches!(
            refused,
            Err(crate::conversation::Error::HelperConversation)
        ));
        assert!(conversations.has(&helper));
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proof of [`may_use`]: `kani::any()` stands for every grant set and every connector.

    use super::*;

    /// PROOF-1, HELPERS-8: `may_use` is true exactly when the connector asked is one granted.
    #[kani::proof]
    fn helpers_8_may_use_exactly_what_was_granted() {
        let (github, tetrix, web) = (
            kani::any::<bool>(),
            kani::any::<bool>(),
            kani::any::<bool>(),
        );
        let mut granted = Granted::NONE;
        if github {
            granted = granted.with(Connector::Github);
        }
        if tetrix {
            granted = granted.with(Connector::Tetrix);
        }
        if web {
            granted = granted.with(Connector::Web);
        }
        let asked = match kani::any::<u8>() {
            0 => Connector::Github,
            1 => Connector::Tetrix,
            _ => Connector::Web,
        };
        let held = match asked {
            Connector::Github => github,
            Connector::Tetrix => tetrix,
            Connector::Web => web,
        };
        assert_eq!(may_use(granted, asked), held);
    }
}
