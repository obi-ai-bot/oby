//! `oby`: obid's loopback API from a shell, a script or a git hook, with one exit code a script
//! can trust (docs/contracts/cli.md). A client of obid's existing routes and nothing more.

pub mod args;
mod commands;
pub mod endpoint;
pub mod exit;
mod http;
pub mod token;

use args::{Command, Usage, USAGE};
use exit::Exit;
use std::io::Write;

/// Why a run ended early, with the code it ends with. The message names files and rules, never the
/// token.
pub struct Fail {
    pub exit: Exit,
    pub message: String,
}

impl Fail {
    pub fn new(exit: Exit, message: impl Into<String>) -> Fail {
        Fail {
            exit,
            message: message.into(),
        }
    }
}

/// One line on stdout; a closed pipe isn't oby's to report.
pub(crate) fn out(line: &str) {
    let _ = writeln!(std::io::stdout().lock(), "{line}");
}

/// The whole run, with its exit code.
pub fn main(args: &[String]) -> i32 {
    let command = match args::parse(args, &mut std::io::stdin()) {
        Ok(Command::Help) => {
            let _ = write!(std::io::stdout().lock(), "{USAGE}");
            return Exit::Done as i32;
        }
        Ok(command) => command,
        Err(Usage(why)) => {
            eprint!("oby: {why}\n{USAGE}");
            return Exit::Usage as i32;
        }
    };
    match commands::run(command) {
        Ok(exit) => exit as i32,
        Err(fail) => {
            eprintln!("oby: {}", fail.message);
            fail.exit as i32
        }
    }
}
