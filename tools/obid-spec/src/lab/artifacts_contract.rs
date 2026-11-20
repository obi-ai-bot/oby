//! Oby Artifacts' contract (the plan https://claude.ai/artifact/VrzfijvgHL6VD8jm5Hbs2k, §3.1, §3.5
//! and §3.8): what the app and the agents' tools talk to about artifacts, shaped as obid's future
//! API so the port renames nothing. It extends `docs/api/artifacts.md`, add-only: every key and
//! route obid has today keeps its name and meaning. Its TypeScript twin is
//! `tools/obid-spec/lab-ui/artifacts/src/contract.ts`; a change here changes the twin in the same
//! commit (`the_typescript_twin_matches`, below). Owned by the session building Oby Artifacts in
//! the lab.
//!
//! Who calls what:
//!
//! - **The person's app**: every route, with obid's owner token (`Authorization: Bearer <token>`).
//!   The owner's own acts ([`SIGNED_ROUTES`]: rename, delete, import, the words of a turn) also
//!   carry the device's signed words ([`WORDS_HEADER`]) over the exact body, as obid's other
//!   owner's words.
//! - **An agent** publishes through its `oby_artifact` tool, never a route of these (§3.4).
//!
//! Errors are obid's: `{"error": "One line a person can read.", "reason": "<code>"}` with a 4xx
//! ([`ApiError`]); the codes are [`REASONS`]. A refused publish says why in one line starting
//! `Not published: `, as obid's does today.
//!
//! Enums the clients decode ([`Kind`], [`Origin`]'s `kind`) only grow: a client treats a kind it
//! doesn't know as a file to download, never as an error (the Swift apps already decode `kind` as
//! an open string).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ── Routes ──────────────────────────────────────────────────────────────────────────────────────

/// `GET`: every artifact, newest first (`?conversation=&job=` narrow it); `GET /{id}` one;
/// `PATCH /{id}` a [`TitleBody`], signed; `DELETE /{id}`, signed.
pub const PATH_ARTIFACTS: &str = "/v1/artifacts";
/// `POST`, signed: a Claude Design project zip (`application/zip`), a page (`text/html`) or a
/// document (`text/markdown`) as the body; answers the new [`Artifact`], `origin.kind` `import`.
pub const PATH_IMPORT: &str = "/v1/artifacts/import";
/// `GET /v1/artifacts/{id}/export/{version}?as=zip|html` ([`EXPORTS`]): the version as one file.
/// Under `export/`, not `{version}/export`, so a file of the version named `export` stays served.
pub const VERB_EXPORT: &str = "export";
/// `POST` a [`TurnBody`], signed: the owner's words. With `artifact`, they land in that
/// artifact's conversation, whatever conversation the app had open ([`TurnAnswer`]).
pub const PATH_PLANS: &str = "/v1/plans";
/// Inside a `design` version, `{version}/_blob/<id>` serves the upload Claude named `/_blob/<id>`
/// from where it is kept ([`Version::blobs`]); an id outside the map is `404`.
pub const BLOB_PREFIX: &str = "_blob/";

/// Every route, as `docs/api/routes.txt` will list them.
pub const ROUTES: &[&str] = &[
    "GET /v1/artifacts",
    "GET /v1/artifacts/{id}",
    "PATCH /v1/artifacts/{id}",
    "DELETE /v1/artifacts/{id}",
    "GET /v1/artifacts/{id}/{version}/{*path}",
    "GET /v1/artifacts/{id}/export/{version}",
    "POST /v1/artifacts/import",
    "POST /v1/plans",
];

/// The owner's acts: refused without the device's signed words.
pub const SIGNED_ROUTES: &[&str] = &[
    "PATCH /v1/artifacts/{id}",
    "DELETE /v1/artifacts/{id}",
    "POST /v1/artifacts/import",
    "POST /v1/plans",
];

