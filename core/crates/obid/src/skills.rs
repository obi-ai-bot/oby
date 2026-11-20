//! The owner's agent skills (docs/contracts/skills.md, SKILLS-1 to 12; docs/api/skills.md): what
//! each account's Claude Code or Codex has installed, listed for the message box's `/`, and the
//! form a picked skill reaches the agent in, as T3 Code does it.
//!
//! - Claude Code's are read from the account's own folder only (its `configDir`, or `~/.claude`
//!   for the default login), never another account's, by Claude Code's own rules as T3 Code's
//!   `ClaudeSkills.ts` follows them: the Mac's managed folder, the account's `skills/`, a
//!   project's `.claude/skills`, and each plugin `settings.json`'s `enabledPlugins` turns on that
//!   `plugins/installed_plugins.json` holds, as `<plugin>:<skill>`.
//! - Codex's are Codex's own `skills/list`, from an `app-server` on the account's own
//!   `CODEX_HOME`, with plug-ins off as a conversation's Codex runs ([`crate::accounts::ask_codex`]).
//!
//! It reads names, descriptions and a few switches; it runs nothing of a skill's and writes
//! nothing. A list is kept until a file it was read from changes ([`mark`]): the same mark tells
//! a conversation whether the agent waiting for its next turn started before an install
//! (SKILLS-9). Where a skill runs is the lanes' (crate::jobs::lanes): only an agent on the owner's
//! own settings has their skills, so a boxed turn or task is offered none.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agent::Tool;
use crate::api::{ApiError, Shared};
use crate::config::Account;

/// The most skills one listing keeps: past it, the rest isn't offered.
const MOST: usize = 500;
/// The most of a `SKILL.md` read for its frontmatter.
const HEAD: u64 = 64 * 1024;
/// The longest name a `skill` may be.
pub const LONGEST: usize = 128;

/// One skill as the box offers it (docs/api/skills.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub user_invocable: bool,
    /// `user`, `project`, `plugin`, `managed` or `system`.
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// Whether `name` is a skill's name as an agent calls it: letters, digits, `:`, `_` and `-`,
/// starting with a letter or digit and holding a letter, at most [`LONGEST`]: never a path, a
/// space or anything a prompt would read as more than the name.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= LONGEST
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))
        && name.chars().any(|c| c.is_ascii_alphabetic())
}

/// The owner's words as Claude Code starts a skill with them: `/<name>` opening the text block, the
/// words after it (SKILLS-6). Claude Code expands a skill only there, in the last text block.
pub fn claude_command(name: &str, words: &str) -> String {
    format!("/{name} {}", words.trim()).trim_end().to_owned()
}

/// The owner's words as Codex starts a skill with them: `$<name>` before them (SKILLS-7).
pub fn codex_mention(name: &str, words: &str) -> String {
    format!("${name} {}", words.trim()).trim_end().to_owned()
}

/// A Claude Code user message whose last text block is `command` (`/name words`), `lead` (all obid
/// adds before the owner's words) its first when there is any (SKILLS-6).
pub fn claude_message(lead: &str, command: &str) -> Value {
    let content = match lead.trim() {
        "" => json!(command),
        lead => json!([{"type": "text", "text": lead}, {"type": "text", "text": command}]),
    };
    json!({"type": "user", "message": {"role": "user", "content": content}})
}

// ---------------------------------------------------------------------------------------------
// Where they're read from

/// Claude Code's managed folder on this machine.
fn claude_managed() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/ClaudeCode")
    } else {
        PathBuf::from("/etc/claude-code")
    }
}

/// The account's own folder: its `configDir`, else the tool's own in the person's home.
pub fn folder(tool: Tool, account: &Account) -> Option<PathBuf> {
    if let Some(dir) = &account.config_dir {
        return Some(dir.clone());
    }
    let home = crate::jobs::login_home()?;
    match tool {
        Tool::Claude => Some(home.join(".claude")),
        Tool::Codex => Some(home.join(".codex")),
        Tool::Cursor => None,
    }
}

/// What one listing reads: the account's folder, the managed folder and a project's.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    pub tool: Tool,
    pub folder: PathBuf,
    pub managed: Option<PathBuf>,
    /// A task's worktree, whose own skills are its project's.
    pub project: Option<PathBuf>,
    /// The person's home, for Codex's `~/.agents/skills`.
    pub home: Option<PathBuf>,
}

