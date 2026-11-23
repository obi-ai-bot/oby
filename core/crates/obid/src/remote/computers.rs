//! The computers this Mac's obid keeps for its window (docs/contracts/remote.md, COMPUTERS-19 to
//! 24): which exist, what each is called, its routes from the link it was added with, its pairing
//! there, and which one the window is on. Their device tokens are in obid's vault, as
//! `oby.computer.<id>`, so every answer, log line and push has them taken out; never in
//! `home/computers.json` (0600) or an answer.
//!
//! It owns that file, the tokens' names, and each computer's [`Supervisor`], SSH tunnels
//! (crate::remote::tunnels) and door (crate::egress, COMPUTERS-27) while obid runs, the doors in
//! memory only. It never talks to a computer itself: crate::remote::reach does, with what it hands
//! out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::route::{Route, Via};
use super::supervisor::{Change, Supervisor, Tried};

/// The header a window's request names the computer it's for with (crate::remote::forward).
pub const HEADER: &str = "oby-computer";

/// How many answers in a row a better route needs before the window moves back up to it.
const UP_AFTER: u32 = 2;

/// One computer the window works on.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Computer {
    pub id: String,
    pub name: String,
    /// `mac` or `linux`, as its obid said when it was added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// In the owner's order: its direct routes, each pinned, and the SSH route it was added over.
    pub routes: Vec<Route>,
    /// Its pairing there (its gateway's `deviceID`), and this Mac's key's id there, if it took it.
    pub pairing: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// The device of this Mac's whose key that is (the one that signed the add): its words through
    /// this computer's token name its id there instead (crate::remote::forward).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<String>,
    pub added: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current: Option<String>,
    #[serde(default)]
    computers: Vec<Computer>,
}

pub struct Computers {
    file: PathBuf,
    vault: Arc<crate::vault::Vault>,
    saved: Mutex<Saved>,
    supervisors: Mutex<HashMap<String, Supervisor>>,
    /// Bumped whenever the window's computer changes or one is forgotten: a forwarded stream ends
    /// on it (crate::remote::forward).
    changes: tokio::sync::watch::Sender<u64>,
    /// The SSH tunnels to them, while obid runs.
    pub tunnels: super::tunnels::Tunnels,
    /// The listener their doors are on, started the first time one is asked for.
    doors: Mutex<Option<Arc<crate::egress::Egress>>>,
}

/// The vault name of computer `id`'s device token.
fn token_name(id: &str) -> String {
    format!("oby.computer.{id}")
}

/// A name a person gave: no control characters, at most 100 characters, trimmed.
pub fn clean_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .take(100)
        .collect::<String>()
        .trim()
        .to_owned()
}