/// The owner's words, signed by a registered device key over the request's exact body:
/// `<deviceId> <signedAt> <signature>`, the signature (P-256, DER, base64) over
/// `oby-words\n<METHOD>\n<path>\n<sha256 hex of the body>\n<signedAt>`, as obid's other routes.
pub const WORDS_HEADER: &str = "X-Oby-Words";
/// How old a signature may be, either way, before it is refused.
pub const WORDS_MAX_SKEW_MS: i64 = 120_000;

/// What every file served from an artifact carries (obid's `artifacts::POLICY`, unchanged).
pub const POLICY: &str = "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline' data: blob:; style-src 'self' 'unsafe-inline' data:; img-src 'self' data: blob:; connect-src 'none'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; sandbox allow-scripts";

// ── Limits: Claude's (the owner's decision of 2026-11-20) ───────────────────────────────────────

/// A megabyte here is 1,048,576 bytes.
pub const MB: u64 = 1024 * 1024;
/// One text file (`.html`, `.css`, `.js`, `.json`, `.md`…), the page among them.
pub const MAX_TEXT_BYTES: u64 = 16 * MB;
/// One binary file (an image, a font, a video, a PDF).
pub const MAX_BINARY_BYTES: u64 = 15 * MB;
/// One SVG.
pub const MAX_SVG_BYTES: u64 = 2 * MB;
/// Files in one version.
pub const MAX_FILES: usize = 512;
/// Bytes in one version.
pub const MAX_VERSION_BYTES: u64 = 256 * MB;
/// Versions of one artifact (obid's; Claude publishes none).
pub const MAX_VERSIONS: u32 = 50;
/// What one job's artifacts, or one conversation's own, hold in all (16 of the largest version).
pub const MAX_OWNER_BYTES: u64 = 4096 * MB;
/// A `design`'s `canvas.json` `notes`, as JSON.
pub const MAX_NOTES_BYTES: u64 = MB;

// ── Artifacts ───────────────────────────────────────────────────────────────────────────────────

/// What an artifact is, from its shape. `design` is new: a folder whose `canvas.json` is Claude
/// Design's object (`v` 3, `boards`); `canvas` stays obid's list of frames, and also reads Swift's
/// `{"frames": […]}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Html,
    Markdown,
    Image,
    Pdf,
    Canvas,
    Design,
}

pub const KINDS: &[&str] = &["html", "markdown", "image", "pdf", "canvas", "design"];

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Html => "html",
            Kind::Markdown => "markdown",
            Kind::Image => "image",
            Kind::Pdf => "pdf",
            Kind::Canvas => "canvas",
            Kind::Design => "design",
        }
    }
    pub fn parse(text: &str) -> Option<Kind> {
        serde_json::from_value(serde_json::json!(text)).ok()
    }
}

/// Where an artifact came from (add-only key `origin`; missing reads as `own`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Origin {
    /// Made by one of the owner's own agents.
    #[default]
    Own,
    /// Shared from another Oby (the sharing PR fills it).
    #[serde(rename_all = "camelCase")]
    Sphere {
        sphere: String,
        from: String,
        offer: String,
    },
    /// Brought in by the owner: `from` is [`IMPORT_FROM`]'s.
    #[serde(rename_all = "camelCase")]
    Import { from: String },
}

pub const ORIGINS: &[&str] = &["own", "sphere", "import"];
/// What `origin.from` says for an import.
pub const IMPORT_FROM: &[&str] = &["claude-design-zip", "zip", "html", "markdown"];

