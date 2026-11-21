//! Sharing's format and the recipient's checks (docs/contracts/artifact-sharing.md, ARTSHARE-1 to
//! 3): the half of sharing that needs no Sphere, ported from the lab's `artifactsphere` core
//! (tools/obid-spec/src/lab/artifactsphere.rs, #2709) to the Obysphere's own spec (obi/0.1,
//! protocol.md 5.20 and section 11, "Receiving an artifact").
//!
//! What leaves is one format: a version packed as a tar that is the same bytes every time
//! ([`pack`]), encrypted once with age (age-encryption.org/v1, the `age` crate) under a fresh file
//! key wrapped to each recipient's card seal key, the X25519 key its card already publishes, as
//! age's X25519 recipient ([`seal`]), so a whole group is one upload, cut into parts of at most
//! [`BLOB_BYTES`] ([`sealed`]). What another Oby sent is checked before anything is kept
//! ([`receive`]), in this order: each part's size and hash and the whole one's, decrypting with
//! this Oby's own seal key, the tar's hash, the tar itself (no link, no path out: [`unpack`]),
//! no more files or bytes than offered, Claude's limits, the secret scan (a scan that can't run
//! refuses too); then the files that instruct agents are set apart ([`split_instructions`],
//! [`set_apart`]), and what an agent may later read of it is quoted ([`quote`]).
//!
//! It owns no state but `home/artifacts/quarantine/<id>/`, which only [`set_apart`] writes; it
//! never writes an artifact's own folder or `artifact.json`, and never reaches the network.

use std::collections::BTreeMap;
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::Digest as _;

use super::limits;
use crate::scan::Verdict;

/// A version's files: each path, relative to the version, and its bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

/// The most one blob holds: a version's ciphertext past it is cut into several, each named with
/// its hash in the offer (the port plan's "Open", option B, its default).
pub const BLOB_BYTES: usize = 64 * 1024 * 1024;
/// The most ciphertext obid opens: a version at Claude's limits, with room for the tar's headers
/// and age's own (a header per recipient, 16 bytes per 64 KiB).
pub const MAX_CIPHER_BYTES: u64 = limits::MAX_BYTES + 8 * 1024 * 1024;
/// The most parts one version is cut into: what a version at Claude's limits needs (the Obysphere
/// takes up to 8, sphere-api.md section 10).
pub const MAX_BLOBS: usize = MAX_CIPHER_BYTES.div_ceil(BLOB_BYTES as u64) as usize;
/// Where the files that instruct agents wait, under `home/artifacts`.
pub const QUARANTINE: &str = "quarantine";

/// Why what was received isn't kept: a stable `reason` and one line starting `Not kept: `.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub reason: &'static str,
    pub message: String,
}

fn not_kept(reason: &'static str, why: impl std::fmt::Display) -> Refused {
    Refused {
        reason,
        message: format!("Not kept: {why}"),
    }
}

/// The SHA-256 of `bytes`, in lower-case hex.
pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

/// A card's seal key (`seal_public_key`'s 32 raw bytes) as age's X25519 recipient, `age1…`.
pub fn recipient_of(seal_public: &[u8; 32]) -> String {
    use bech32::ToBase32;
    bech32::encode("age", seal_public.to_base32(), bech32::Variant::Bech32)
        .expect("32 bytes fit a recipient")
}

/// This Oby's own seal key (its private half's 32 bytes) as age's X25519 identity,
/// `AGE-SECRET-KEY-1…`: held only while it opens one file, never kept or said.
fn identity_of(seal_private: &[u8; 32]) -> Result<age::x25519::Identity, String> {
    use bech32::ToBase32;
    let text = bech32::encode(
        "age-secret-key-",
        seal_private.to_base32(),
        bech32::Variant::Bech32,
    )
    .map_err(|e| e.to_string())?
    .to_uppercase();
    text.parse()
        .map_err(|_| "that isn't an X25519 seal key".to_owned())
}

/// `plain` encrypted once, its file key wrapped to each of `recipients` (each a card's seal key):
/// any one of their seal keys opens it, and nobody else's.
pub fn seal(plain: &[u8], recipients: &[[u8; 32]]) -> Result<Vec<u8>, String> {
    let keys: Vec<age::x25519::Recipient> = recipients
        .iter()
        .map(|r| {
            recipient_of(r)
                .parse()
                .map_err(|e| format!("a seal key isn't an age recipient: {e}"))
        })
        .collect::<Result<_, String>>()?;
    let encryptor = age::Encryptor::with_recipients(keys.iter().map(|k| k as &dyn age::Recipient))
        .map_err(|_| "there's no recipient to seal it for".to_owned())?;
    let mut out = vec![];
    let mut w = encryptor.wrap_output(&mut out).map_err(|e| e.to_string())?;
    w.write_all(plain).map_err(|e| e.to_string())?;
    w.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// What this Oby's seal key (its private half) opens of `cipher`: never more than
/// [`MAX_CIPHER_BYTES`].
pub fn open(cipher: &[u8], seal_private: &[u8; 32]) -> Result<Vec<u8>, String> {
    let identity = identity_of(seal_private)?;
    let decryptor = age::Decryptor::new(cipher).map_err(|e| e.to_string())?;
    let r = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|e| e.to_string())?;
    let mut out = vec![];
    r.take(MAX_CIPHER_BYTES + 1)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    if out.len() as u64 > MAX_CIPHER_BYTES {
        return Err("it holds more than a version may".into());
    }
    Ok(out)
}

