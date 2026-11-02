// Which obid this app uses at launch (docs/contracts/engine.md). No Electron here, so its tests run
// anywhere; main/obid.ts's startObid carries it out.

// one `key = value` line of `launchctl print`
export const field = (printed: string | null, key: string) =>
  printed?.split('\n').map((l) => l.trim()).find((l) => l.startsWith(`${key} = `))?.slice(key.length + 3);

// Who listens on 127.0.0.1:7777 (main/obid.ts, listener)
export type Who = 'mine' | 'foreign' | 'none';
// Another agent launchd has loaded that starts an obid (the Swift Oby's, install.sh's), as printed
export type Other = { label: string; printed: string };

export type Plan =
  // something that isn't this user's obid holds the port: nothing registered or stopped
  | { do: 'taken' }
  // another agent's obid holds the port: it's used, and this app's own stands down
  | { do: 'adopt' }
  // another agent's obid is running and doesn't listen yet: look again once it had time to
  | { do: 'wait' }
  // this app's own runs (who: 'mine') or starts (who: 'none'); retire: the agents to take out of
  // launchd first, which would start their obid again and go for the port
  | { do: 'own'; who: 'mine' | 'none'; retire: string[] };

// Both agents' plists say KeepAlive { SuccessfulExit: false } (apps/mac/app/Tools/
// com.deskree.oby.obid.plist, install/install.sh): launchd starts one again after a failure or a
// signal, every 10 s at most, and never after exit 0 until the next login. obid binds the port
// before anything else and exits 1 when it's taken (core/crates/obid/src/lib.rs, serve), so one
// that runs without the port holds no job.
function comesBack({ printed }: Other) {
  const exit = parseInt(field(printed, 'last exit code') ?? '', 10);
  return ['running', 'spawn scheduled'].includes(field(printed, 'state') ?? '') || field(printed, 'last terminating signal') !== undefined || (!isNaN(exit) && exit !== 0);
}

// listening: the pids on the port; waited: 'wait' already had its time. A loaded agent counts only
// if its obid holds the port: stopped, it blocks nothing.
export function atLaunch(who: Who, listening: string[], others: Other[], waited: boolean): Plan {
  if (who === 'foreign') return { do: 'taken' };
  if (who === 'mine' && others.some(({ printed }) => listening.includes(field(printed, 'pid') ?? ''))) return { do: 'adopt' };
  if (who === 'none' && !waited && others.some(({ printed }) => field(printed, 'state') === 'running')) return { do: 'wait' };
  return { do: 'own', who, retire: others.filter(comesBack).map((o) => o.label) };
}
