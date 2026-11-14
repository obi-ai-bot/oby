//! Receipts for requests that create work (docs/contracts/receipts.md): for a day, and across
//! restarts, which request key got which answer, so the same request sent again gets the first
//! answer, not a second job. One file per key, `home/receipts/<sha256 of the key>.json` (0600),
//! holding the body's hash and the answer with the secrets obid knows taken out: never the body.
//! `POST /v1/jobs` (R02-04), a job's messages, stop and move and the inbox answer (R02-05) use
//! them, as do `oby_dispatch`, `oby_tell_job` and obid's own Try again ([`once`], R02-06).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How long a receipt holds, in seconds: one this old or older is never returned.
pub const KEPT: i64 = 86_400;

/// The longest key, in characters.
pub const LONGEST: usize = 128;

/// What a request key got.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub key: String,
    /// The route it was sent to, like `POST /v1/jobs`.
    pub route: String,
    /// The hex sha256 of the body's canonical JSON ([`body_hash`]).
    pub body_hash: String,
    /// The HTTP status it answered.
    pub status: u16,
    /// The answer's JSON text, redacted.
    pub answer: String,
    /// The job it started, if any.
    pub job: Option<String>,
    /// Seconds since the epoch.
    pub created: i64,
}

/// What [`Receipts::get`] found for a key.
#[derive(Debug, PartialEq)]
pub enum Found {
    /// No receipt under a day old: the request is new.
    None,
    /// The same route and body: the first answer.
    Repeat {
        status: u16,
        answer: String,
        job: Option<String>,
    },
    /// The key was used for another request.
    Conflict,
}

/// Held while a request with its key is answered: another with the same key waits for it.
pub type Guard = tokio::sync::OwnedMutexGuard<()>;

pub struct Receipts {
    dir: PathBuf,
    /// One lock per key, by its file's name; a lock nobody holds goes at the next sweep.
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

/// The hex sha256 of `body`'s canonical JSON: serde_json's objects keep their keys sorted.
pub fn body_hash(body: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(body).map_err(|e| e.to_string())?;
    Ok(hex::encode(obi_protocol::keys::sha256(bytes)))
}

/// `run` once for `key` (RECEIPTS-12 to 14): held from the lookup to the receipt, a receipt for the
/// same `route` and `body` gives `repeat` its answer and job, and runs nothing. Else `run`'s answer,
/// kept with its job when it gives one. With no store, a conflict or a store that fails, it runs.
pub async fn once<T>(
    store: Option<&Receipts>,
    key: &str,
    route: &str,
    body: &Value,
    repeat: impl FnOnce(String, Option<String>) -> T,
    run: impl Future<Output = (T, Option<(Value, Option<String>)>)>,
) -> T {
    let Some(store) = store else {
        return run.await.0;
    };
    let _guard = store.lock(key).await;
    let now = obi_protocol::time::now();
    if let Ok(Found::Repeat { answer, job, .. }) = store.get(key, route, body, now) {
        return repeat(answer, job);
    }
    let (answer, kept) = run.await;
    if let Some((kept, job)) = kept
        && let Err(e) = store.put(key, route, body, 200, &kept, job.as_deref(), now)
    {
        tracing::warn!("{route}: its receipt wasn't kept: {e}");
    }
    answer
}

/// The file's name for `key`, its sha256: a key never becomes a path.
fn name(key: &str) -> String {
    hex::encode(obi_protocol::keys::sha256(key.as_bytes()))
}

impl Receipts {
    /// The store in `home/receipts`, made if it isn't there (0700).
    pub fn open(home: &Path) -> std::io::Result<Receipts> {
        let dir = home.join("receipts");
        crate::store::make_private_dir(&dir)?;
        Ok(Receipts {
            dir,
            locks: Mutex::default(),
        })
    }

