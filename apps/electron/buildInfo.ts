// The constants src/shared/build.ts reads, for electron.vite.config.ts: the commit this tree is
// (OBY_BUILD_COMMIT, as scripts/run-dev.sh sets it, else git's HEAD here), and a scratch build's
// obid (OBY_SCRATCH_PORT and OBY_SCRATCH_HOME, both or neither; docs/build-and-run.md).
import { execFileSync } from 'node:child_process';
import { isAbsolute } from 'node:path';

function commit(): string {
  const given = process.env.OBY_BUILD_COMMIT?.trim();
  if (given) return given;
  try { return execFileSync('git', ['rev-parse', 'HEAD'], { cwd: __dirname, encoding: 'utf8' }).trim(); } catch { return ''; }
}

function scratch(): { port: number; home: string } | null {
  const port = process.env.OBY_SCRATCH_PORT, home = process.env.OBY_SCRATCH_HOME;
  if (!port && !home) return null;
  const n = Number(port);
  if (!Number.isInteger(n) || n < 1024 || n > 65535 || n === 7777) throw new Error(`OBY_SCRATCH_PORT has to be a port from 1024 to 65535 other than 7777, not ${port}`);
  if (!home || !isAbsolute(home)) throw new Error(`OBY_SCRATCH_HOME has to be an absolute folder, not ${home}`);
  return { port: n, home };
}

export function buildInfo() {
  return { __OBY_COMMIT__: JSON.stringify(commit()), __OBY_SCRATCH__: JSON.stringify(scratch()) };
}