impl Place {
    pub fn of(tool: Tool, account: &Account, project: Option<&Path>) -> Option<Place> {
        Some(Place {
            tool,
            folder: folder(tool, account)?,
            managed: (tool == Tool::Claude).then(claude_managed),
            project: project.map(Path::to_path_buf),
            home: crate::jobs::login_home(),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Claude Code (SKILLS-1 to 3)

#[derive(Debug, Clone)]
struct Root {
    dir: PathBuf,
    source: &'static str,
    /// A plugin's name: its skills are `<plugin>:<skill>`.
    plugin: Option<String>,
}

#[derive(Debug, Default)]
struct Settings {
    off: Vec<String>,
    plugins: BTreeMap<String, bool>,
}

/// Claude Code's settings files, lowest precedence first: the account's, the project's, managed.
fn settings_files(place: &Place) -> Vec<PathBuf> {
    let mut files = vec![place.folder.join("settings.json")];
    if let Some(project) = &place.project {
        files.push(project.join(".claude/settings.json"));
        files.push(project.join(".claude/settings.local.json"));
    }
    if let Some(managed) = &place.managed {
        files.push(managed.join("managed-settings.json"));
    }
    files
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// `skillOverrides` (a whole file's ignored when one of its values isn't Claude Code's) and
/// `enabledPlugins` (a value that isn't a boolean takes back a lower file's).
fn settings(place: &Place) -> Settings {
    let mut overrides: BTreeMap<String, String> = BTreeMap::new();
    let mut plugins = BTreeMap::new();
    for file in settings_files(place) {
        let Some(value) = read_json(&file) else {
            continue;
        };
        if let Some(map) = value["skillOverrides"].as_object() {
            let known = ["on", "name-only", "user-invocable-only", "off"];
            if map
                .values()
                .all(|v| v.as_str().is_some_and(|v| known.contains(&v)))
            {
                for (name, v) in map {
                    overrides.insert(name.clone(), v.as_str().unwrap_or_default().to_owned());
                }
            }
        }
        if let Some(map) = value["enabledPlugins"].as_object() {
            for (key, on) in map {
                match on.as_bool() {
                    Some(on) => plugins.insert(key.clone(), on),
                    None => plugins.remove(key),
                };
            }
        }
    }
    Settings {
        off: (overrides.into_iter())
            .filter(|(_, v)| v == "off")
            .map(|(k, _)| k)
            .collect(),
        plugins,
    }
}

/// Whether `inner` is inside `outer` once both are resolved.
fn inside(outer: &Path, inner: &Path) -> bool {
    match (std::fs::canonicalize(outer), std::fs::canonicalize(inner)) {
        (Ok(outer), Ok(inner)) => inner.starts_with(outer),
        _ => false,
    }
}

/// A plugin's skill folders: `skills/`, its own `SKILL.md` when it has neither, and those its
/// manifest names inside it.
fn plugin_roots(install: &Path, prefix: &str, manifest: Option<&Value>, out: &mut Vec<Root>) {
    let root = |dir: PathBuf| Root {
        dir,
        source: "plugin",
        plugin: Some(prefix.to_owned()),
    };
    let default = install.join("skills");
    let has_default = default.is_dir();
    if has_default {
        out.push(root(default));
    }
    let declared: Vec<String> = match manifest.map(|m| &m["skills"]) {
        Some(Value::String(one)) => vec![one.clone()],
        Some(Value::Array(all)) => all
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => vec![],
    };
    let has_declared = manifest.is_some_and(|m| !m["skills"].is_null());
    if !has_declared && !has_default && install.join("SKILL.md").is_file() {
        out.push(root(install.to_path_buf()));
    }
    for relative in declared {
        let dir = install.join(&relative);
        if inside(install, &dir) {
            out.push(root(dir));
        }
    }
}

/// Where Claude Code finds skills for `place`, first name winning: managed, the account's, the
/// project's, then each enabled plugin's. Also every file read on the way, for [`mark`].
fn claude_roots(place: &Place, settings: &Settings, read: &mut Vec<PathBuf>) -> Vec<Root> {
    let mut roots = vec![];
    if let Some(managed) = &place.managed {
        roots.push(Root {
            dir: managed.join(".claude/skills"),
            source: "managed",
            plugin: None,
        });
    }
    roots.push(Root {
        dir: place.folder.join("skills"),
        source: "user",
        plugin: None,
    });
    if let Some(project) = &place.project {
        roots.push(Root {
            dir: project.join(".claude/skills"),
            source: "project",
            plugin: None,
        });
    }
    let installed = place.folder.join("plugins/installed_plugins.json");
    read.push(installed.clone());
    let Some(installed) = read_json(&installed) else {
        return roots;
    };
    let project = (place.project.as_ref()).and_then(|p| std::fs::canonicalize(p).ok());
    let Some(plugins) = installed["plugins"].as_object() else {
        return roots;
    };
    let mut keys: Vec<&String> = plugins.keys().collect();
    keys.sort();
    for key in keys {
        if settings.plugins.get(key) != Some(&true) {
            continue;
        }
        let records = plugins[key].as_array().cloned().unwrap_or_default();
        let applies = |r: &&Value| match r["scope"].as_str() {
            Some("user" | "managed") => true,
            Some("project" | "local") => {
                let at = r["projectPath"]
                    .as_str()
                    .and_then(|p| std::fs::canonicalize(p).ok());
                at.is_some() && at == project
            }
            _ => false,
        };
        let applicable: Vec<&Value> = records.iter().filter(applies).collect();
        let Some(record) = (applicable.iter()).find(|r| {
            r["installPath"]
                .as_str()
                .is_some_and(|p| Path::new(p).exists())
        }) else {
            continue;
        };
        let Some(install) = record["installPath"].as_str().map(PathBuf::from) else {
            continue;
        };
        let manifest_file = install.join(".claude-plugin/plugin.json");
        read.push(manifest_file.clone());
        let manifest = read_json(&manifest_file);
        let prefix = (manifest.as_ref())
            .and_then(|m| m["name"].as_str())
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| key.split('@').next().unwrap_or_default().to_owned());
        if prefix.is_empty() {
            continue;
        }
        plugin_roots(&install, &prefix, manifest.as_ref(), &mut roots);
    }
    roots
}

/// One candidate: its root, the folder name it's listed by, its `SKILL.md`.
fn candidates(roots: &[Root]) -> Vec<(Root, String, PathBuf, bool)> {
    let mut out = vec![];
    for root in roots {
        let single = root.plugin.is_some() && root.dir.join("SKILL.md").is_file();
        if single {
            let name = root
                .dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push((root.clone(), name, root.dir.join("SKILL.md"), true));
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&root.dir) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names.into_iter().take(MOST) {
            let file = root.dir.join(&name).join("SKILL.md");
            out.push((root.clone(), name, file, false));
        }
    }
    out
}

/// A `SKILL.md`'s frontmatter: none, unreadable, or its keys, one-line values unquoted and block
/// values (`>`, `|`) joined.
fn frontmatter(text: &str) -> Option<Option<HashMap<String, String>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Some(None);
    }
    let mut keys: HashMap<String, String> = HashMap::new();
    let mut block: Option<(String, Vec<String>)> = None;
    for line in lines {
        if line.trim_end() == "---" {
            if let Some((key, parts)) = block.take() {
                keys.insert(key, parts.join(" "));
            }
            return Some(Some(keys));
        }
        if let Some((_, parts)) = &mut block {
            if line.starts_with([' ', '\t']) || line.trim().is_empty() {
                if !line.trim().is_empty() {
                    parts.push(line.trim().to_owned());
                }
                continue;
            }
            let (key, parts) = block.take().unwrap_or_default();
            keys.insert(key, parts.join(" "));
        }
        if line.trim().is_empty()
            || line.trim_start().starts_with('#')
            || line.starts_with([' ', '\t'])
        {
            continue;
        }
        let (key, value) = line.split_once(':')?;
        let key = key.trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
        let value = value.trim();
        if matches!(value, ">" | "|" | ">-" | "|-" | ">+" | "|+") {
            block = Some((key.to_owned(), vec![]));
            continue;
        }
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        keys.insert(key.to_owned(), value.to_owned());
    }
    // No closing line: not frontmatter Claude Code reads.
    None
}

/// YAML's switches as Claude Code reads them (1.1's `yes`, `on`, …).
fn switch(value: Option<&String>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "y" | "1" => Some(true),
        "false" | "no" | "off" | "n" | "0" => Some(false),
        _ => None,
    }
}

fn head(path: &Path) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    file.take(HEAD).read_to_string(&mut text).ok()?;
    Some(text)
}