    /// The file for `key`. A key is 1 to [`LONGEST`] characters; any other is refused, and
    /// nothing is read or written for it.
    fn path(&self, key: &str) -> Result<PathBuf, String> {
        let length = key.chars().count();
        if length == 0 || length > LONGEST {
            return Err(format!("A request key is 1 to {LONGEST} characters."));
        }
        Ok(self.dir.join(format!("{}.json", name(key))))
    }

    /// Waits until no one else answers a request with `key`; the guard holds it until dropped.
    pub async fn lock(&self, key: &str) -> Guard {
        let lock = {
            let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
            locks.entry(name(key)).or_default().clone()
        };
        lock.lock_owned().await
    }

    /// Keeps what `key` got: `answer` as JSON text with the secrets obid knows taken out.
    #[allow(clippy::too_many_arguments)]
    pub fn put(
        &self,
        key: &str,
        route: &str,
        body: &Value,
        status: u16,
        answer: &Value,
        job: Option<&str>,
        now: i64,
    ) -> Result<(), String> {
        let path = self.path(key)?;
        let receipt = Receipt {
            key: key.to_string(),
            route: route.to_string(),
            body_hash: body_hash(body)?,
            status,
            answer: crate::secrets::known().redact(&answer.to_string()),
            job: job.map(str::to_string),
            created: now,
        };
        let bytes = serde_json::to_vec(&receipt).map_err(|e| e.to_string())?;
        crate::store::write_private(&path, &bytes).map_err(|e| e.to_string())
    }

    /// What `key` got, judged against this `route` and `body`. A file that doesn't read is
    /// logged and counts as none.
    pub fn get(&self, key: &str, route: &str, body: &Value, now: i64) -> Result<Found, String> {
        let path = self.path(key)?;
        let receipt = match std::fs::read(&path).map(|b| serde_json::from_slice::<Receipt>(&b)) {
            Ok(Ok(receipt)) => receipt,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Found::None),
            Ok(Err(e)) => {
                tracing::warn!("{} doesn't read: {e}", path.display());
                return Ok(Found::None);
            }
            Err(e) => {
                tracing::warn!("{} doesn't read: {e}", path.display());
                return Ok(Found::None);
            }
        };
        if now - receipt.created >= KEPT {
            return Ok(Found::None);
        }
        if receipt.route != route || receipt.body_hash != body_hash(body)? {
            return Ok(Found::Conflict);
        }
        Ok(Found::Repeat {
            status: receipt.status,
            answer: receipt.answer,
            job: receipt.job,
        })
    }