/// One artifact, as `artifact.json` keeps it and the API shows it: obid's keys of today, then the
/// new ones (`profile`, `origin`, `shares`, `public`), each with a default so an old file reads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    /// The conversation that made it (TASKCONV), or the one its first words gave it.
    pub conversation: Option<String>,
    pub job: Option<String>,
    pub created: String,
    /// Oldest first.
    pub versions: Vec<Version>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub origin: Origin,
    /// Who it is shared with (the sharing PR fills it).
    #[serde(default)]
    pub shares: Vec<ShareSummary>,
    /// Its public link, never the key (the sharing PR fills it).
    #[serde(default)]
    pub public: Option<PublicSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub version: u32,
    pub created: String,
    /// Its files, paths relative to the version's folder, sorted.
    pub files: Vec<String>,
    pub bytes: u64,
    /// A `design`'s uploads: Claude's `/_blob/<id>` → where the version keeps it
    /// (`assets/<name>.<ext>`). Empty for every other kind.
    #[serde(default)]
    pub blobs: BTreeMap<String, String>,
    /// The job that made this version, when one did: a follow-up job in the artifact's
    /// conversation makes a new version of an artifact another job made.
    #[serde(default)]
    pub job: Option<String>,
    /// Who made this version when it isn't the owner's own agents: another Oby that was given
    /// edit (`"Ben's Oby"`), or, on a recipient's copy, the owner whose version it is. Such a
    /// version is data: reading it marks the turn untrusted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSummary {
    pub to: String,
    pub access: String,
    pub version: u32,
    pub latest: bool,
    pub state: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicSummary {
    pub link_id: String,
    pub version: u32,
    pub latest: bool,
    pub at: String,
}

/// `PATCH /v1/artifacts/{id}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitleBody {
    pub title: String,
}

/// What obid's events stream carries as `artifact` (`EVENT_TYPES`): a version published (`version`),
/// or a rename (`renamed: true`, the new `title`, no `version`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactEvent {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    pub title: String,
    pub kind: Kind,
    pub conversation: Option<String>,
    pub job: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed: Option<bool>,
}

pub const EVENT_TYPES: &[&str] = &["artifact"];

/// What an export can be. PDF and PNG come with the renderer (the `artifactview` PR).
pub const EXPORTS: &[&str] = &["zip", "html"];

// ── The turn's artifact (§3.5) ─────────────────────────────────────────────────────────────────

/// The words of a turn (`POST /v1/plans`, whose `Request` is `deny_unknown_fields`: `artifact` is
/// an add-only key of it, and of the app's signed `wordsOf` shape).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnBody {
    pub text: String,
    /// The conversation the app had open.
    #[serde(default)]
    pub conversation: Option<String>,
    /// The artifact open while the words were said.
    #[serde(default)]
    pub artifact: Option<TurnArtifact>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnArtifact {
    pub id: String,
    #[serde(default)]
    pub version: Option<u32>,
}

/// Where the words landed: the artifact's conversation, made for it when it had none (`created`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnAnswer {
    pub conversation: String,
    pub created: bool,
    pub artifact: Option<TurnArtifact>,
}

// ── Sharing (§3.6) and public links (§3.7) ─────────────────────────────────────────────────────

/// `GET` the [`ShareRecord`]s; `POST` a [`ShareBody`], signed; `DELETE …/shares/{to}`, signed
/// ([`EVERYONE`] revokes them all).
pub const VERB_SHARES: &str = "shares";
/// `POST` an [`AnswerBody`], signed: on a recipient's copy, its owner's words back (or a decline).
pub const VERB_ANSWERS: &str = "answers";
/// `POST` a [`SendBackBody`], signed: on a copy shared with `edit`, a version of it sent back.
pub const VERB_SEND_BACK: &str = "sendBack";
/// `POST` a [`PublicBody`], signed: a public link ([`PublicAnswer`]); `DELETE`, signed: revoked.
pub const VERB_PUBLIC: &str = "public";

pub const SHARING_ROUTES: &[&str] = &[
    "GET /v1/artifacts/{id}/shares",
    "POST /v1/artifacts/{id}/shares",
    "DELETE /v1/artifacts/{id}/shares/{to}",
    "POST /v1/artifacts/{id}/answers",
    "POST /v1/artifacts/{id}/sendBack",
    "POST /v1/artifacts/{id}/public",
    "DELETE /v1/artifacts/{id}/public",
];

/// Every sharing route but reading the shares is the owner's own act, signed.
pub const SHARING_SIGNED_ROUTES: &[&str] = &[
    "POST /v1/artifacts/{id}/shares",
    "DELETE /v1/artifacts/{id}/shares/{to}",
    "POST /v1/artifacts/{id}/answers",
    "POST /v1/artifacts/{id}/sendBack",
    "POST /v1/artifacts/{id}/public",
    "DELETE /v1/artifacts/{id}/public",
];

