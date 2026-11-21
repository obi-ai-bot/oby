//! The owner's edit of an artifact: a new version, made from one they saw (docs/contracts/
//! artifacts.md, ARTIFACTS-16 to 18; the app's Code tab saves through it, the port plan's L3).
//! It owns no state of its own: the new version goes through the store's one way of keeping a
//! version ([`Artifacts::finish`]), with every check a publish has, and says `by: "owner"`.
//! It never writes another artifact, never changes a version that's kept, and never takes a
//! file an agent's room or worktree holds: only the version it names and the text it was sent.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use super::{Artifacts, Kind, Made, Publish, copy, limits};
use crate::api::{ApiError, Shared};

/// Why an edit made nothing.
#[derive(Debug, PartialEq)]
pub enum Refusal {
    /// There's no such artifact, or no such version of it.
    NotThere,
    /// `base` isn't the latest version any more: the latest is this one.
    NotLatest(u32),
    /// One line saying why, as a publish says it.
    Refused(String),
}

impl Artifacts {
    /// A new version of `id`: version `base`'s files, with each of `changes` (a path of that
    /// version, and its new text) laid over them. `base` must be the latest version, so two edits
    /// never pass each other unseen. The same files as `base` make nothing new: `base` is the
    /// answer. The new version is the next one, `by: "owner"`.
    pub async fn edit(
        &self,
        id: &str,
        base: u32,
        changes: &BTreeMap<String, String>,
    ) -> Result<(String, u32), Refusal> {
        let artifact = self.get(id).ok_or(Refusal::NotThere)?;
        let kept = artifact
            .versions
            .iter()
            .find(|v| v.version == base)
            .ok_or(Refusal::NotThere)?
            .clone();
        if base != artifact.latest() {
            return Err(Refusal::NotLatest(artifact.latest()));
        }
        if changes.is_empty() {
            return Err(Refusal::Refused(
                "Not saved: the edit changes no file.".into(),
            ));
        }
        for path in changes.keys() {
            if !kept.files.contains(path) {
                return Err(Refusal::Refused(format!(
                    "Not saved: {path} isn't a file of version {base}."
                )));
            }
        }
        let staging = self
            .dir
            .join(format!(".staging-{}", uuid::Uuid::new_v4().simple()));
        let made = self
            .edit_in(&artifact.id, artifact.kind, &kept, changes, &staging)
            .await;
        let _ = std::fs::remove_dir_all(&staging);
        made.map_err(|why| Refusal::Refused(why.replacen("Not published:", "Not saved:", 1)))
    }

    async fn edit_in(
        &self,
        id: &str,
        kind: Kind,
        base: &super::Version,
        changes: &BTreeMap<String, String>,
        staging: &std::path::Path,
    ) -> Result<(String, u32), String> {
        // the version's own files, read as a publish reads any folder (no link, nothing hidden)
        let (folder, version, to) = (
            self.dir.join(id),
            base.version.to_string(),
            staging.to_path_buf(),
        );
        let copied = tokio::task::spawn_blocking(move || copy(&folder, &version, &to))
            .await
            .map_err(|e| format!("Not published: the copy stopped ({e})."))?;
        let copied = match copied {
            Ok(mut copied) => {
                for (path, text) in changes {
                    if let Some(why) = limits::file_too_big(path, text.len() as u64) {
                        return Err(format!("Not published: {why}"));
                    }
                    let file = staging.join(path);
                    let before = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
                    crate::store::write_private(&file, text.as_bytes())
                        .map_err(|e| format!("Not published: obid couldn't write {path} ({e})."))?;
                    copied.bytes = copied.bytes - before + text.len() as u64;
                }
                if copied.bytes > limits::MAX_BYTES {
                    return Err(format!("Not published: {}", limits::version_too_big()));
                }
                // a document, a picture or a PDF is one file, as it was published
                copied.folder = !matches!(kind, Kind::Markdown | Kind::Image | Kind::Pdf);
                Ok(copied)
            }
            Err(why) => Err(why),
        };
        let artifact = self
            .get(id)
            .ok_or("Not published: the artifact was deleted meanwhile.")?;
        let what = Publish {
            root: staging,
            path: &artifact.title,
            title: &artifact.title,
            kind: Some(kind),
            conversation: artifact.conversation.as_deref(),
            job: None,
            artifact: Some(id),
            kept: None,
        };
        let made = Made {
            by: Some("owner"),
            blobs: (!base.blobs.is_empty()).then(|| base.blobs.clone()),
        };
        self.finish(
            &what,
            &artifact.title,
            Some(artifact.clone()),
            staging,
            copied,
            made,
        )
        .await
    }
}

/// The bound on an edit's body: a megabyte past the largest text file (16 MB), so a file just over
/// it is read whole and answered in obid's words. Several files at once fit in it together.
pub fn body_limit() -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(17 << 20)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    base: u32,
    files: BTreeMap<String, String>,
}

/// `POST /v1/artifacts/{id}/versions` with `{"base": n, "files": {"<path>": "<text>"}}`, signed
/// by one of the owner's devices (`X-Oby-Words`): the owner's edit, a new version.
pub async fn edit(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if crate::routes::signed_by(&shared, "POST", &uri, &headers, &body).is_none() {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "An edit is the owner's own, signed by one of their devices.",
        ));
    }
    let Edit { base, files } = serde_json::from_slice(&body).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "The body is {\"base\": <version>, \"files\": {\"<path>\": \"<text>\"}}.",
        )
    })?;
    match shared.artifacts.edit(&id, base, &files).await {
        Ok((id, version)) => Ok((
            StatusCode::CREATED,
            axum::Json(json!({"id": id, "version": version})),
        )
            .into_response()),
        Err(Refusal::NotThere) => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "There's no such artifact, or no such version of it.",
        )),
        Err(Refusal::NotLatest(latest)) => Err(ApiError::new(
            StatusCode::CONFLICT,
            format!(
                "Version {latest} came after the one this edit was made on: open it, then edit."
            ),
        )),
        Err(Refusal::Refused(why)) => Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            shared.artifacts.speakable(why).await,
        )),
    }
}