/// One line of a description: no line breaks or other control characters, at most 300 characters.
fn one_line(text: &str) -> Option<String> {
    let line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let line: String = line.chars().take(300).collect();
    (!line.is_empty()).then_some(line)
}

/// Claude Code's skills for `place` (SKILLS-1 to 3), by name.
pub fn claude(place: &Place) -> Vec<Skill> {
    let settings = settings(place);
    let roots = claude_roots(place, &settings, &mut vec![]);
    let mut found: BTreeMap<String, Skill> = BTreeMap::new();
    for (root, entry, file, single) in candidates(&roots) {
        if found.len() >= MOST {
            break;
        }
        let Some(text) = head(&file) else {
            continue;
        };
        let Some(front) = frontmatter(&text) else {
            continue;
        };
        let get = |key: &str| front.as_ref().and_then(|f| f.get(key));
        let name = match &root.plugin {
            Some(prefix) => {
                // A folder's skill goes by its folder; a plugin's own `SKILL.md` by its name.
                let declared = (single.then(|| get("name")).flatten())
                    .map(|n| {
                        n.strip_prefix(&format!("{prefix}:"))
                            .unwrap_or(n)
                            .to_owned()
                    })
                    .filter(|n| !n.is_empty());
                let bare = declared.unwrap_or(entry.clone());
                let bare: String = bare
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect();
                format!("{prefix}:{bare}")
            }
            None => entry.trim().to_owned(),
        };
        if !valid_name(&name) || found.contains_key(&name) {
            continue;
        }
        if root.plugin.is_none() && settings.off.contains(&name) {
            continue;
        }
        found.insert(
            name.clone(),
            Skill {
                description: get("description").and_then(|d| one_line(d)),
                user_invocable: switch(get("user-invocable")) != Some(false),
                source: root.source.to_owned(),
                plugin: root.plugin.clone(),
                name,
            },
        );
    }
    found.into_values().collect()
}

