# Which engine the Electron app uses

The business behavior contract for what the Electron app does about obid at launch on a Mac that may also have the Swift Oby's login item (`com.deskree.oby.obid`) or install.sh's agent (`com.deskree.obid`) ([#1850](https://github.com/obi-ai-bot/oby/issues/1850)). The words the person sees are the app's existing ones; this contract adds none.

## Feature and status

- **Outcome:** Oby works on a Mac that ever ran the Swift Oby: an old registration that does nothing can't take the app down.
- **Status:** **Implemented** in `apps/electron`: the decision in `src/shared/engine.ts` (`atLaunch`), carried out by `startObid` in `src/main/obid.ts`. Replaces standing down for any loaded agent, running or not.
- **Scope:** a packaged app's launch on macOS. **Out of scope:** which engine wins at the next login (below), and telling the person that their engine changed (the words would be Sangam's).

## Behavior

- **Actors and ownership:** the app's main process decides and acts through launchd; obid owns nothing here beyond exiting 1 on a taken port.
- **Triggers and prerequisites:** the packaged app opens. A development run starts nothing.
- **States and transitions:** see the scenarios. A loaded agent counts only when its obid holds the port. One that runs without the port gets up to 10 seconds to bind (obid binds first, so it holds no job until it does), and then counts as stopped.
- **Coming back for the port:** both agents' plists say `KeepAlive { SuccessfulExit: false }`, so launchd starts one again after a failure or a signal, every 10 seconds at most, and not after exit 0 until the next login. Each of those starts could take the port while this app's own restarts, so that agent is taken out of launchd with `launchctl bootout`, until the next login. Its registration and plist stay, and the Swift app doesn't re-register it while an obid answers (`obidRuns` in `apps/mac/app/LoginItem.swift`). One stopped with exit 0 isn't touched.
- **Failures and recovery:** a refused `bootout` changes nothing else: this app's own still starts. If it can't start, the window says why, as for any start.
- **Invariants and boundaries:** only this user's launchd obid gets the token; a foreign program on the port gets nothing, and nothing is registered or stopped for it. `launchctl disable` isn't used: it outlasts the Swift app's own registration, and the frozen Swift app couldn't undo it.
- **At the next login:** `RunAtLoad` starts both agents while the Swift Oby's is still registered, and the first to bind wins. If that's the Swift Oby's, ENGINE-2 applies and it's used, with this app's own unregistered, until it stops answering. Removing the Swift Oby's login item ends the race.

## Interfaces and dependencies

launchd (`launchctl print`, `bootout`), `lsof`, SMAppService through Electron. No obid API changes.

## Acceptance scenarios

Tests: `apps/electron/src/renderer/src/test/engine.test.ts`, named after these.

- **ENGINE-1** An agent is loaded and stopped (exit 0), and nothing answers on 7777: this app registers its own, says nothing about the other, and leaves that agent alone.
- **ENGINE-2** An agent is loaded and its obid holds the port: it's used, and this app's own stands down.
- **ENGINE-3** Nothing is loaded and nothing answers: this app registers its own.
- **ENGINE-4** An agent is loaded and launchd would start it again (failed, killed, or running without the port), and nothing answers: it gets its 10 seconds, then is taken out of launchd, and this app registers its own.
- **ENGINE-5** This app's own holds the port and an agent is loaded: this app's own stays registered; the agent is taken out of launchd only if it would come back.
- **ENGINE-6** A foreign program holds the port: the window says the port is taken, and nothing is registered or stopped, whatever launchd has loaded.
