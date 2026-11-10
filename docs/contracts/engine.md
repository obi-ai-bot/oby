# Which engine the Electron app uses

The business behavior contract for what the Electron app does about obid at launch on a Mac that may also have the Swift Oby's login item (`com.deskree.oby.obid`) or install.sh's agent (`com.deskree.obid`) ([#1850](https://github.com/obi-ai-bot/oby/issues/1850)). The words the person sees are the app's existing ones; this contract adds none.

## Feature and status

- **Outcome:** Oby works on a Mac that ever ran the Swift Oby. This app's own engine is the engine.
- **Status:** **Implemented** in `apps/electron`: `startObid`, `retireSwift` and `gone` in `src/main/obid.ts`. The owner's decision of 2026-11-02: the Swift Oby is to be deleted, and the work is 100% on the Electron app. Replaces adopting the Swift Oby's engine and standing down for it.
- **Scope:** a packaged app's launch on macOS, and its obid coming back while the window is open (ENGINE-12). **Out of scope:** telling the person that their engine changed (the words would be Sangam's).

## Behavior

- **Actors and ownership:** the app's main process decides and acts through launchd. obid owns nothing here.
- **Triggers and prerequisites:** the packaged app opens; `startObid` runs once per launch. A development run starts nothing and retires nothing.
- **The Swift Oby's agent:** when launchd has it loaded, running or not, holding the port or not, and this app's own runs or is about to start (this copy is in Applications, and nothing foreign or install.sh's holds the port), the app runs `launchctl bootout` and then `launchctl disable` for that label, each for up to 5 seconds, waits up to 5 seconds for its pid to leave the port, and goes on. It is never adopted, deferred to or stood down for, and its obid doesn't get the token. `bootout` stops it and its `KeepAlive`; `disable` keeps `RunAtLoad` from bringing it back at a login. This is deliberate and lasts: the Swift app can't start its engine again on that Mac until `launchctl enable gui/<uid>/com.deskree.oby.obid`.
- **install.sh's agent:** never booted out or disabled. Holding 7777, it's adopted and this app's own stands down, as before. Running on another port, or stopped, it blocks nothing and this app's own starts. An install.sh agent can listen elsewhere with `--listen`.
- **Failures and recovery:** a refused or hung `bootout` or `disable` stops nothing else. If the Swift Oby's obid still holds the port, it counts as anything else on the port: the window shows the port is taken, and nothing is registered. If this app's own can't start, the window says why, as for any start.
- **obid restarted while the window is open:** a `launchctl kickstart -k` during an install, a configuration reload, or launchd's `KeepAlive` bringing back a crashed obid leaves nothing on the port for a few seconds. When a call finds nothing answering and launchd still has this app's agent (or its session job), main waits up to 20 seconds for obid to answer again, in one wait every call shares: meanwhile calls answer that the engine is starting (`why: 'waiting'`, no error on the page) and run no `lsof` or `launchctl` of their own. If obid answers, calls go on, and the event stream reconnects as before. If it doesn't, calls get "Nothing answers on 127.0.0.1:7777" with launchd's reason (`why()`), and for 30 seconds after that they fail at once instead of waiting again. With launchd not having this app's agent, or something foreign on the port, nothing changes.
- **Invariants and boundaries:** only this app's own obid, or install.sh's, gets the token. A foreign program on the port gets nothing, and nothing is registered or stopped for it.

## Known limits

- `launchctl disable` lasts, and the person isn't told their engine changed.
- If macOS then asks for Login Items approval, the Swift Oby's agent has already gone: there's no engine until the person approves, as the window says.
- A still-held port shows the existing "port is taken" words, which name another user's Oby or another program, not the Swift Oby.
- `launchctl print` is given 5 seconds: a hung one counts as "launchd doesn't have it", so neither a start nor a restart wait (ENGINE-12) can stay "starting" for good (`engine_12_a_launchctl_that_hangs_still_ends_the_wait`).
- `bootout` and `disable` haven't run against a real SMAppService agent.

## Interfaces and dependencies

launchd (`launchctl print`, `bootout`, `disable`), `lsof`, SMAppService through Electron. No obid API changes.

## Acceptance scenarios

Tests: `apps/electron/src/renderer/src/test/engine.test.ts`, through `startObid` on a fake launchd, `lsof` and port, named after these.

- **ENGINE-1** The Swift Oby's agent is running off the port: it's booted out and disabled, and this app registers its own.
- **ENGINE-2** The Swift Oby's agent is stopped: it's booted out and disabled, and this app registers its own.
- **ENGINE-3** The Swift Oby's obid holds the port: it's booted out and disabled, the app waits for the port, registers its own, and never stands down.
- **ENGINE-4** Nothing is loaded: no agent is touched, and this app registers its own.
- **ENGINE-5** A foreign program holds the port: the window says the port is taken, and nothing is registered or booted out, the Swift Oby's agent included.
- **ENGINE-6** The Swift Oby's obid holds the port and `bootout` is refused: `disable` is still tried, the window says the port is taken, its obid isn't adopted, and nothing is registered.
- **ENGINE-7** install.sh's agent runs on another port: it's left alone, and this app registers its own.
- **ENGINE-8** install.sh's agent holds the port: it's adopted, this app's own stands down, and neither it nor a loaded Swift Oby's agent is booted out.
- **ENGINE-9** This copy is outside Applications and the person chooses Not Now: the Swift Oby's agent is left alone, on the port or not, and the window says why no engine starts.
- **ENGINE-10** `launchctl` hangs on `bootout` and `disable`: each times out, and this app registers its own.
- **ENGINE-11** This app's own already holds the port: it stays registered, and a loaded Swift Oby's agent is booted out and disabled.
- **ENGINE-12** (implemented) This app's obid stops answering while its agent is still registered: calls answer 'waiting' through one shared wait; if it's back within 20 seconds, calls succeed again; if not, they get the hard error with launchd's reason and fail fast for 30 seconds. Without this app's agent registered, the hard error is at once.