// ---------------------------------------------------------------------------------------------
// Codex (SKILLS-4)

/// The skills in Codex's answer to `skills/list` (`{"data": [{"cwd", "skills": [...]}]}`): those
/// on, by its own name, its short description first.
pub fn from_codex(answer: &Value) -> Vec<Skill> {
    let mut found: BTreeMap<String, Skill> = BTreeMap::new();
    let entries = answer["data"].as_array().cloned().unwrap_or_default();
    for skill in entries
        .iter()
        .flat_map(|e| e["skills"].as_array().cloned().unwrap_or_default())
    {
        let Some(name) = skill["name"].as_str().filter(|n| valid_name(n)) else {
            continue;
        };
        if skill["enabled"] == false || found.contains_key(name) || found.len() >= MOST {
            continue;
        }
        let description = [
            &skill["shortDescription"],
            &skill["interface"]["shortDescription"],
            &skill["description"],
        ]
        .into_iter()
        .find_map(|d| d.as_str().and_then(one_line));
        let plugin = skill["pluginId"].as_str().and_then(one_line);
        let source = match skill["scope"].as_str() {
            _ if plugin.is_some() => "plugin",
            Some("repo") => "project",
            Some("system") => "system",
            Some("admin") => "managed",
            _ => "user",
        };
        found.insert(
            name.to_owned(),
            Skill {
                name: name.to_owned(),
                description,
                user_invocable: true,
                source: source.to_owned(),
                plugin,
            },
        );
    }
    found.into_values().collect()
}

/// Codex's own listing for `place`, from an `app-server` started for it alone on the account's
/// `CODEX_HOME` in a folder of obid's, as a conversation's runs (plug-ins off).
async fn codex(shared: &Shared, account: &Account, place: &Place) -> Vec<Skill> {
    let Some(bin) = shared
        .jobs
        .agent_bin(Tool::Codex)
        .filter(|b| b.is_absolute())
    else {
        return vec![];
    };
    let dir = shared
        .config
        .home
        .join("plans")
        .join(uuid::Uuid::new_v4().to_string());
    if crate::store::make_private_dir(&dir).is_err() {
        return vec![];
    }
    let scratch = crate::model_lists::Scratch(dir);
    let cwd = place.project.clone().unwrap_or_else(|| scratch.0.clone());
    let params = json!({"cwds": [cwd]});
    let answer = crate::accounts::ask_codex(
        &bin,
        account.config_dir.as_deref(),
        &scratch.0,
        "skills/list",
        params,
    )
    .await;
    answer.map(|a| from_codex(&a)).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// The mark: what a listing was read from, as it is now (SKILLS-9)

fn stat_into(path: &Path, hasher: &mut impl Hasher) {
    path.hash(hasher);
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(hasher);
        meta.modified().ok().hash(hasher);
        meta.is_dir().hash(hasher);
    }
}

/// Every file and folder under `dir` to `depth`, not into hidden folders but `.system`.
fn tree_into(dir: &Path, depth: usize, hasher: &mut impl Hasher, budget: &mut usize) {
    stat_into(dir, hasher);
    if depth == 0 || *budget == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let hidden = (path.file_name()).is_some_and(|n| {
            let n = n.to_string_lossy();
            n.starts_with('.') && n != ".system"
        });
        if path.is_dir() && !hidden {
            tree_into(&path, depth - 1, hasher, budget);
        } else if path.file_name().is_some_and(|n| n == "SKILL.md") {
            stat_into(&path, hasher);
        }
    }
}

