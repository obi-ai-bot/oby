//! `obid serve --config /path/to/config.json` (docs/runner-api.md, "obid only: configuration"),
//! and `obid vault --config /path/to/config.json list | add NAME | remove NAME`: the owner's vault
//! from a terminal on the machine, a value only ever on stdin (obid::vault).

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use obid::config::Config;

const USAGE: &str = "usage: obid serve --config /path/to/config.json\n       obid serve   (macOS, as Oby.app starts it: ~/Library/Application Support/Oby/obid/config.json)\n       obid vault --config /path/to/config.json list | add NAME (the value on stdin) | remove NAME\n       obid --version";

enum Command {
    /// `config` none: as Oby.app starts it (obid::app), on macOS only.
    Serve {
        config: Option<PathBuf>,
    },
    Vault {
        config: PathBuf,
        action: obid::vault::Action,
    },
    Version,
    Help,
}

fn parse(args: &[String]) -> Result<Command, String> {
    match args {
        [flag] if flag == "--version" || flag == "-V" => Ok(Command::Version),
        [flag] if flag == "--help" || flag == "-h" => Ok(Command::Help),
        [serve] if serve == "serve" && cfg!(target_os = "macos") => {
            Ok(Command::Serve { config: None })
        }
        [serve, flag, path] if serve == "serve" && flag == "--config" => Ok(Command::Serve {
            config: Some(PathBuf::from(path)),
        }),
        [serve, flag] if serve == "serve" && flag.starts_with("--config=") => Ok(Command::Serve {
            config: Some(PathBuf::from(&flag["--config=".len()..])),
        }),
        [vault, flag, path, rest @ ..] if vault == "vault" && flag == "--config" => {
            Ok(Command::Vault {
                config: PathBuf::from(path),
                action: obid::vault::Action::parse(rest)?,
            })
        }
        [vault, flag, rest @ ..] if vault == "vault" && flag.starts_with("--config=") => {
            Ok(Command::Vault {
                config: PathBuf::from(&flag["--config=".len()..]),
                action: obid::vault::Action::parse(rest)?,
            })
        }
        [vault, ..] if vault == "vault" => Err(obid::vault::USAGE.into()),
        _ => Err(USAGE.into()),
    }
}

