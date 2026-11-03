// Which processes are the test rig's (e2e/rig.mjs, docs/contracts/test-rig.md), from what ps says
// now: a pid alone proves nothing once its process has gone, and a command line naming the rig's
// folder can be anyone's, a person's `tail` or the shell that called the rig.
import { execFileSync } from "node:child_process";

const ROW = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(\S+)\s+(\S+\s+\S+\s+\d+\s+[\d:]+\s+\d+)\s+(.*)$/;

// Every process but the zombies.
export function table() {
  return execFileSync("ps", ["-axww", "-o", "pid=,ppid=,pgid=,stat=,lstart=,command="], { encoding: "utf8", maxBuffer: 1 << 26, env: { PATH: process.env.PATH, LC_ALL: "C" } })
    .split("\n").map((row) => ROW.exec(row)).filter(Boolean)
    .map(([, pid, ppid, pgid, stat, lstart, command]) => ({ pid: Number(pid), ppid: Number(ppid), pgid: Number(pgid), stat, lstart, command }))
    .filter((p) => !p.stat.startsWith("Z"));
}

// What's remembered of a process just started, or null when it has gone already.
export function record(pid, rows = table()) {
  const found = rows.find((p) => p.pid === pid);
  return found ? { pid, lstart: found.lstart } : null;
}

// Whether a remembered process is still the rig's: the same process, started when it was, and
// naming a path under the rig's folder. A leader that has gone leaves its group's members to say so
// (the Sphere's launcher can go before workerd), and its pid can't be another's while they live.
export function ours(remembered, dir, rows = table()) {
  if (!remembered?.pid) return false;
  const under = (p) => p.command.includes(`${dir}/`);
  const leader = rows.find((p) => p.pid === remembered.pid);
  if (leader) return leader.lstart === remembered.lstart && under(leader);
  return rows.some((p) => p.pgid === remembered.pid && under(p));
}

// The processes below `roots`, by parent and by group, roots too, as records: taken before anything
// is stopped, since an agent in a group of its own has no parent left to find it by after. Never
// `self` or what started it.
export function family(roots, self = process.pid, rows = table()) {
  const above = new Set();
  for (let at = rows.find((p) => p.pid === self); at && !above.has(at.pid); at = rows.find((p) => p.pid === at.ppid)) above.add(at.pid);
  const found = new Set(roots);
  for (let grew = true; grew;) {
    grew = false;
    for (const p of rows) if (!found.has(p.pid) && (found.has(p.ppid) || found.has(p.pgid))) { found.add(p.pid); grew = true; }
  }
  return rows.filter((p) => found.has(p.pid) && !above.has(p.pid)).map((p) => ({ pid: p.pid, lstart: p.lstart }));
}

// Of `records`, those still the same process.
export const stillUp = (records, rows = table()) => records.filter((r) => rows.some((p) => p.pid === r.pid && p.lstart === r.lstart));

// What the rig can't say is its own and doesn't touch: what a killed obid left, with no parent.
export const orphans = (dir, rows = table()) => rows.filter((p) => p.ppid === 1 && p.command.includes(`${dir}/`)).map((p) => p.pid);

// The exchange this question made: new since `before`, with these words. The same words asked
// earlier are another exchange, and a question that wasn't sent has none.
export function newExchange(before, after, words) {
  const known = new Set(before.map((e) => e.correlationId));
  return after.find((e) => !known.has(e.correlationId) && e.messages?.[0]?.envelope?.body?.text === words);
}