impl Computers {
    /// The computers kept in `home/computers.json`. A file that doesn't read counts as none, and
    /// is left as it is until a change writes it again.
    pub fn open(home: &Path, vault: Arc<crate::vault::Vault>) -> Computers {
        let file = home.join("computers.json");
        let saved = std::fs::read(&file)
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<Saved>(&bytes)
                    .inspect_err(|e| tracing::warn!("{} doesn't read: {e}", file.display()))
                    .ok()
            })
            .unwrap_or_default();
        Computers {
            file,
            vault,
            saved: Mutex::new(saved),
            supervisors: Mutex::new(HashMap::new()),
            changes: tokio::sync::watch::channel(0).0,
            tunnels: super::tunnels::Tunnels::default(),
            doors: Mutex::new(None),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Saved> {
        self.saved.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn keep(&self, saved: &Saved) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(saved).map_err(|e| e.to_string())?;
        crate::store::write_private(&self.file, &bytes).map_err(|e| e.to_string())
    }

    /// The computer `id`, while this Mac keeps it.
    pub fn get(&self, id: &str) -> Option<Computer> {
        self.lock().computers.iter().find(|c| c.id == id).cloned()
    }

    /// Its device token, from the vault.
    pub fn token(&self, id: &str) -> Result<Option<String>, String> {
        self.vault.value(&token_name(id))
    }

    /// Keeps a computer paired with a code (crate::remote::code) or over SSH
    /// (crate::remote::adding), its token in the vault first: with no token kept, nothing is.
    pub fn add(
        &self,
        paired: super::code::Paired,
        os: Option<String>,
        signer: Option<String>,
    ) -> Result<Computer, String> {
        let id = {
            let mut bytes = [0u8; 8];
            getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
            hex::encode(bytes)
        };
        self.vault
            .add(&paired.token, &token_name(&id))
            .map_err(|e| format!("Oby couldn't keep that computer's token in its vault: {e}"))?;
        let computer = Computer {
            id: id.clone(),
            name: clean_name(&paired.name),
            os: os.filter(|os| matches!(os.as_str(), "mac" | "linux")),
            routes: paired.routes,
            pairing: paired.pairing,
            signer: signer.filter(|_| paired.device.is_some()),
            device: paired.device,
            added: obi_protocol::time::format(obi_protocol::time::now()),
        };
        let mut saved = self.lock();
        saved.computers.push(computer.clone());
        if let Err(e) = self.keep(&saved) {
            saved.computers.pop();
            let _ = self.vault.remove(&token_name(&id));
            return Err(format!("Oby couldn't keep that computer: {e}"));
        }
        Ok(computer)
    }

    /// Renames `id`, on this Mac only. False when there's no such computer.
    pub fn rename(&self, id: &str, name: &str) -> Result<bool, String> {
        let mut saved = self.lock();
        let Some(i) = saved.computers.iter().position(|c| c.id == id) else {
            return Ok(false);
        };
        let before = std::mem::replace(&mut saved.computers[i].name, clean_name(name));
        if let Err(e) = self.keep(&saved) {
            saved.computers[i].name = before;
            return Err(e);
        }
        Ok(true)
    }

    /// Told whenever the window's computer changes or one is forgotten.
    pub fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changes.subscribe()
    }

    fn changed(&self) {
        self.changes.send_modify(|n| *n += 1);
    }

    /// The computer the window is on: none is this Mac.
    pub fn current(&self) -> Option<String> {
        self.lock().current.clone()
    }

    /// Puts the window on `id` (none: this Mac). False when there's no such computer.
    pub fn set_current(&self, id: Option<&str>) -> Result<bool, String> {
        let mut saved = self.lock();
        if id.is_some_and(|id| !saved.computers.iter().any(|c| c.id == id)) {
            return Ok(false);
        }
        let before = std::mem::replace(&mut saved.current, id.map(str::to_owned));
        if let Err(e) = self.keep(&saved) {
            saved.current = before;
            return Err(e);
        }
        drop(saved);
        self.changed();
        Ok(true)
    }

    /// Forgets `id`: its record, its token and its supervisor; the window back on this Mac if it
    /// was on it. False when there's no such computer.
    pub fn forget(&self, id: &str) -> Result<bool, String> {
        let mut saved = self.lock();
        let before = saved.computers.len();
        saved.computers.retain(|c| c.id != id);
        if saved.computers.len() == before {
            return Ok(false);
        }
        if saved.current.as_deref() == Some(id) {
            saved.current = None;
        }
        self.keep(&saved)?;
        drop(saved);
        self.supervisors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        self.tunnels.forget(id);
        if let Some(doors) = &*self.doors.lock().unwrap_or_else(|e| e.into_inner()) {
            doors.close_computer(id);
        }
        self.changed();
        self.vault.remove(&token_name(id))?;
        Ok(true)
    }

    /// The proxy a page's `localhost` on computer `id` goes through (COMPUTERS-27), the same each
    /// time, until [`Computers::forget`]: none when there's no such computer. Its listener starts
    /// with `carrier` the first time any door is asked for.
    pub fn door(
        &self,
        id: &str,
        carrier: impl FnOnce() -> crate::egress::Carrier,
    ) -> Result<Option<String>, String> {
        // Held while the door opens, so a forget that comes now closes it.
        let saved = self.lock();
        if !saved.computers.iter().any(|c| c.id == id) {
            return Ok(None);
        }
        let mut doors = self.doors.lock().unwrap_or_else(|e| e.into_inner());
        let egress = match &*doors {
            Some(egress) => egress.clone(),
            None => {
                let started = crate::egress::Egress::start_computers(carrier())
                    .map_err(|e| format!("Oby couldn't open a door to that computer: {e}"))?;
                doors.insert(started).clone()
            }
        };
        Ok(Some(egress.open_computer(id)))
    }

    /// The routes to try for `id` now, best first: the one in use and those above it, as the
    /// supervisor says, or all of them before any answered.
    pub fn to_probe(&self, id: &str) -> Vec<Route> {
        let Some(computer) = self.get(id) else {
            return Vec::new();
        };
        self.with_supervisor(&computer, |s| s.to_probe())
    }

    /// What one try of `route` for `id` came to; what it changed, when it did.
    pub fn observe(&self, id: &str, route: &str, result: Result<(), String>) -> Option<Change> {
        let computer = self.get(id)?;
        self.with_supervisor(&computer, |s| {
            s.observe(route, result);
            s.decide()
        })
    }

    /// The route `id` is reached by now, and every route tried with why.
    pub fn route(&self, id: &str) -> (Option<Route>, Vec<Tried>) {
        let Some(computer) = self.get(id) else {
            return (None, Vec::new());
        };
        self.with_supervisor(&computer, |s| (s.current().cloned(), s.tried()))
    }

    fn with_supervisor<T>(&self, computer: &Computer, f: impl FnOnce(&mut Supervisor) -> T) -> T {
        let mut supervisors = self.supervisors.lock().unwrap_or_else(|e| e.into_inner());
        let supervisor = supervisors
            .entry(computer.id.clone())
            .or_insert_with(|| Supervisor::new(computer.routes.clone(), UP_AFTER));
        f(supervisor)
    }

    /// `GET /v1/computers`: `{"current": "this" | <id>, "computers": [...]}`, each with its state
    /// (`connected` on a route, `lost` with every route tried, `idle` before any was), never a
    /// token.
    pub fn list(&self) -> Value {
        let saved = self.lock();
        let current = saved.current.clone().unwrap_or_else(|| "this".into());
        let computers: Vec<Value> = saved
            .computers
            .clone()
            .iter()
            .map(|c| self.listed(c))
            .collect();
        drop(saved);
        json!({"current": current, "computers": computers})
    }

    /// One computer as the list shows it.
    pub fn listed(&self, computer: &Computer) -> Value {
        let (route, tried) = self.with_supervisor(computer, |s| (s.current().cloned(), s.tried()));
        let state = match (&route, tried.is_empty()) {
            (Some(_), _) => "connected",
            (None, true) => "idle",
            (None, false) => "lost",
        };
        let mut listed = json!({"id": computer.id, "name": computer.name, "state": state});
        if let Some(os) = &computer.os {
            listed["os"] = json!(os);
        }
        if let Some(route) = route {
            let address = match &route.via {
                Via::Https { host, port, .. } => format!("{host}:{port}"),
                Via::Ssh { destination, .. } => destination.clone(),
            };
            listed["route"] = json!({"id": route.id, "kind": route.kind, "address": address});
        }
        if state == "lost" {
            listed["tried"] = json!(tried);
        }
        listed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::pin::Pin;

    fn vault(home: &Path) -> Arc<crate::vault::Vault> {
        Arc::new(crate::vault::Vault::file(
            home.join("vault"),
            Arc::new(crate::secrets::Secrets::labelled("test")),
        ))
    }

    fn paired(name: &str) -> super::super::code::Paired {
        super::super::code::Paired {
            name: name.into(),
            routes: vec![
                Route::direct("direct-1", "https://192.168.1.20:8443", Pin([5; 32]), None).unwrap(),
            ],
            route: "direct-1".into(),
            token: "a-device-token-of-sixty-four-characters-at-least-0123456789abcd".into(),
            pairing: "pairing-1".into(),
            device: Some("key-1".into()),
        }
    }

    #[test]
    fn computers_19_a_computer_is_kept_its_token_only_in_the_vault() {
        let home = std::env::temp_dir().join(format!("obid-computers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let computers = Computers::open(&home, vault(&home));
        let studio = computers
            .add(
                paired("Studio\u{7}"),
                Some("mac".into()),
                Some("mac-key".into()),
            )
            .unwrap();
        assert_eq!(studio.name, "Studio");
        let file = std::fs::read_to_string(home.join("computers.json")).unwrap();
        assert!(!file.contains("sixty-four"), "no token in the file: {file}");
        let listed = computers.list();
        assert!(!listed.to_string().contains("sixty-four"), "{listed}");
        assert_eq!(listed["current"], "this");
        assert_eq!(listed["computers"][0]["state"], "idle");
        assert_eq!(listed["computers"][0]["os"], "mac");
        assert!(
            computers
                .token(&studio.id)
                .unwrap()
                .unwrap()
                .contains("sixty-four")
        );
        // Kept across a restart, current too; renamed on this Mac.
        assert!(computers.set_current(Some(&studio.id)).unwrap());
        assert!(!computers.set_current(Some("nope")).unwrap());
        assert!(computers.rename(&studio.id, "  Studio Mac ").unwrap());
        let again = Computers::open(&home, vault(&home));
        assert_eq!(again.current().as_deref(), Some(studio.id.as_str()));
        assert_eq!(again.get(&studio.id).unwrap().name, "Studio Mac");
        // Reached, then lost, as its supervisor says.
        assert!(matches!(
            again.observe(&studio.id, "direct-1", Ok(())),
            Some(Change::Moved { .. })
        ));
        assert_eq!(again.list()["computers"][0]["state"], "connected");
        assert_eq!(
            again.list()["computers"][0]["route"]["address"],
            "192.168.1.20:8443"
        );
        assert!(matches!(
            again.observe(&studio.id, "direct-1", Err("no answer".into())),
            Some(Change::Lost { .. })
        ));
        let lost = again.list();
        assert_eq!(lost["computers"][0]["state"], "lost", "{lost}");
        assert_eq!(lost["computers"][0]["tried"][0]["why"], "no answer");
        // Forgotten: its record, its token, and the window back on this Mac.
        assert!(again.forget(&studio.id).unwrap());
        assert!(!again.forget(&studio.id).unwrap());
        assert_eq!(again.current(), None);
        assert_eq!(again.token(&studio.id).unwrap(), None);
        let _ = std::fs::remove_dir_all(&home);
    }
}