/// What `place`'s skills were read from, as it is now: the same number while nothing was
/// installed, removed, turned on or off or changed there. Reads only names and times.
pub fn mark(place: &Place) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    place.hash(&mut hasher);
    match place.tool {
        Tool::Claude => {
            for file in settings_files(place) {
                stat_into(&file, &mut hasher);
            }
            let mut read = vec![];
            let roots = claude_roots(place, &settings(place), &mut read);
            for file in read {
                stat_into(&file, &mut hasher);
            }
            for (_, _, file, _) in candidates(&roots) {
                stat_into(&file, &mut hasher);
            }
            for root in roots {
                stat_into(&root.dir, &mut hasher);
            }
        }
        Tool::Codex => {
            let mut budget = 4000;
            stat_into(&place.folder.join("config.toml"), &mut hasher);
            tree_into(&place.folder.join("skills"), 3, &mut hasher, &mut budget);
            tree_into(&place.folder.join("plugins"), 3, &mut hasher, &mut budget);
            if let Some(home) = &place.home {
                tree_into(&home.join(".agents/skills"), 3, &mut hasher, &mut budget);
            }
            if let Some(project) = &place.project {
                tree_into(&project.join(".agents/skills"), 3, &mut hasher, &mut budget);
                tree_into(&project.join(".codex/skills"), 3, &mut hasher, &mut budget);
            }
        }
        Tool::Cursor => {}
    }
    hasher.finish()
}

/// The mark of `account`'s own skills, for a run of `tool` (none for Cursor, nor a boxed run,
/// which has none of them): what a waiting agent's key carries (SKILLS-9).
pub fn mark_for(tool: Tool, account: &Account, own: bool) -> u64 {
    match Place::of(tool, account, None) {
        Some(place) if own && tool != Tool::Cursor => mark(&place),
        _ => 0,
    }
}

// ---------------------------------------------------------------------------------------------
// The listing, kept until its mark changes

type Kept = Mutex<HashMap<(String, Place), (u64, Vec<Skill>)>>;

fn kept() -> &'static Kept {
    static KEPT: std::sync::OnceLock<Kept> = std::sync::OnceLock::new();
    KEPT.get_or_init(Kept::default)
}

/// `account`'s skills for a run of `tool` in `project` (none for Cursor), read again only when
/// what they're read from changed.
pub async fn of(
    shared: &Shared,
    tool: Tool,
    account: &Account,
    project: Option<&Path>,
) -> Vec<Skill> {
    let Some(place) = Place::of(tool, account, project).filter(|_| tool != Tool::Cursor) else {
        return vec![];
    };
    let now = mark(&place);
    let key = (account.id.clone(), place.clone());
    if let Some((at, skills)) = kept().lock().unwrap_or_else(|e| e.into_inner()).get(&key)
        && *at == now
    {
        return skills.clone();
    }
    let skills = match tool {
        Tool::Claude => {
            let place = place.clone();
            tokio::task::spawn_blocking(move || claude(&place))
                .await
                .unwrap_or_default()
        }
        Tool::Codex => codex(shared, account, &place).await,
        Tool::Cursor => vec![],
    };
    (kept().lock().unwrap_or_else(|e| e.into_inner())).insert(key, (now, skills.clone()));
    skills
}

/// Whether the owner may start `name` on `account`'s `tool` in `project`.
pub async fn offers(
    shared: &Shared,
    tool: Tool,
    account: &Account,
    project: Option<&Path>,
    name: &str,
) -> bool {
    of(shared, tool, account, project)
        .await
        .iter()
        .any(|s| s.name == name && s.user_invocable)
}

// ---------------------------------------------------------------------------------------------
// Where the owner's next words would go (SKILLS-5)

/// Who would hear the owner's next words in a box: the tool, account and project, or why none of
/// their skills runs there.
pub struct Hearer {
    pub tool: Option<Tool>,
    pub account: Option<Account>,
    pub project: Option<PathBuf>,
    pub why: Option<String>,
}

/// Why a boxed run of `tool` gets none of the owner's skills.
pub fn boxed_why(tool: Tool) -> String {
    match tool {
        Tool::Cursor => "Cursor runs in Oby's box here, without your skills.".into(),
        Tool::Codex => {
            "Codex runs your skills only on a codex login: here it runs in Oby's box.".into()
        }
        Tool::Claude => "This runs in Oby's box, without your skills.".into(),
    }
}

const NO_ACCOUNT: &str = "No account can talk here yet.";