/// A share to every member of the Obysphere.
pub const EVERYONE: &str = "everyone";
/// What a public link's request must say, signed: the owner saw the Share menu's warning.
pub const ACKNOWLEDGED: &str = "anyone-with-the-link";

/// What a member may do with a shared artifact: look; also answer; also send a version back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    View,
    Comment,
    Edit,
}

pub const ACCESS: &[&str] = &["view", "comment", "edit"];

/// Where a share stands: sent, its member's answer back, declined, past its blob's expiry, or
/// taken back.
pub const SHARE_STATES: &[&str] = &["sent", "answered", "declined", "expired", "revoked"];

/// Who a share goes to: some members by name, or [`EVERYONE`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShareTo {
    Members(Vec<String>),
    All(String),
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShareBody {
    pub to: ShareTo,
    pub access: Access,
    /// What the owner asks of them, quoted to them (at most 2,000 characters).
    #[serde(default)]
    pub ask: Option<String>,
    /// They always see the newest version (the owner's rule), unless `false`.
    #[serde(default = "yes")]
    pub latest: bool,
    /// The owner's yes to sending files that instruct agents (`CLAUDE.md`, `AGENTS.md`, `.claude/`…),
    /// which the recipient quarantines anyway.
    #[serde(default)]
    pub instructions: bool,
}

/// One share, as `home/artifacts/<id>/shares.json` keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareRecord {
    pub to: String,
    pub sphere: String,
    pub offer: String,
    pub access: Access,
    pub version: u32,
    pub latest: bool,
    pub at: String,
    pub state: String,
    /// The blob the offer names (its newest one).
    pub blob: String,
    /// When that blob stops being fetched (Unix ms).
    pub expires: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnswerBody {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub decline: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendBackBody {
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicBody {
    /// [`ACKNOWLEDGED`], or the link isn't made.
    #[serde(default)]
    pub acknowledged: Option<String>,
    #[serde(default = "yes")]
    pub latest: bool,
}

/// The whole link, its key in the part after `#`: shown once, then kept only in obid's vault.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicAnswer {
    pub link: String,
    pub link_id: String,
    pub version: u32,
    pub latest: bool,
}

// ── Errors ──────────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub reason: String,
}