/// How many recipients' wrapped keys an age file's header holds.
pub fn stanzas(cipher: &[u8]) -> usize {
    let end = cipher
        .windows(4)
        .position(|w| w == b"\n---")
        .unwrap_or(cipher.len().min(4096));
    String::from_utf8_lossy(&cipher[..end])
        .lines()
        .filter(|l| l.starts_with("-> X25519 "))
        .count()
}

/// A version as one tar, the same bytes every time: names sorted, mode 0644, owner 0, time 0.
pub fn pack(files: &Files) -> Result<Vec<u8>, String> {
    let mut b = tar::Builder::new(vec![]);
    b.mode(tar::HeaderMode::Deterministic);
    for (path, bytes) in files {
        let mut h = tar::Header::new_ustar();
        h.set_size(bytes.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(0);
        h.set_uid(0);
        h.set_gid(0);
        h.set_entry_type(tar::EntryType::Regular);
        b.append_data(&mut h, path, bytes.as_slice())
            .map_err(|e| format!("{path}: {e}"))?;
    }
    b.into_inner().map_err(|e| e.to_string())
}

/// A blob's or a part's size and SHA-256, as an offer's `blob` and `blobs` name them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub sha256: String,
    pub bytes: u64,
}

impl Part {
    pub fn of(bytes: &[u8]) -> Part {
        Part {
            sha256: sha256(bytes),
            bytes: bytes.len() as u64,
        }
    }
}

/// What a sender uploads for a version, and what its offer says of it (protocol.md 5.20): the
/// ciphertext in one part, or in parts of at most [`BLOB_BYTES`] past it; the whole one's size and
/// hash (`blob`) and, when cut, each part's (`blobs`); the tar's hash, its files and their bytes
/// (`artifact.sha256`, `files`, `bytes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    /// What is uploaded, in order: one part, or 2 and more.
    pub uploads: Vec<Vec<u8>>,
    /// `blob`: the whole ciphertext.
    pub blob: Part,
    /// `blobs`: each part, when there's more than one; empty otherwise.
    pub blobs: Vec<Part>,
    /// The tar's SHA-256, before it was encrypted.
    pub sha256: String,
    pub files: usize,
    pub bytes: u64,
}

/// `files` packed, sealed once for every one of `recipients` (their cards' seal keys) and cut into
/// parts.
pub fn sealed(files: &Files, recipients: &[[u8; 32]]) -> Result<Sealed, String> {
    let tar = pack(files)?;
    let cipher = seal(&tar, recipients)?;
    let uploads: Vec<Vec<u8>> = cipher.chunks(BLOB_BYTES).map(<[u8]>::to_vec).collect();
    let blobs = match uploads.len() {
        1 => vec![],
        _ => uploads.iter().map(|p| Part::of(p)).collect(),
    };
    Ok(Sealed {
        blob: Part::of(&cipher),
        uploads,
        blobs,
        sha256: sha256(&tar),
        files: files.len(),
        bytes: files.values().map(|b| b.len() as u64).sum(),
    })
}

/// Whether `path` is a plain relative path: no `/` at the start or end, no empty, `.` or `..`
/// part, no `\` or NUL.
fn plain(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', '\0'])
        && !path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
}

