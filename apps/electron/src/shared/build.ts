// What this copy of the app was built from, baked in by electron.vite.config.ts (buildInfo.ts) and
// never read from the environment at run time (docs/build-and-run.md, "Today's dev on this Mac").

declare const __OBY_COMMIT__: string;
declare const __OBY_SCRATCH__: { port: number; home: string } | null;

// The commit, or '' for a build that wasn't told (a browser's mock, a test)
export const COMMIT: string = typeof __OBY_COMMIT__ === 'string' ? __OBY_COMMIT__ : '';

// A scratch install's obid (scripts/run-dev.sh --port --home): its port and home, baked into that
// build only; null for every other build, which talks to the obid on 7777
export const SCRATCH: { port: number; home: string } | null = typeof __OBY_SCRATCH__ === 'object' ? __OBY_SCRATCH__ : null;

// How About shows a commit: its first 7 hex digits, as git does
export const short = (commit: string | null | undefined) => (commit ?? '').slice(0, 7);

// About's Version: `0.2.23 (abc1234)`, in the place Sangam's screen keeps for a build ("0.9.2
// (412)"); with obid's own when it was built from another commit, `0.2.23 (abc1234, engine def5678)`
export function versionText(version: string, app: string, engine: string | null | undefined): string {
  const mine = short(app), its = short(engine);
  const built = [mine, its && its !== mine ? `engine ${its}` : ''].filter(Boolean).join(', ');
  return built ? `${version} (${built})` : version;
}
