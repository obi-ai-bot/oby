//! A running command's output as Claude Code keeps it (docs/contracts/live-task.md, LIVETASK-34
//! and 36). Claude Code's stream says a command in the foreground started (`system`
//! `task_started`, with its `task_id`) and nothing more until it ends; meanwhile it writes what the
//! command prints to `<CLAUDE_CODE_TMPDIR>/claude-<uid>/<its folder>/<session>/tasks/<task>.output`
//! (claude 2.1.325, measured 2026-11-17). obid reads that file, and only it: a plain file of obid's
//! own user, under the run's own temporary folder, every folder on the way a real one, the file
//! itself opened without following a link. It owns no state of the job's; the transcript
//! (crate::transcript) keeps what it reads, masked, and never writes it anywhere.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// How much of the file one read takes at most: a command printing faster than this every tenth
/// of a second shows its output a little behind, never all of it at once.
const MOST: u64 = 1024 * 1024;

/// One command's output file, read as it grows.
pub struct Spill {
    temp: PathBuf,
    session: String,
    task: String,
    file: Option<File>,
    read: u64,
    /// The bytes of a character the last read cut in two.
    partial: Vec<u8>,
}

impl Spill {
    /// The output of Claude Code's task `task` in its session `session`, under `temp`, the run's
    /// `CLAUDE_CODE_TMPDIR`.
    pub fn new(temp: &Path, session: &str, task: &str) -> Spill {
        Spill {
            temp: temp.to_path_buf(),
            session: session.to_owned(),
            task: task.to_owned(),
            file: None,
            read: 0,
            partial: Vec::new(),
        }
    }

    /// What the command printed since the last call, as text (a character cut in two waits for
    /// the rest): nothing while the file isn't there, isn't a plain file of obid's user under the
    /// run's temporary folder, or hasn't grown.
    pub fn more(&mut self) -> String {
        if self.file.is_none() {
            self.file = self.open();
        }
        let Some(file) = self.file.as_mut() else {
            return String::new();
        };
        let len = file.metadata().map_or(0, |m| m.len());
        // A file that got shorter is read on from its end.
        self.read = self.read.min(len);
        if len == self.read || file.seek(SeekFrom::Start(self.read)).is_err() {
            return String::new();
        }
        let mut bytes = std::mem::take(&mut self.partial);
        let Ok(got) = file.take(MOST).read_to_end(&mut bytes) else {
            return String::new();
        };
        self.read += got as u64;
        let whole = match std::str::from_utf8(&bytes) {
            Ok(_) => bytes.len(),
            // Only an end cut short waits; bytes that are no text at all go out replaced.
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => bytes.len(),
        };
        self.partial = bytes.split_off(whole);
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// The task's output file, opened, when it's there and may be read: its name and the
    /// session's are names (no `/`, no `..`), the folders to it real ones (no link), and the file
    /// a plain one of obid's user, opened without following a link and without waiting on a pipe.
    fn open(&self) -> Option<File> {
        if !plain_name(&self.session) || !plain_name(&self.task) {
            return None;
        }
        let file_name = format!("{}.output", self.task);
        for user in folders(&self.temp, |name| name.starts_with("claude-")) {
            for place in folders(&user, |_| true) {
                let tasks = place.join(&self.session).join("tasks");
                if !real_folder(&place.join(&self.session)) || !real_folder(&tasks) {
                    continue;
                }
                if let Some(file) = open_plain(&tasks.join(&file_name), &self.temp) {
                    return Some(file);
                }
            }
        }
        None
    }
}

/// A name Claude Code gives a session or a task: letters, digits, `-` and `_`.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A folder itself, not a link to one.
fn real_folder(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

/// The real folders in `path` whose names `keep` keeps.
fn folders(path: &Path, keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    if !real_folder(path) {
        return vec![];
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(&keep))
        .map(|entry| entry.path())
        .filter(|path| real_folder(path))
        .collect()
}

/// `path` opened to read, only as a plain file of the user who owns `temp` (obid's: obid made it),
/// never through a link.
fn open_plain(path: &Path, temp: &Path) -> Option<File> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let owner = std::fs::symlink_metadata(temp).ok()?.uid();
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    (meta.is_file() && meta.uid() == owner).then_some(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(name: &str) -> (PathBuf, PathBuf) {
        let temp = std::env::temp_dir().join(format!("obid-spill-{name}-{}", uuid::Uuid::new_v4()));
        let tasks = temp.join("claude-501/-work-folder/sess-1/tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        (temp, tasks)
    }

    /// LIVETASK-34: what the command prints comes as it grows, a character cut in two once whole.
    #[test]
    fn livetask_34_the_output_file_is_read_as_it_grows() {
        let (temp, tasks) = place("grows");
        let mut spill = Spill::new(&temp, "sess-1", "b21taen9b");
        assert_eq!(spill.more(), "", "no file yet");
        let path = tasks.join("b21taen9b.output");
        std::fs::write(&path, "line 1\n").unwrap();
        assert_eq!(spill.more(), "line 1\n");
        assert_eq!(spill.more(), "");
        let mut bytes = b"line 2 caf".to_vec();
        bytes.push("é".as_bytes()[0]);
        std::fs::write(&path, [b"line 1\n".as_slice(), &bytes].concat()).unwrap();
        assert_eq!(spill.more(), "line 2 caf");
        std::fs::write(
            &path,
            [b"line 1\n".as_slice(), b"line 2 caf", "é\n".as_bytes()].concat(),
        )
        .unwrap();
        assert_eq!(spill.more(), "é\n");
        std::fs::remove_dir_all(&temp).unwrap();
    }

    /// LIVETASK-36: a link where the file or a folder should be is never followed, nor a name
    /// that isn't one; a pipe there is never waited on.
    #[test]
    fn livetask_36_a_link_in_its_place_is_never_read() {
        let (temp, tasks) = place("link");
        let secret = temp.join("vault.json");
        std::fs::write(&secret, "the vault\n").unwrap();
        std::os::unix::fs::symlink(&secret, tasks.join("t1.output")).unwrap();
        assert_eq!(Spill::new(&temp, "sess-1", "t1").more(), "");
        // A session folder that is a link to another place.
        let elsewhere = temp.join("elsewhere/tasks");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("t2.output"), "not its own\n").unwrap();
        std::os::unix::fs::symlink(
            temp.join("elsewhere"),
            temp.join("claude-501/-work-folder/sess-2"),
        )
        .unwrap();
        assert_eq!(Spill::new(&temp, "sess-2", "t2").more(), "");
        // Names that would climb out.
        assert_eq!(Spill::new(&temp, "sess-1", "../../vault").more(), "");
        assert_eq!(Spill::new(&temp, "..", "t1").more(), "");
        // A pipe: opened without waiting, and not a plain file.
        let fifo = tasks.join("t3.output");
        let made = std::process::Command::new("/usr/bin/mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(made.success());
        assert_eq!(Spill::new(&temp, "sess-1", "t3").more(), "");
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