/// What a recipient takes from a tar: plain files at plain paths. A link of either kind, a path
/// out (`..`, absolute), anything but a file or a folder, a path twice (letter case aside: macOS
/// ignores it) or a path that is both a file and a folder refuses the whole of it, as does more
/// than [`limits::MAX_ENTRIES`] entries. Each file is read to one byte past its limit at most.
pub fn unpack(tar_bytes: &[u8]) -> Result<Files, Refused> {
    let bad = |why: String| not_kept("bad_blob", why);
    let mut a = tar::Archive::new(tar_bytes);
    let mut out = Files::new();
    let mut lower = std::collections::BTreeSet::new();
    let entries = a
        .entries()
        .map_err(|e| bad(format!("it isn't a tar ({e}).")))?;
    for (n, e) in entries.enumerate() {
        if n >= limits::MAX_ENTRIES {
            return Err(bad(format!(
                "it holds more than {} entries.",
                limits::MAX_ENTRIES
            )));
        }
        let mut e = e.map_err(|e| bad(format!("the tar can't be read ({e}).")))?;
        let path = String::from_utf8(e.path_bytes().into_owned())
            .map_err(|_| bad("a name in it isn't UTF-8.".into()))?;
        let kind = e.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(bad(format!("{path} is a link, which obid never follows.")));
        }
        let trimmed = if kind.is_dir() {
            path.strip_suffix('/').unwrap_or(&path)
        } else {
            path.as_str()
        };
        if !plain(trimmed) {
            return Err(bad(format!("{path:?} isn't a plain path inside it.")));
        }
        if kind.is_dir() {
            continue;
        }
        if !kind.is_file() {
            return Err(bad(format!("{path} isn't a plain file.")));
        }
        if !lower.insert(path.to_lowercase()) {
            return Err(bad(format!("{path} is in it twice.")));
        }
        let (most, _) = limits::most_for(&path);
        let mut data = vec![];
        (&mut e)
            .take(most + 1)
            .read_to_end(&mut data)
            .map_err(|e| bad(format!("{path} can't be read ({e}).")))?;
        out.insert(path, data);
    }
    // `a` and `a/b` can't both be kept: one would have to be a file and a folder.
    for path in out.keys() {
        let mut at = path.as_str();
        while let Some((up, _)) = at.rsplit_once('/') {
            if out.contains_key(up) {
                return Err(bad(format!("{up} is both a file and a folder in it.")));
            }
            at = up;
        }
    }
    Ok(out)
}

/// Why `files` aren't kept for Claude's limits, if they aren't (ARTIFACTS-8's, said as received).
fn over_limits(files: &Files) -> Option<String> {
    if let Some(why) = limits::too_many(files.len()) {
        return Some(why);
    }
    let mut total: u64 = 0;
    for (path, bytes) in files {
        if let Some(why) = limits::file_too_big(path, bytes.len() as u64) {
            return Some(why);
        }
        total += bytes.len() as u64;
    }
    (total > limits::MAX_BYTES).then(limits::version_too_big)
}

/// Whether a path is a file that could instruct an agent (protocol.md section 11, "Receiving an
/// artifact"), in any letter case (macOS ignores it): a file named `AGENTS.md`, `CLAUDE.md`,
/// `GEMINI.md` or `.mcp.json` anywhere, or anything under a `.claude/`, `.cursor/`, `.codex/`,
/// `.gemini/`, `.github/`, `hooks/` or `skills/` folder.
pub fn instructs(path: &str) -> bool {
    let lower = path.to_lowercase();
    let (folders, name) = lower.rsplit_once('/').unwrap_or(("", &lower));
    matches!(name, "agents.md" | "claude.md" | "gemini.md" | ".mcp.json")
        || folders.split('/').any(|p| {
            matches!(
                p,
                ".claude" | ".cursor" | ".codex" | ".gemini" | ".github" | "hooks" | "skills"
            )
        })
}

/// The files kept, and those set apart because they instruct agents.
pub fn split_instructions(files: Files) -> (Files, Files) {
    files.into_iter().partition(|(p, _)| !instructs(p))
}

/// What the offer says (protocol.md 5.20): what the recipient holds the parts, the whole, the tar
/// and the files to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offered<'a> {
    /// `blob`: the whole ciphertext.
    pub blob: &'a Part,
    /// `blobs`: each part, in order, when there's more than one.
    pub blobs: &'a [Part],
    /// `artifact.sha256`: the tar's.
    pub sha256: &'a str,
    /// `artifact.files` and `artifact.bytes`: no more files or bytes than these.
    pub files: usize,
    pub bytes: u64,
}

impl<'a> From<&'a Sealed> for Offered<'a> {
    fn from(s: &'a Sealed) -> Offered<'a> {
        Offered {
            blob: &s.blob,
            blobs: &s.blobs,
            sha256: &s.sha256,
            files: s.files,
            bytes: s.bytes,
        }
    }
}

/// What a recipient keeps of a version, and what waits apart for the owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Received {
    pub kept: Files,
    pub apart: Files,
}

/// The texts the secret scan reads of `files`: each file, as text whatever its bytes, named by its
/// path, then every path.
fn texts(files: &Files) -> Vec<(String, String)> {
    let mut texts: Vec<(String, String)> = files
        .iter()
        .map(|(p, b)| (p.clone(), String::from_utf8_lossy(b).into_owned()))
        .collect();
    let names: Vec<&str> = files.keys().map(String::as_str).collect();
    texts.push(("the files' names".into(), names.join("\n")));
    texts
}