/// A task's hearer: its agent while it's at work in the own lane (TASKCONV-33 to 37).
pub fn of_job(shared: &Shared, job: &crate::job::Job) -> Hearer {
    let tool = Tool::from_id(&job.agent);
    let account = tool.and_then(|t| {
        crate::accounts::find(&shared.config.agents.get(), &job.account)
            .filter(|(found, _)| *found == t)
            .map(|(_, a)| a)
    });
    let (Some(tool), Some(account)) = (tool, account) else {
        return Hearer {
            tool,
            account: None,
            project: None,
            why: Some(NO_ACCOUNT.into()),
        };
    };
    let at_work =
        job.status == crate::job::Status::Running && job.phase == Some(crate::job::Phase::Work);
    let why = if !at_work {
        Some("Skills go to a task while it's at work.".to_owned())
    } else if !crate::jobs::Jobs::own_lane(job, tool) {
        Some(boxed_why(tool))
    } else {
        None
    };
    Hearer {
        tool: Some(tool),
        project: job.worktree.as_ref().map(PathBuf::from),
        account: Some(account),
        why,
    }
}

/// The owner's `words` to `job` with the skill `name` they picked, in the form its agent starts it
/// with (SKILLS-11): only while the task is at work in the own lane and its agent has it.
pub async fn job_words(
    shared: &Shared,
    job: &crate::job::Job,
    name: &str,
    words: &str,
) -> Result<String, String> {
    let hearer = of_job(shared, job);
    if let Some(why) = hearer.why {
        return Err(why);
    }
    let (Some(tool), Some(account)) = (hearer.tool, hearer.account) else {
        return Err(NO_ACCOUNT.into());
    };
    if !offers(shared, tool, &account, hearer.project.as_deref(), name).await {
        return Err(format!(
            "{} · {} has no skill {name} here.",
            tool.name(),
            account.name
        ));
    }
    Ok(match tool {
        Tool::Codex => codex_mention(name, words),
        _ => claude_command(name, words),
    })
}

#[derive(Deserialize)]
pub struct Asked {
    conversation: Option<String>,
    job: Option<String>,
    account: Option<String>,
}

/// `GET /v1/skills?conversation=` / `?job=` / `?account=` / neither (docs/api/skills.md).
pub async fn list(
    State(shared): State<Arc<Shared>>,
    scope: Option<axum::Extension<crate::profiles::Scope>>,
    Query(asked): Query<Asked>,
) -> Result<Response, ApiError> {
    let scope = crate::profile_scope::of(&shared, scope);
    let hearer = match (&asked.conversation, &asked.job, &asked.account) {
        (Some(id), None, None) => {
            let gone = || ApiError::new(StatusCode::NOT_FOUND, "There's no such conversation.");
            let profile = shared.conversations.profile_of(id).ok_or_else(gone)?;
            if !scope.holds(profile.as_deref()) {
                return Err(gone());
            }
            crate::brain::harness::hearer(&shared, Some(id), &scope)
        }
        (None, Some(id), None) => {
            let gone = || ApiError::new(StatusCode::NOT_FOUND, "There's no such job.");
            let job = shared.jobs.get(id).ok_or_else(gone)?;
            if !scope.holds(job.profile.as_deref()) {
                return Err(gone());
            }
            of_job(&shared, &job)
        }
        (None, None, Some(id)) => {
            let gone = || ApiError::new(StatusCode::NOT_FOUND, "There's no such account.");
            let (tool, account) =
                crate::accounts::find(&shared.config.agents.get(), id).ok_or_else(gone)?;
            if !shared.profiles.accounts.holds(&scope, id) {
                return Err(gone());
            }
            Hearer {
                tool: Some(tool),
                why: (tool == Tool::Cursor).then(|| boxed_why(tool)),
                account: Some(account),
                project: None,
            }
        }
        (None, None, None) => crate::brain::harness::hearer(&shared, None, &scope),
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "Ask for one of conversation, job or account, or none.",
            ));
        }
    };
    let skills = match (&hearer.tool, &hearer.account, &hearer.why) {
        (Some(tool), Some(account), None) => {
            of(&shared, *tool, account, hearer.project.as_deref()).await
        }
        _ => vec![],
    };
    let mut body = json!({
        "tool": hearer.tool.map(|t| t.id()),
        "account": hearer.account.as_ref().map(|a| a.id.clone()),
        "skills": skills,
    });
    if let Some(why) = hearer.why {
        body["why"] = json!(why);
    }
    Ok((StatusCode::OK, axum::Json(body)).into_response())
}