    /// Removes every receipt a day old or older, and the locks nobody holds; says how many
    /// receipts went. A file it can't read or remove is logged and left.
    pub fn sweep(&self, now: i64) -> std::io::Result<usize> {
        self.locks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, lock| Arc::strong_count(lock) > 1);
        let mut removed = 0;
        for entry in std::fs::read_dir(&self.dir)?.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            match std::fs::read(&path).map(|b| serde_json::from_slice::<Receipt>(&b)) {
                Ok(Ok(receipt)) if now - receipt.created < KEPT => {}
                Ok(Ok(_)) => match std::fs::remove_file(&path) {
                    Ok(()) => removed += 1,
                    Err(e) => tracing::warn!("can't remove {}: {e}", path.display()),
                },
                Ok(Err(e)) => tracing::warn!("{} doesn't read: {e}", path.display()),
                Err(e) => tracing::warn!("{} doesn't read: {e}", path.display()),
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn home(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("obid-receipts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    const NOW: i64 = 1_000_000;

    #[tokio::test]
    async fn receipts_1_a_repeat_gets_the_first_answer() {
        let receipts = Receipts::open(&home("repeat")).unwrap();
        let first = receipts.lock("K").await;
        // Another request with K waits; one with another key doesn't.
        let second = receipts.lock("K");
        tokio::pin!(second);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), &mut second)
                .await
                .is_err()
        );
        drop(receipts.lock("L").await);
        let body = json!({"repo": "o/r", "task": {"a": 1, "b": 2}});
        let answer = json!({"id": "ab12", "status": "queued"});
        receipts
            .put("K", "POST /v1/jobs", &body, 201, &answer, Some("ab12"), NOW)
            .unwrap();
        drop(first);
        let _second = second.await;
        let reordered: Value =
            serde_json::from_str(r#"{"task": {"b": 2, "a": 1}, "repo": "o/r"}"#).unwrap();
        assert_eq!(
            receipts
                .get("K", "POST /v1/jobs", &reordered, NOW + KEPT - 1)
                .unwrap(),
            Found::Repeat {
                status: 201,
                answer: answer.to_string(),
                job: Some("ab12".to_string())
            }
        );
        assert_eq!(
            receipts.get("M", "POST /v1/jobs", &body, NOW).unwrap(),
            Found::None
        );
        // A key out of bounds reads and writes nothing.
        assert!(receipts.get("", "POST /v1/jobs", &body, NOW).is_err());
        let long = "k".repeat(LONGEST + 1);
        assert!(
            receipts
                .put(&long, "POST /v1/jobs", &body, 201, &answer, None, NOW)
                .is_err()
        );
        assert!(
            receipts
                .get(&"k".repeat(LONGEST), "POST /v1/jobs", &body, NOW)
                .is_ok()
        );
    }

    #[test]
    fn receipts_2_another_body_is_a_conflict() {
        let dir = home("conflict");
        let receipts = Receipts::open(&dir).unwrap();
        let body = json!({"task": "Do it"});
        receipts
            .put("K", "POST /v1/jobs", &body, 201, &json!({}), None, NOW)
            .unwrap();
        let other = json!({"task": "Do it again"});
        assert_eq!(
            receipts.get("K", "POST /v1/jobs", &other, NOW).unwrap(),
            Found::Conflict
        );
        assert_eq!(
            receipts.get("K", "POST /v1/tasks", &body, NOW).unwrap(),
            Found::Conflict
        );
        // The file is named by the key's hash, holds the body's hash and never the body.
        let text =
            std::fs::read_to_string(dir.join("receipts").join(format!("{}.json", name("K"))));
        assert!(!text.unwrap().contains("Do it"));
    }

    #[test]
    fn receipts_3_a_restart_keeps_receipts() {
        let dir = home("restart");
        let body = json!({"task": "Do it"});
        let receipts = Receipts::open(&dir).unwrap();
        receipts
            .put(
                "K",
                "POST /v1/jobs",
                &body,
                201,
                &json!({"id": "ab12"}),
                None,
                NOW,
            )
            .unwrap();
        drop(receipts);
        let receipts = Receipts::open(&dir).unwrap();
        assert!(matches!(
            receipts.get("K", "POST /v1/jobs", &body, NOW + 60).unwrap(),
            Found::Repeat { status: 201, .. }
        ));
    }

    #[test]
    fn receipts_4_a_day_old_receipt_goes() {
        let dir = home("day-old");
        let receipts = Receipts::open(&dir).unwrap();
        let body = json!({"task": "Do it"});
        receipts
            .put("K", "POST /v1/jobs", &body, 201, &json!({}), None, NOW)
            .unwrap();
        receipts
            .put("L", "POST /v1/jobs", &body, 201, &json!({}), None, NOW + 1)
            .unwrap();
        let later = NOW + KEPT;
        assert_eq!(
            receipts.get("K", "POST /v1/jobs", &body, later).unwrap(),
            Found::None
        );
        assert_eq!(receipts.sweep(later).unwrap(), 1);
        let file = dir.join("receipts").join(format!("{}.json", name("K")));
        assert!(!file.exists());
        assert!(
            dir.join("receipts")
                .join(format!("{}.json", name("L")))
                .exists()
        );
        let new = json!({"task": "Something else"});
        receipts
            .put(
                "K",
                "POST /v1/jobs",
                &new,
                202,
                &json!({"id": "cd34"}),
                None,
                later,
            )
            .unwrap();
        assert!(matches!(
            receipts.get("K", "POST /v1/jobs", &new, later).unwrap(),
            Found::Repeat { status: 202, .. }
        ));
    }
}