/// A recipient's checks on `parts` (as fetched, in order: one, or the offer's `blobs`) against what
/// `offered` says, opened with this Oby's own seal key, in this order: the parts' count, each
/// part's size and hash, the whole one's, decrypting, the tar's hash, the tar ([`unpack`]), no more
/// files or bytes than offered, Claude's limits, then `scan` (crate::scan::scan on the texts, in
/// obid): any finding, or a scan that can't run, and nothing is kept. Then the files that instruct
/// agents set apart. The first that fails is the refusal, and nothing is kept.
pub async fn receive<F, Fut>(
    offered: &Offered<'_>,
    parts: &[Vec<u8>],
    seal_private: &[u8; 32],
    scan: F,
) -> Result<Received, Refused>
where
    F: FnOnce(Vec<(String, String)>) -> Fut,
    Fut: Future<Output = Verdict>,
{
    let wanted = offered.blobs.len().max(1);
    if parts.len() != wanted || offered.blobs.len() == 1 || wanted > MAX_BLOBS {
        return Err(not_kept(
            "bad_blob",
            format!("the offer names {wanted} parts, and {} came.", parts.len()),
        ));
    }
    let named = match offered.blobs {
        [] => std::slice::from_ref(offered.blob),
        blobs => blobs,
    };
    for (n, (part, said)) in parts.iter().zip(named).enumerate() {
        if part.len() > BLOB_BYTES || Part::of(part) != *said {
            return Err(not_kept(
                "bad_blob",
                format!("part {} isn't the one offered (its hash differs).", n + 1),
            ));
        }
    }
    let cipher = parts.concat();
    if Part::of(&cipher) != *offered.blob {
        return Err(not_kept(
            "bad_blob",
            "its blob isn't the one offered (its hash differs).",
        ));
    }
    let tar = open(&cipher, seal_private)
        .map_err(|e| not_kept("bad_blob", format!("it can't be decrypted ({e}).")))?;
    if sha256(&tar) != offered.sha256 {
        return Err(not_kept(
            "bad_blob",
            "what it holds isn't what was offered (its hash differs).",
        ));
    }
    let files = unpack(&tar)?;
    let bytes: u64 = files.values().map(|b| b.len() as u64).sum();
    if files.len() > offered.files || bytes > offered.bytes {
        return Err(not_kept(
            "bad_blob",
            format!(
                "it holds {} files and {bytes} bytes, more than the {} files and {} bytes offered.",
                files.len(),
                offered.files,
                offered.bytes
            ),
        ));
    }
    if let Some(why) = over_limits(&files) {
        return Err(not_kept("too_big", why));
    }
    match scan(texts(&files)).await {
        Verdict::Clean => {}
        Verdict::Found(found) => {
            let file = found
                .first()
                .map(|f| crate::scan::shown_name(&f.name))
                .unwrap_or_default();
            return Err(not_kept(
                "secret_found",
                format!("the secret scan found what looks like a secret in {file}."),
            ));
        }
        Verdict::CouldNotRun(why) => {
            return Err(not_kept(
                "scan_failed",
                format!("the secret scan couldn't run ({why})."),
            ));
        }
    }
    let (kept, apart) = split_instructions(files);
    Ok(Received { kept, apart })
}

/// `apart` written under `<artifacts>/quarantine/<id>/` (folders 0700, files 0600), where the
/// owner decides about them; the paths written. `id` is one of obid's artifact ids, and every path
/// a plain one ([`unpack`] took no other).
pub fn set_apart(artifacts: &Path, id: &str, apart: &Files) -> std::io::Result<Vec<String>> {
    let refused = |why: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, why);
    if uuid::Uuid::try_parse(id).is_err() || id.len() != 36 {
        return Err(refused(format!("{id:?} isn't an artifact's id")));
    }
    let dir: PathBuf = artifacts.join(QUARANTINE).join(id);
    for (path, bytes) in apart {
        if !plain(path) {
            return Err(refused(format!("{path:?} isn't a plain path")));
        }
        let to = dir.join(path);
        crate::store::make_private_dir(to.parent().unwrap_or(&dir))?;
        crate::store::write_private(&to, bytes)?;
    }
    Ok(apart.keys().cloned().collect())
}

/// Another Oby's words or files as an agent may see them: fenced as data from outside, with a
/// marker it can't guess and that nothing inside can close, never as the owner's words.
pub fn quote(from: &str, what: &str, text: &str) -> String {
    let nonce = crate::brain::briefing::nonce();
    format!(
        "{}\n{}",
        crate::brain::briefing::fence_rule(&nonce, &format!("{what} from {from}")),
        crate::brain::briefing::fence(&nonce, text)
    )
}

#[cfg(test)]
mod tests;
