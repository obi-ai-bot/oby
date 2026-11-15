//! obid's API token, read from `home/api-token` only under the client rules obid documents
//! (docs/api/configuration.md, "The Mac app on the same Mac"). oby never writes the file.

use rustix::fs::{Mode, OFlags};
use std::fs::{File, Metadata};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub const FILE: &str = "api-token";
const MAX_BYTES: u64 = 4096;
const MIN_LEN: usize = 32;

/// The token. It has no `Debug` or `Display`: it is only ever written into an `Authorization` header.
#[derive(Clone)]
pub struct Token(String);

impl Token {
    pub fn bearer(&self) -> String {
        format!("Bearer {}", self.0)
    }
}

/// What the file's metadata says, for the rules below.
#[derive(Clone, Copy)]
struct Facts {
    link: bool,
    regular: bool,
    owner: u32,
    mode: u32,
    len: u64,
}

impl Facts {
    fn of(meta: &Metadata) -> Facts {
        Facts {
            link: meta.file_type().is_symlink(),
            regular: meta.file_type().is_file(),
            owner: meta.uid(),
            mode: meta.mode(),
            len: meta.len(),
        }
    }

    /// The rule the file breaks, if any, for the user `me`.
    fn refusal(self, me: u32) -> Option<&'static str> {
        if self.link {
            Some("is a link, and oby reads the token only from the file itself")
        } else if !self.regular {
            Some("isn't a regular file")
        } else if self.owner != me {
            Some("isn't owned by you")
        } else if self.mode & 0o077 != 0 {
            Some("can be read or written by others: make it yours alone (chmod 600)")
        } else if self.len > MAX_BYTES {
            Some("is over 4096 bytes")
        } else {
            None
        }
    }
}

/// At least 32 visible ASCII characters, with only a trailing CR/LF taken off.
fn text(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .rposition(|b| *b != b'\n' && *b != b'\r')
        .map_or(0, |i| i + 1);
    let token = &bytes[..end];
    (token.len() >= MIN_LEN && token.iter().all(u8::is_ascii_graphic))
        .then(|| String::from_utf8_lossy(token).into_owned())
}

/// The token at `path`, or why it isn't one: the file's name and the rule, never what it holds.
pub fn read(path: &Path) -> Result<Token, String> {
    let why = |rule: &str| format!("{}: {rule}", path.display());
    let me = rustix::process::getuid().as_raw();
    let meta = std::fs::symlink_metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => why("there's no such file: obid makes it when it starts"),
        _ => why("can't be read"),
    })?;
    if let Some(rule) = Facts::of(&meta).refusal(me) {
        return Err(why(rule));
    }
    // Opened without following a link and judged again as opened, so a swap after the look above
    // reads nothing.
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut file =
        File::from(rustix::fs::open(path, flags, Mode::empty()).map_err(|_| why("can't be read"))?);
    let meta = file.metadata().map_err(|_| why("can't be read"))?;
    if let Some(rule) = Facts::of(&meta).refusal(me) {
        return Err(why(rule));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| why("can't be read"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(why("is over 4096 bytes"));
    }
    text(&bytes)
        .map(Token)
        .ok_or_else(|| why("doesn't hold a token: at least 32 visible ASCII characters"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const GOOD: &str = "0123456789abcdef0123456789abcdef";

    fn file(dir: &Path, text: &[u8], mode: u32) -> std::path::PathBuf {
        let path = dir.join(FILE);
        std::fs::write(&path, text).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    fn refused(path: &Path, rule: &str) {
        match read(path) {
            Ok(_) => panic!("{} was taken", path.display()),
            Err(why) => {
                assert!(why.contains(rule), "{why}");
                assert!(why.starts_with(&path.display().to_string()), "{why}");
                assert!(!why.contains(GOOD), "{why}");
            }
        }
    }

    #[test]
    fn a_0600_file_of_yours_is_taken_with_only_its_trailing_newline_cut() {
        let dir = tempfile::tempdir().unwrap();
        for text in [GOOD.to_string(), format!("{GOOD}\n"), format!("{GOOD}\r\n")] {
            let token = read(&file(dir.path(), text.as_bytes(), 0o600)).unwrap();
            assert_eq!(token.bearer(), format!("Bearer {GOOD}"));
        }
        assert!(read(&file(dir.path(), GOOD.as_bytes(), 0o400)).is_ok());
    }

    #[test]
    fn a_missing_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        refused(&dir.path().join(FILE), "no such file");
    }

    #[test]
    fn a_link_is_refused_even_to_a_good_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("real");
        std::fs::write(&target, GOOD).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.path().join(FILE);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        refused(&link, "is a link");
    }

    #[test]
    fn a_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(FILE)).unwrap();
        refused(&dir.path().join(FILE), "isn't a regular file");
    }

    #[test]
    fn a_file_others_can_reach_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        for mode in [0o640, 0o604, 0o620, 0o602, 0o610, 0o601, 0o644] {
            refused(&file(dir.path(), GOOD.as_bytes(), mode), "chmod 600");
        }
    }

    #[test]
    fn a_file_of_another_user_is_refused() {
        let me = rustix::process::getuid().as_raw();
        let facts = Facts {
            link: false,
            regular: true,
            owner: me,
            mode: 0o100600,
            len: 65,
        };
        assert_eq!(facts.refusal(me), None);
        let theirs = Facts {
            owner: me.wrapping_add(1),
            ..facts
        };
        assert_eq!(theirs.refusal(me), Some("isn't owned by you"));
    }

    #[test]
    fn a_file_over_4096_bytes_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        refused(&file(dir.path(), &[b'a'; 4097], 0o600), "over 4096 bytes");
        assert!(read(&file(dir.path(), &[b'a'; 4096], 0o600)).is_ok());
    }

    #[test]
    fn a_short_or_invisible_token_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        for text in [
            "x".repeat(31),
            format!("{GOOD} x"),
            format!(" {GOOD}"),
            format!("{GOOD}\t"),
            format!("{GOOD}\n\nx"),
            format!("{GOOD}é"),
            String::new(),
        ] {
            refused(
                &file(dir.path(), text.as_bytes(), 0o600),
                "doesn't hold a token",
            );
        }
    }
}