/// `POST /v1/conversations/{id}/restart` (SKILLS-10): ⌘K's Restart agent.
pub async fn restart(
    State(shared): State<Arc<Shared>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    scope: Option<axum::Extension<crate::profiles::Scope>>,
) -> Result<Response, ApiError> {
    let scope = crate::profile_scope::of(&shared, scope);
    let gone = || ApiError::new(StatusCode::NOT_FOUND, "There's no such conversation.");
    let profile = shared.conversations.profile_of(&id).ok_or_else(gone)?;
    if !scope.holds(profile.as_deref()) {
        return Err(gone());
    }
    let restarted = shared.harness.restart(&id);
    Ok((StatusCode::OK, axum::Json(json!({"restarted": restarted}))).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("obid-skills-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn skill(dir: &Path, name: &str, front: &str) {
        write(
            &dir.join(name).join("SKILL.md"),
            &format!("---\n{front}\n---\n\nDo the thing.\n"),
        );
    }

    fn place(folder: &Path, project: Option<&Path>, managed: Option<&Path>) -> Place {
        Place {
            tool: Tool::Claude,
            folder: folder.to_path_buf(),
            managed: managed.map(Path::to_path_buf),
            project: project.map(Path::to_path_buf),
            home: None,
        }
    }

    fn names(skills: &[Skill]) -> Vec<&str> {
        skills.iter().map(|s| s.name.as_str()).collect()
    }

    /// SKILLS-1: Claude Code's skills come from the account's own folder, with their description and
    /// whether the owner may start them; another account's folder isn't read.
    #[test]
    fn skills_1_claude_codes_own_skills_from_the_accounts_folder() {
        let root = scratch("own");
        let (mine, theirs) = (root.join("mine"), root.join("theirs"));
        skill(
            &mine.join("skills"),
            "write-tests",
            "name: write-tests\ndescription: Writes the tests first.",
        );
        skill(
            &mine.join("skills"),
            "agent-only",
            "description: >\n  Only the agent\n  starts this.\nuser-invocable: false",
        );
        skill(
            &theirs.join("skills"),
            "not-mine",
            "description: Someone else's.",
        );
        let skills = claude(&place(&mine, None, None));
        assert_eq!(names(&skills), ["agent-only", "write-tests"]);
        assert_eq!(
            skills[1].description.as_deref(),
            Some("Writes the tests first.")
        );
        assert_eq!(skills[1].source, "user");
        assert!(skills[1].user_invocable);
        assert_eq!(
            skills[0].description.as_deref(),
            Some("Only the agent starts this.")
        );
        assert!(!skills[0].user_invocable);
    }

    /// SKILLS-2: an enabled plugin's skills are `<plugin>:<skill>`, named by its manifest; a plugin
    /// turned off, or installed for another project, gives none.
    #[test]
    fn skills_2_an_enabled_plugins_skills_are_plugin_colon_skill() {
        let root = scratch("plugins");
        let (folder, project, other) = (root.join("acct"), root.join("proj"), root.join("other"));
        for p in [&project, &other] {
            std::fs::create_dir_all(p).unwrap();
        }
        let install = |name: &str| root.join("cache").join(name);
        for (dir, manifest) in [
            ("review", r#"{"name": "review"}"#),
            ("quiet", r#"{"name": "quiet"}"#),
            ("elsewhere", r#"{"name": "elsewhere"}"#),
            ("named", r#"{"name": "renamed", "skills": ["./extra"]}"#),
        ] {
            write(&install(dir).join(".claude-plugin/plugin.json"), manifest);
            skill(
                &install(dir).join("skills"),
                "check-diff",
                "description: Reads the diff.",
            );
        }
        skill(
            &install("named").join("extra"),
            "lint",
            "description: Lints.",
        );
        let record = |dir: &str, scope: &str, at: Option<&Path>| json!([{"scope": scope, "installPath": install(dir), "projectPath": at}]);
        write(
            &folder.join("plugins/installed_plugins.json"),
            &json!({"plugins": {
                "review@market": record("review", "user", None),
                "quiet@market": record("quiet", "user", None),
                "elsewhere@market": record("elsewhere", "project", Some(&other)),
                "named@market": record("named", "user", None),
            }})
            .to_string(),
        );
        write(
            &folder.join("settings.json"),
            r#"{"enabledPlugins": {"review@market": true, "quiet@market": false, "elsewhere@market": true, "named@market": true}}"#,
        );
        let skills = claude(&place(&folder, Some(&project), None));
        assert_eq!(
            names(&skills),
            ["renamed:check-diff", "renamed:lint", "review:check-diff"]
        );
        let review = &skills[2];
        assert_eq!(review.source, "plugin");
        assert_eq!(review.plugin.as_deref(), Some("review"));
        // The other project's install is that project's.
        let there = claude(&place(&folder, Some(&other), None));
        assert!(names(&there).contains(&"elsewhere:check-diff"));
    }

    /// SKILLS-3: a skill turned off in the settings isn't listed, unreadable frontmatter is
    /// skipped, and the first of a name wins: managed, then the account's, then the project's.
    #[test]
    fn skills_3_settings_and_frontmatter_decide_what_is_offered() {
        let root = scratch("rules");
        let (folder, project, managed) =
            (root.join("acct"), root.join("proj"), root.join("managed"));
        skill(
            &managed.join(".claude/skills"),
            "deploy",
            "description: The managed one.",
        );
        skill(
            &folder.join("skills"),
            "deploy",
            "description: The owner's.",
        );
        skill(&folder.join("skills"), "hidden", "description: Turned off.");
        write(
            &folder.join("skills/broken/SKILL.md"),
            "---\nnot: [closed\n",
        );
        skill(
            &project.join(".claude/skills"),
            "local",
            "description: The project's.",
        );
        write(
            &folder.join("settings.json"),
            r#"{"skillOverrides": {"hidden": "off"}}"#,
        );
        let skills = claude(&place(&folder, Some(&project), Some(&managed)));
        assert_eq!(names(&skills), ["deploy", "local"]);
        assert_eq!(skills[0].description.as_deref(), Some("The managed one."));
        assert_eq!(skills[0].source, "managed");
        assert_eq!(skills[1].source, "project");
        // A value Claude Code doesn't know drops the whole file's overrides, as it does.
        write(
            &folder.join("settings.json"),
            r#"{"skillOverrides": {"hidden": "off", "deploy": "sometimes"}}"#,
        );
        let skills = claude(&place(&folder, None, None));
        assert!(names(&skills).contains(&"hidden"));
    }

    /// SKILLS-4: Codex's own listing, those on, its short description first, its scope as the
    /// source.
    #[test]
    fn skills_4_codex_lists_its_own() {
        let answer = json!({"data": [{"cwd": "/w", "skills": [
            {"name": "pdf", "description": "Long.", "shortDescription": "Reads PDFs.", "enabled": true, "scope": "user", "path": "/x"},
            {"name": "off", "description": "x", "enabled": false, "scope": "user", "path": "/y"},
            {"name": "builtin", "description": "Codex's.", "enabled": true, "scope": "system", "path": "/z"},
            {"name": "../escape", "description": "x", "enabled": true, "scope": "repo", "path": "/q"},
        ]}]});
        let skills = from_codex(&answer);
        assert_eq!(names(&skills), ["builtin", "pdf"]);
        assert_eq!(skills[1].description.as_deref(), Some("Reads PDFs."));
        assert_eq!(skills[0].source, "system");
    }

    /// SKILLS-6 and 7: the forms each agent understands; SKILLS-12: what may be a name.
    #[test]
    fn skills_6_and_7_each_agent_gets_its_own_form() {
        assert_eq!(
            claude_command("review:check-diff", " the diff \n"),
            "/review:check-diff the diff"
        );
        assert_eq!(codex_mention("pdf", "read it"), "$pdf read it");
        let alone = claude_message("", "/write-tests");
        assert_eq!(alone["message"]["content"], "/write-tests");
        let led = claude_message("Earlier turns…", "/write-tests now");
        let blocks = led["message"]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1]["text"], "/write-tests now");
        for good in ["a", "review:check-diff", "write_tests-2"] {
            assert!(valid_name(good), "{good}");
        }
        for bad in [
            "",
            "-x",
            "a b",
            "../x",
            "x/y",
            "123",
            "/x",
            &"a".repeat(129),
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    /// SKILLS-9: the mark changes when a skill is installed, a plugin turned on, or a skill's file
    /// changes, and stays while nothing does.
    #[test]
    fn skills_9_the_mark_moves_with_what_is_installed() {
        let root = scratch("mark");
        let folder = root.join("acct");
        skill(&folder.join("skills"), "one", "description: One.");
        let at = place(&folder, None, None);
        let first = mark(&at);
        assert_eq!(mark(&at), first);
        skill(&folder.join("skills"), "two", "description: Two.");
        let second = mark(&at);
        assert_ne!(second, first);
        write(&folder.join("settings.json"), r#"{"enabledPlugins": {}}"#);
        assert_ne!(mark(&at), second);
        // Codex: a skill added under its CODEX_HOME.
        let codex = Place {
            tool: Tool::Codex,
            ..at
        };
        let before = mark(&codex);
        skill(&folder.join("skills/.system"), "builtin", "description: x");
        assert_ne!(mark(&codex), before);
    }
}