fn main() -> ExitCode {
    // `obid __session -- <program> <arguments>`: how obid starts every sandboxed process, in a
    // session of its own (obid::sandbox::SESSION). Read before anything else, and as it is: its
    // arguments needn't be UTF-8.
    let raw: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if raw.first().is_some_and(|a| a == obid::sandbox::SESSION) {
        if raw.get(1).is_none_or(|a| a != "--") {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
        let error = obid::process::exec_in_own_session(&raw[2..]);
        eprintln!("obid: can't start the sandboxed process: {error}");
        return ExitCode::from(126);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match parse(&args) {
        Ok(command) => command,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };
    // Where a start that fails says why (obid::START_ERROR): the configuration's `home`, or, while
    // that can't be read, the one Oby.app's obid uses.
    let mut start_home: Option<PathBuf> = None;
    let config_path = match command {
        Command::Version => {
            println!("obid {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Command::Help => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Command::Serve {
            config: Some(config),
        } => config,
        // As Oby.app starts it: install.sh's file, written the first time, and its log.
        Command::Serve { config: None } => {
            let Some(home) = nix::unistd::User::from_uid(nix::unistd::getuid())
                .ok()
                .flatten()
                .map(|user| user.dir)
                .filter(|dir| dir.is_absolute())
            else {
                fatal("can't find the person's home folder");
                return ExitCode::FAILURE;
            };
            if let Err(e) = obid::app::log_to_file(&home) {
                fatal(&e);
            }
            start_home = Some(obid::app::obid_home(&home));
            let path = obid::app::config_path(&home);
            if let Err(e) = obid::app::ensure_config(&path, &home) {
                return fail(start_home.as_deref(), e, 1);
            }
            path
        }
        Command::Vault { config, action } => return vault(&config, &action),
    };

    // The runtime, then SIGTERM and SIGINT caught, before anything else: from here on a stop
    // signal, however early, ends obid with exit 0 (docs/runner-api.md, "obid only").
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            fatal(&format!("can't start the runtime: {e}"));
            return ExitCode::FAILURE;
        }
    };
    let stop = {
        let _entered = runtime.enter();
        match obid::StopSignal::install() {
            Ok(stop) => stop,
            Err(e) => {
                fatal(&format!("can't catch SIGTERM and SIGINT: {e}"));
                return ExitCode::FAILURE;
            }
        }
    };

    // Plug-ins' lines (the `obi_plugin` target) go to the log only with the secrets obid holds
    // taken out (obid::logs, docs/ARCHITECTURE.md 5.4).
    tracing_subscriber::fmt()
        .with_writer(obid::logs::Redacting::new(
            std::io::stderr,
            obid::secrets::known(),
        ))
        .with_ansi(std::io::stderr().is_terminal())
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("OBID_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("obid {} starting", env!("CARGO_PKG_VERSION"));
    let config = match Config::load(&config_path) {
        Ok(config) => config,
        Err(e) => {
            let home = home_in(&config_path).or(start_home);
            return fail(home.as_deref(), e, 2);
        }
    };
    for key in &config.ignored {
        tracing::warn!(
            "{}: {key} isn't a key this obid knows: ignored",
            config_path.display()
        );
    }
    let home = config.home.clone();
    // The token only ever from home/api-token: never from obid's environment.
    if let Err(e) =
        obid::token::refuse_environment(&config.home, std::env::var_os(obid::token::ENV).is_some())
    {
        return fail(Some(&home), e, 2);
    }
    if let Err(e) = obid::prepare_home(&config) {
        return fail(Some(&home), e, 1);
    }
    let token = match obid::token::load(&config.home) {
        Ok(token) => token,
        Err(e) => return fail(Some(&home), e, 1),
    };

    let result = runtime.block_on(obid::serve(config, token, stop.wait(), |address| {
        // One line on stdout that says where obid answers; never the token.
        println!("obid listening on http://{address}");
        tracing::info!(%address, "obid listening");
        let _ = std::fs::remove_file(home.join(obid::START_ERROR));
    }));
    // Whatever a connection still holds, obid has stopped answering: don't wait on it.
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(Some(&home), e, 1),
    }
}

/// Says `reason` on stderr and keeps it in `home/last-start-error` (obid::START_ERROR), where the
/// apps read why obid isn't there, then exits with `code`.
fn fail(home: Option<&std::path::Path>, reason: impl std::fmt::Display, code: u8) -> ExitCode {
    let reason = reason.to_string();
    fatal(&reason);
    if let Some(home) = home {
        obid::note_start_error(home, &reason);
    }
    ExitCode::from(code)
}

/// `line` on stderr, with the time, like the log's own lines: obid's last word before it exits.
fn fatal(line: &str) {
    let now = obi_protocol::time::format(obi_protocol::time::now());
    eprintln!("{now} obid: {line}");
}

/// The absolute `home` the configuration at `path` names, read as plain JSON: for a configuration
/// obid refused.
fn home_in(path: &std::path::Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(PathBuf::from(value.get("home")?.as_str()?)).filter(|home| home.is_absolute())
}

/// `obid vault`: the vault in the configuration's `home`, run as obid's user. Nothing it prints
/// holds a value.
fn vault(config: &std::path::Path, action: &obid::vault::Action) -> ExitCode {
    let config = match Config::load(config) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("obid: {e}");
            return ExitCode::from(2);
        }
    };
    ExitCode::from(obid::vault::run(
        &config.home,
        &config.vault,
        action,
        obid::vault::read_value,
        |line| println!("{line}"),
        |line| eprintln!("obid: {line}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn commands() {
        assert!(
            matches!(parse(&args(&["serve", "--config", "/c.json"])), Ok(Command::Serve { config: Some(config) }) if config.as_path() == Path::new("/c.json"))
        );
        assert!(
            matches!(parse(&args(&["serve", "--config=/c.json"])), Ok(Command::Serve { config: Some(config) }) if config.as_path() == Path::new("/c.json"))
        );
        assert!(matches!(parse(&args(&["--version"])), Ok(Command::Version)));
        assert!(
            matches!(parse(&args(&["vault", "--config", "/c.json", "add", "staging-key"])), Ok(Command::Vault { config, action: obid::vault::Action::Add(name) }) if config.as_path() == Path::new("/c.json") && name == "staging-key")
        );
        assert!(matches!(
            parse(&args(&["vault", "--config=/c.json", "list"])),
            Ok(Command::Vault {
                action: obid::vault::Action::List,
                ..
            })
        ));
        // `obid serve` alone is how Oby.app starts it, on macOS only.
        assert_eq!(
            matches!(
                parse(&args(&["serve"])),
                Ok(Command::Serve { config: None })
            ),
            cfg!(target_os = "macos")
        );
        for wrong in [
            &[][..],
            &["serve", "--config"],
            &["serve", "/c.json"],
            &["run", "--config", "/c.json"],
            // A value is never an argument.
            &[
                "vault",
                "--config",
                "/c.json",
                "add",
                "staging-key",
                "the-value",
            ],
            &["vault", "add", "staging-key"],
            &["vault", "--config", "/c.json"],
        ] {
            assert!(parse(&args(wrong)).is_err(), "{wrong:?}");
        }
    }
}