pub const REASONS: &[&str] = &[
    // The owner's token and words.
    "no_token",
    "bad_token",
    "unsigned",
    "bad_signature",
    "unknown_device",
    // Routes.
    "not_found",
    "bad_request",
    "bad_title",
    // A publish or an import refused (`Not published: …`).
    "not_published",
    "too_big",
    "too_many",
    "bad_design",
    "bad_canvas",
    "made_elsewhere",
    "full",
    "bad_zip",
    "no_export",
    // Sharing and public links.
    "feature_not_taken",
    "policy_refused",
    "not_acknowledged",
    "not_allowed",
    "not_shared",
    "instructions",
    "secret_found",
    "scan_failed",
    "bad_blob",
    "expired",
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const TWIN: &str = include_str!("../../lab-ui/artifacts/src/contract.ts");

    /// The strings of `export const <name> = [ … ] as const;` in the twin, in order.
    fn twin_list(name: &str) -> Vec<String> {
        let start = TWIN
            .find(&format!("export const {name} = ["))
            .unwrap_or_else(|| panic!("contract.ts has no {name}"));
        let rest = &TWIN[start..];
        let end = rest.find("] as const").expect("the list's end");
        rest[..end]
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect()
    }

    fn twin_const(name: &str) -> String {
        let start = TWIN
            .find(&format!("export const {name} = \""))
            .unwrap_or_else(|| panic!("contract.ts has no {name}"));
        TWIN[start..].split('"').nth(1).unwrap().to_string()
    }

    fn twin_number(name: &str) -> u64 {
        let start = TWIN
            .find(&format!("export const {name} = "))
            .unwrap_or_else(|| panic!("contract.ts has no {name}"));
        let rest = &TWIN[start + format!("export const {name} = ").len()..];
        let text: String = rest
            .split(';')
            .next()
            .unwrap()
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '_')
            .collect();
        // `16 * MB` and the like: a product of numbers and MB.
        text.split('*')
            .map(|p| if p == "MB" { MB } else { p.parse().unwrap() })
            .product()
    }

    #[test]
    fn the_typescript_twin_matches() {
        assert_eq!(twin_list("ROUTES"), ROUTES);
        assert_eq!(twin_list("SIGNED_ROUTES"), SIGNED_ROUTES);
        assert_eq!(twin_list("KINDS"), KINDS);
        assert_eq!(twin_list("ORIGINS"), ORIGINS);
        assert_eq!(twin_list("IMPORT_FROM"), IMPORT_FROM);
        assert_eq!(twin_list("EVENT_TYPES"), EVENT_TYPES);
        assert_eq!(twin_list("EXPORTS"), EXPORTS);
        assert_eq!(twin_list("REASONS"), REASONS);
        assert_eq!(twin_list("SHARING_ROUTES"), SHARING_ROUTES);
        assert_eq!(twin_list("SHARING_SIGNED_ROUTES"), SHARING_SIGNED_ROUTES);
        assert_eq!(twin_list("ACCESS"), ACCESS);
        assert_eq!(twin_list("SHARE_STATES"), SHARE_STATES);
        for (name, value) in [
            ("PATH_ARTIFACTS", PATH_ARTIFACTS),
            ("PATH_IMPORT", PATH_IMPORT),
            ("VERB_EXPORT", VERB_EXPORT),
            ("PATH_PLANS", PATH_PLANS),
            ("BLOB_PREFIX", BLOB_PREFIX),
            ("WORDS_HEADER", WORDS_HEADER),
            ("VERB_SHARES", VERB_SHARES),
            ("VERB_ANSWERS", VERB_ANSWERS),
            ("VERB_SEND_BACK", VERB_SEND_BACK),
            ("VERB_PUBLIC", VERB_PUBLIC),
            ("EVERYONE", EVERYONE),
            ("ACKNOWLEDGED", ACKNOWLEDGED),
        ] {
            assert_eq!(twin_const(name), value, "{name}");
        }
        for (name, value) in [
            ("MB", MB),
            ("MAX_TEXT_BYTES", MAX_TEXT_BYTES),
            ("MAX_BINARY_BYTES", MAX_BINARY_BYTES),
            ("MAX_SVG_BYTES", MAX_SVG_BYTES),
            ("MAX_FILES", MAX_FILES as u64),
            ("MAX_VERSION_BYTES", MAX_VERSION_BYTES),
            ("MAX_VERSIONS", MAX_VERSIONS as u64),
            ("MAX_OWNER_BYTES", MAX_OWNER_BYTES),
            ("MAX_NOTES_BYTES", MAX_NOTES_BYTES),
        ] {
            assert_eq!(twin_number(name), value, "{name}");
        }
        for k in KINDS {
            assert_eq!(Kind::parse(k).unwrap().as_str(), *k);
        }
        // Every field the wire carries is named in the twin.
        let wire = [
            serde_json::to_value(sample()).unwrap(),
            serde_json::to_value(ArtifactEvent {
                id: "a".into(),
                version: Some(1),
                title: "t".into(),
                kind: Kind::Design,
                conversation: None,
                job: None,
                renamed: Some(true),
            })
            .unwrap(),
            serde_json::to_value(TurnBody {
                text: "t".into(),
                conversation: Some("c".into()),
                artifact: Some(TurnArtifact {
                    id: "a".into(),
                    version: Some(1),
                }),
            })
            .unwrap(),
            serde_json::to_value(TurnAnswer {
                conversation: "c".into(),
                created: true,
                artifact: None,
            })
            .unwrap(),
            serde_json::to_value(ApiError {
                error: "e".into(),
                reason: "r".into(),
            })
            .unwrap(),
            serde_json::to_value(ShareBody {
                to: ShareTo::Members(vec!["ben".into()]),
                access: Access::Edit,
                ask: Some("a".into()),
                latest: true,
                instructions: false,
            })
            .unwrap(),
            serde_json::to_value(ShareRecord {
                to: "ben".into(),
                sphere: "s".into(),
                offer: "o".into(),
                access: Access::View,
                version: 1,
                latest: true,
                at: "t".into(),
                state: "sent".into(),
                blob: "b".into(),
                expires: 1,
            })
            .unwrap(),
            serde_json::to_value(AnswerBody {
                text: Some("t".into()),
                decline: false,
            })
            .unwrap(),
            serde_json::to_value(SendBackBody { version: 2 }).unwrap(),
            serde_json::to_value(PublicBody {
                acknowledged: Some(ACKNOWLEDGED.into()),
                latest: true,
            })
            .unwrap(),
            serde_json::to_value(PublicAnswer {
                link: "l".into(),
                link_id: "i".into(),
                version: 1,
                latest: true,
            })
            .unwrap(),
        ];
        fn keys(v: &Value, out: &mut Vec<String>) {
            if let Value::Object(m) = v {
                for (k, v) in m {
                    out.push(k.clone());
                    keys(v, out);
                }
            } else if let Value::Array(a) = v {
                for v in a {
                    keys(v, out);
                }
            }
        }
        let mut all = vec![];
        for v in &wire {
            keys(v, &mut all);
        }
        for k in all {
            if k.starts_with("blob-") {
                continue; // a map's key, not a field
            }
            assert!(
                TWIN.contains(&format!("{k}:")) || TWIN.contains(&format!("{k}?:")),
                "contract.ts lacks the field {k}"
            );
        }
    }

    fn sample() -> Artifact {
        Artifact {
            id: "a".into(),
            title: "t".into(),
            kind: Kind::Design,
            conversation: Some("c".into()),
            job: Some("j".into()),
            created: "2026-11-20T00:00:00Z".into(),
            versions: vec![Version {
                version: 1,
                created: "2026-11-20T00:00:00Z".into(),
                files: vec!["canvas.json".into()],
                bytes: 1,
                blobs: BTreeMap::from([("blob-1".into(), "assets/blob-1.png".into())]),
                job: Some("j".into()),
                by: Some("Ben's Oby".into()),
            }],
            profile: Some("p".into()),
            origin: Origin::Sphere {
                sphere: "s".into(),
                from: "f".into(),
                offer: "o".into(),
            },
            shares: vec![ShareSummary {
                to: "t".into(),
                access: "view".into(),
                version: 1,
                latest: true,
                state: "sent".into(),
            }],
            public: Some(PublicSummary {
                link_id: "l".into(),
                version: 1,
                latest: true,
                at: "2026-11-20T00:00:00Z".into(),
            }),
        }
    }

    #[test]
    fn an_artifact_json_of_today_still_reads() {
        // obid's artifact.json as docs/api/artifacts.md shows it: no new key.
        let old = json!({"id": "3f0c", "title": "Landing page", "kind": "html", "conversation": null,
            "job": "79f64f22", "created": "2026-10-17T09:12:00Z",
            "versions": [{"version": 1, "created": "2026-10-17T09:12:00Z",
                          "files": ["index.html", "style.css"], "bytes": 1832}]});
        let a: Artifact = serde_json::from_value(old).unwrap();
        assert_eq!(a.origin, Origin::Own);
        assert!(a.versions[0].blobs.is_empty() && a.shares.is_empty() && a.public.is_none());
    }

    #[test]
    fn the_words_header_is_the_browsers() {
        assert_eq!(WORDS_HEADER, super::super::browser_contract::WORDS_HEADER);
        assert_eq!(
            WORDS_MAX_SKEW_MS,
            super::super::browser_contract::WORDS_MAX_SKEW_MS
        );
    }
}
