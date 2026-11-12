// What docs/contracts/test-rig.md promises of stop, reset and start without a Sphere or an obid, on
// scratch folders with a stand-in HOME, never near the real one. Run: node --test e2e/rig.test.mjs
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createHash, createPublicKey, generateKeyPairSync, verify } from "node:crypto";
import { once } from "node:events";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { FIXTURE } from "./lib/fixture.mjs";
import { review, reviewBody, REVIEW_SECONDS, RULES } from "./lib/rig-review.mjs";
import { family, newExchange, ours, record, stillUp, table } from "./lib/rig-state.mjs";

const rig = join(dirname(fileURLToPath(import.meta.url)), "rig.mjs");
const FOREVER = "setInterval(() => {}, 1000)";

function aScratchMac(t) {
  const top = realpathSync(mkdtempSync(join(tmpdir(), "oby-rig-")));
  const home = join(top, "home"), dir = join(top, "rig");
  const oby = join(home, "Library/Application Support/Oby");
  mkdirSync(join(oby, "Runner"), { recursive: true });
  writeFileSync(join(oby, "oby.sqlite"), "the owner's");
  mkdirSync(dir);
  const started = [];
  const proc = (...args) => {
    const child = spawn(process.execPath, ["-e", FOREVER, ...args], { detached: true, stdio: "ignore" });
    started.push(child.pid);
    child.unref();
    return child.pid;
  };
  t.after(() => {
    for (const pid of started) { try { process.kill(-pid, "SIGKILL"); } catch {} }
    rmSync(top, { recursive: true, force: true });
  });
  // OBID_BIN names nothing: if a guard here ever breaks, `start` stops at it, with nothing built or started.
  const run = (...args) => spawnSync(process.execPath, [rig, ...args], { encoding: "utf8", env: { PATH: process.env.PATH, HOME: home, OBY_RIG_DIR: dir, OBID_BIN: join(top, "no-obid") } });
  return { top, home, dir, oby, proc, run, started };
}

// A process's command line is there a moment after it's spawned.
async function recorded(pid) {
  for (let i = 0; i < 100; i++) {
    const r = record(pid);
    if (r) { await sleep(50); return record(pid); }
    await sleep(20);
  }
  throw new Error(`${pid} never showed`);
}
const up = (pid) => { try { process.kill(pid, 0); return true; } catch { return false; } };
async function gone(pid) {
  for (let i = 0; i < 100 && up(pid); i++) await sleep(50);
  return !up(pid);
}
const state = (dir, value) => writeFileSync(join(dir, "rig.json"), JSON.stringify(value));

test("rig_9_stop_stops_the_rigs_processes_and_what_they_started", async (t) => {
  const { dir, run, started } = aScratchMac(t);
  // As obid and a job's agent: a child in a group of its own, its command line naming nothing of the rig's.
  const parent = spawn(process.execPath, ["-e", `const c = require("node:child_process").spawn(process.execPath, ["-e", ${JSON.stringify(FOREVER)}], { detached: true, stdio: "ignore" }); c.unref(); console.log(c.pid); ${FOREVER}`, join(dir, "you/config.json")], { detached: true, stdio: ["ignore", "pipe", "ignore"] });
  parent.unref();
  started.push(parent.pid);
  const [line] = await once(parent.stdout, "data");
  const agent = Number(String(line).trim());
  started.push(agent);
  parent.stdout.destroy();
  state(dir, { running: { you: await recorded(parent.pid) } });
  const out = run("stop");
  assert.equal(out.status, 0, out.stderr);
  assert.match(out.stdout, /Nothing is left running/);
  assert.ok(await gone(parent.pid), "the rig's process");
  assert.ok(await gone(agent), "what it started, in a group of its own");
});

test("rig_14_stop_touches_nothing_that_only_names_the_rig", async (t) => {
  const { top, dir, proc, run } = aScratchMac(t);
  const someone = proc(join(dir, "rig.json")); // a person's tail, a session's Bash call
  const sibling = proc(join(`${dir}-other-session`, "x"));
  await recorded(someone);
  await recorded(sibling);
  state(dir, {});
  assert.equal(run("stop").status, 0);
  await sleep(200);
  assert.ok(up(someone), "a process that only names the rig's folder");
  assert.ok(up(sibling), "another session's sibling folder");
  // The very shell that called it, whose command line names the folder.
  const shell = spawnSync("/bin/sh", ["-c", `OBY_RIG_DIR="${dir}" HOME="${top}/home" "${process.execPath}" "${rig}" stop; echo survived`], { encoding: "utf8" });
  assert.match(shell.stdout, /survived/);
});

test("rig_15_a_remembered_pid_that_isnt_the_rigs_is_never_signalled", async (t) => {
  const { dir, proc, run } = aScratchMac(t);
  // Its pid is someone's by now: started at another time; and started then, but naming nothing of the rig's.
  const reused = proc();
  const stranger = proc(join(`${dir}-other-session`, "x"));
  const named = await recorded(stranger);
  state(dir, { running: { app: { pid: reused, lstart: "Thu Jan  1 00:00:00 2026" }, dana: named } });
  const out = run("stop");
  assert.equal(out.status, 0, out.stderr);
  await sleep(200);
  assert.ok(up(reused), "a pid started at another time");
  assert.ok(up(stranger), "a pid naming nothing of the rig's");
  const after = JSON.parse(readFileSync(join(dir, "rig.json"), "utf8"));
  assert.deepEqual(after.running, {}, "they aren't remembered any more");
});

test("rig_16_a_link_among_the_rigs_folders_is_refused_and_the_real_oby_stays", (t) => {
  const { dir, oby, run } = aScratchMac(t);
  state(dir, {});
  for (const name of ["you", "you/home", "dana", "dana/home", "sphere", "fixture", "obi-sphere"]) {
    rmSync(join(dir, "dana"), { recursive: true, force: true });
    rmSync(join(dir, "you"), { recursive: true, force: true });
    mkdirSync(dirname(join(dir, name)), { recursive: true });
    rmSync(join(dir, name), { recursive: true, force: true });
    symlinkSync(oby, join(dir, name));
    for (const command of ["reset", "stop", "start"]) {
      const out = run(command);
      assert.equal(out.status, 2, `${command} with ${name} a link: ${out.stdout}${out.stderr}`);
      assert.match(out.stderr, /^Refused: /);
    }
    assert.deepEqual(readdirSync(oby).sort(), ["Runner", "oby.sqlite"], name);
    rmSync(join(dir, name));
  }
});

test("rig_16_a_link_to_anywhere_else_is_refused_too", (t) => {
  // Not only the real Oby: a reset deletes through it, whatever it points at.
  const { top, dir, run } = aScratchMac(t);
  state(dir, {});
  const theirs = join(top, "someones-work");
  mkdirSync(join(theirs, "project"), { recursive: true });
  mkdirSync(join(dir, "dana"));
  symlinkSync(theirs, join(dir, "dana/home"));
  const out = run("reset");
  assert.equal(out.status, 2, out.stdout + out.stderr);
  assert.match(out.stderr, /is a link/);
  assert.deepEqual(readdirSync(theirs), ["project"]);
});

test("rig_23_a_wrong_obid_bin_says_so_with_nothing_up", (t) => {
  const { dir, run } = aScratchMac(t);
  const out = run("start");
  assert.equal(out.status, 1, out.stdout);
  assert.match(out.stderr, /OBID_BIN names .*no-obid, and nothing is there/);
  assert.deepEqual(readdirSync(dir).sort(), ["rig.json"], "nothing cloned, built or started");
  assert.ok(!JSON.parse(readFileSync(join(dir, "rig.json"), "utf8")).running);
});

test("rig_16_a_link_further_in_stops_start_before_it_writes_through_it", (t) => {
  const { top, dir, run } = aScratchMac(t);
  const theirs = join(top, "their-folder");
  mkdirSync(theirs);
  writeFileSync(join(theirs, ".credentials.json"), "their login");
  for (const name of ["dana/home/accounts/stand-in", "dana/gh"]) {
    rmSync(join(dir, "dana"), { recursive: true, force: true });
    state(dir, {});
    mkdirSync(dirname(join(dir, name)), { recursive: true });
    symlinkSync(theirs, join(dir, name));
    const out = run("start");
    assert.equal(out.status, 2, `${name}: ${out.stdout}${out.stderr}`);
    assert.match(out.stderr, /is a link/, name);
    assert.deepEqual(readdirSync(theirs), [".credentials.json"], name);
  }
});

test("rig_16_a_link_swapped_in_while_reset_stops_is_refused_too", async (t) => {
  const { top, dir, run, started } = aScratchMac(t);
  const theirs = join(top, "their-folder");
  mkdirSync(join(theirs, "project"), { recursive: true });
  mkdirSync(join(dir, "dana/home"), { recursive: true });
  // What's stopped swaps the link in as it goes: after the check every command starts with.
  const swap = `const fs = require("node:fs"); process.on("SIGTERM", () => { fs.rmSync(${JSON.stringify(join(dir, "dana/home"))}, { recursive: true }); fs.symlinkSync(${JSON.stringify(theirs)}, ${JSON.stringify(join(dir, "dana/home"))}); process.exit(0); }); console.log("ready"); ${FOREVER}`;
  const child = spawn(process.execPath, ["-e", swap, join(dir, "you/x")], { detached: true, stdio: ["ignore", "pipe", "ignore"] });
  child.unref();
  started.push(child.pid);
  await once(child.stdout, "data");
  child.stdout.destroy();
  state(dir, { running: { you: await recorded(child.pid) } });
  const out = run("reset");
  assert.equal(out.status, 2, out.stdout + out.stderr);
  assert.match(out.stderr, /is a link/);
  assert.deepEqual(readdirSync(theirs), ["project"], "not deleted through it");
});

test("rig_15_start_times_are_in_one_time_zone", { skip: process.platform === "win32" }, () => {
  // As ps says it with TZ=UTC: a Mac moving to another zone changes nothing remembered.
  const utc = spawnSync("ps", ["-o", "lstart=", "-p", String(process.pid)], { encoding: "utf8", env: { PATH: process.env.PATH, LC_ALL: "C", TZ: "UTC" } }).stdout.trim();
  assert.equal(table().find((p) => p.pid === process.pid).lstart, utc);
});

test("rig_16_reset_removes_a_link_further_in_and_not_what_it_points_at", (t) => {
  const { dir, oby, run } = aScratchMac(t);
  state(dir, { danaAccount: "88a49151" });
  mkdirSync(join(dir, "dana/home/accounts/88a49151"), { recursive: true });
  writeFileSync(join(dir, "dana/home/accounts.json"), "{}");
  symlinkSync(oby, join(dir, "dana/home/jobs"));
  const out = run("reset");
  assert.equal(out.status, 0, out.stderr);
  assert.deepEqual(readdirSync(oby).sort(), ["Runner", "oby.sqlite"]);
  assert.deepEqual(readdirSync(join(dir, "dana/home")).sort(), ["accounts", "accounts.json"], "Dana's account stays");
  assert.deepEqual(JSON.parse(readFileSync(join(dir, "rig.json"), "utf8")), { danaAccount: "88a49151" });
});

test("rig_13_every_command_refuses_a_folder_that_isnt_the_rigs", (t) => {
  const { dir, run } = aScratchMac(t);
  mkdirSync(join(dir, "someones-work"));
  writeFileSync(join(dir, "fixture"), "theirs");
  for (const command of ["start", "stop", "reset", "status", "app"]) {
    assert.equal(run(command).status, 2, command);
    assert.deepEqual(readdirSync(dir).sort(), ["fixture", "someones-work"], command);
  }
});

test("rig_17_a_start_cut_short_never_says_its_running", async (t) => {
  const { dir, proc, run } = aScratchMac(t);
  const running = {};
  for (const name of ["sphere", "you", "dana"]) running[name] = await recorded(proc(join(dir, `${name}/x`)));
  state(dir, { running, ready: false });
  const out = run("start");
  assert.equal(out.status, 1, out.stdout);
  assert.match(out.stderr, /A start was cut short/);
  assert.doesNotMatch(out.stdout, /running already/);
  assert.match(run("status").stdout, /A start was cut short/);
});

test("rig_18_another_checkouts_rig_isnt_stopped_from_here_unless_forced", async (t) => {
  const { top, dir, proc, run } = aScratchMac(t);
  const you = await recorded(proc(join(dir, "you/x")));
  state(dir, { running: { you }, ready: true, checkout: top });
  for (const command of ["stop", "reset", "job", "review", "policy", "dana", "app", "ask"]) {
    const out = run(command, "x");
    assert.equal(out.status, 1, command);
    assert.match(out.stderr, /The rig is up from/, command);
  }
  assert.ok(up(you.pid));
  assert.equal(run("stop", "--force").status, 0);
  assert.ok(await gone(you.pid));
  // With nothing up, its checkout doesn't matter, and one that's gone asks nothing.
  assert.equal(run("stop").status, 0);
});

test("rig_19_one_command_at_a_time_and_a_lock_nobody_holds_is_nothing", async (t) => {
  const { dir, proc, run } = aScratchMac(t);
  state(dir, {});
  const holder = await recorded(proc());
  writeFileSync(join(dir, "rig.lock"), JSON.stringify(holder));
  const out = run("stop");
  assert.equal(out.status, 1);
  assert.match(out.stderr, new RegExp(`Another rig command is running \\(pid ${holder.pid}\\)`));
  assert.ok(up(holder.pid));
  writeFileSync(join(dir, "rig.lock"), JSON.stringify({ pid: holder.pid, lstart: "Thu Jan  1 00:00:00 2026" }));
  assert.equal(run("stop").status, 0);
  assert.ok(!existsSync(join(dir, "rig.lock")), "it goes when the command ends");
});

test("rig_20_a_rig_json_that_doesnt_read_stops_nothing_from_working", (t) => {
  const { dir, run } = aScratchMac(t);
  writeFileSync(join(dir, "rig.json"), '{"running": {"you"');
  const status = run("status");
  assert.equal(status.status, 1);
  assert.match(status.stderr, /doesn't read: node e2e\/rig.mjs reset/);
  const stop = run("stop");
  assert.equal(stop.status, 0, stop.stderr);
  assert.match(stop.stdout, /isn't known/);
  writeFileSync(join(dir, "rig.json"), '{"running": {"you"');
  assert.equal(run("reset").status, 0);
  assert.deepEqual(JSON.parse(readFileSync(join(dir, "rig.json"), "utf8")), {});
});

test("rig_21_stop_and_reset_on_nothing_say_so", (t) => {
  const { dir, run } = aScratchMac(t);
  rmSync(dir, { recursive: true });
  for (const command of ["stop", "reset"]) {
    assert.equal(run(command).status, 0, command);
    assert.ok(!existsSync(dir), command);
  }
});

test("rig_15_ours_goes_by_when_it_started_and_where_it_points", () => {
  const dir = "/x/rig";
  const row = (pid, pgid, lstart, command, ppid = 1) => ({ pid, ppid, pgid, stat: "S", lstart, command });
  const then = "Tue Nov  3 16:00:00 2026", later = "Tue Nov  3 17:00:00 2026";
  const remembered = { pid: 10, lstart: then };
  assert.ok(ours(remembered, dir, [row(10, 10, then, "obid --config /x/rig/you/config.json")]));
  assert.ok(!ours(remembered, dir, [row(10, 10, later, "obid --config /x/rig/you/config.json")]), "another start");
  assert.ok(!ours(remembered, dir, [row(10, 10, then, "sleep 300")]), "another command");
  assert.ok(!ours(remembered, dir, [row(10, 10, then, "obid --config /x/rig-other/you/config.json")]), "a sibling folder");
  assert.ok(ours(remembered, dir, [row(11, 10, later, "workerd /x/rig/obi-sphere/server")]), "its group, its leader gone");
  assert.ok(!ours(remembered, dir, [row(11, 10, later, "sleep 300")]), "someone's group");
  assert.ok(!ours(remembered, dir, []), "nothing there");
  assert.ok(!ours(undefined, dir, []));
});

test("rig_9_family_finds_children_by_parent_and_group_and_never_who_called", () => {
  const row = (pid, ppid, pgid) => ({ pid, ppid, pgid, stat: "S", lstart: `t${pid}`, command: "" });
  const rows = [row(1, 0, 1), row(50, 1, 50), row(60, 50, 60), row(70, 60, 10), row(10, 60, 10), row(20, 10, 20), row(21, 20, 21), row(30, 1, 30), row(31, 30, 31), row(40, 1, 40)];
  const found = family([10], 60, rows).map((r) => r.pid).sort((a, b) => a - b);
  assert.deepEqual(found, [10, 20, 21, 70], "a child, a grandchild in another group, a group member; not its callers 60, 50 and 1, nor 30, 31, 40");
  const fromBelow = family([60], 10, rows).map((r) => r.pid);
  for (const caller of [10, 60, 50, 1]) assert.ok(!fromBelow.includes(caller), `the caller's own chain: ${caller}`);
});

test("rig_15_stillUp_goes_by_when_it_started", () => {
  const rows = [{ pid: 5, ppid: 1, pgid: 5, stat: "S", lstart: "now", command: "" }];
  assert.deepEqual(stillUp([{ pid: 5, lstart: "now" }, { pid: 5, lstart: "then" }, { pid: 6, lstart: "now" }], rows), [{ pid: 5, lstart: "now" }]);
});

test("rig_6_ask_finds_only_the_exchange_this_question_made", () => {
  const exchange = (correlationId, text) => ({ correlationId, state: "done", messages: [{ envelope: { body: { text } } }] });
  const earlier = exchange("a", "May I?");
  // The same words, once answered: not sent this time, so there's no exchange to tell of.
  assert.equal(newExchange([earlier], [earlier], "May I?"), undefined);
  const again = exchange("b", "May I?");
  assert.equal(newExchange([earlier], [again, earlier], "May I?"), again);
  assert.equal(newExchange([], [exchange("c", "Other words")], "May I?"), undefined);
});

// ---- review (RIG-25), against stand-ins of the two obids' APIs

const REQUEST = { id: "i1", kind: "obiRequest", title: "Review request from dana", state: "open", obi: { correlationId: "x1" } };
const RESULT = JSON.parse(readFileSync(join(dirname(rig), "fixtures/messages/result.example.json"), "utf8")).body;
const ended = (body, state = "done") => ({ state, messages: [
  { direction: "sent", envelope: { type: "review_request" } }, { direction: "received", envelope: { type: "accept" } },
  ...(body ? [{ direction: "received", envelope: { type: "result", body } }] : [])] });

// Dana's obid takes the request and tells its exchange as `ends`; yours lists the request, or `card`
// once you've answered, and answers each POST as `answers` says.
function twoObids({ ends = ended(RESULT), answers = [{ status: 200, text: "{}" }], card } = {}) {
  const calls = [];
  const answered = () => calls.some((c) => c.who === "you");
  const you = {
    get: async (path) => (path === "/v1/obid/sphere" ? { obiId: "obi-alex" } : path === "/v1/inbox" ? [answered() && card ? card : REQUEST] : assert.fail(`GET ${path} on you`)),
    call: async (method, path, body, headers = {}) => { calls.push({ who: "you", method, path, body, headers }); return answers.shift(); },
  };
  const dana = {
    get: async (path) => { assert.equal(path, "/v1/obid/sphere/exchanges/x1"); return ends; },
    call: async (method, path, body) => { calls.push({ who: "dana", method, path, body }); return { status: 202, text: JSON.stringify({ correlationId: "x1", state: "sent" }) }; },
  };
  return { you, dana, calls };
}
function aDevice() {
  const { privateKey, publicKey } = generateKeyPairSync("ec", { namedCurve: "prime256v1" });
  return { id: "d1", key: privateKey.export({ type: "pkcs8", format: "pem" }), publicKey };
}
const quick = async (what, probe) => { for (let i = 0; i < 5; i++) { const got = await probe(); if (got) return got; } throw new Error(`${what}: nothing`); };
const reviewing = (obids, device, hold = false, lines = []) => review({ ...obids, device, body: reviewBody(), hold, until: quick, say: (line) => lines.push(line) });

test("rig_25_the_spheres_rule_lets_alex_and_dana_ask_each_other_for_reviews", () => {
  for (const [from, to] of [["alex", "dana"], ["dana", "alex"]]) {
    const rule = RULES.find((r) => r.from_owner === from && r.to_owner === to);
    assert.deepEqual(rule?.actions, ["question", "review_request"], `${from} to ${to}`);
  }
});

test("rig_25_review_sends_the_fixtures_request_and_accepts_with_your_words_signed", async () => {
  const obids = twoObids(), device = aDevice(), lines = [];
  assert.equal(await reviewing(obids, device, false, lines), "approved");
  const [request, answer, ...more] = obids.calls;
  assert.deepEqual(more, []);
  assert.deepEqual({ who: request.who, method: request.method, path: request.path }, { who: "dana", method: "POST", path: "/v1/obid/sphere/requests" });
  assert.deepEqual(request.body, { to: "obi-alex", type: "review_request", body: reviewBody(), expiresIn: REVIEW_SECONDS });
  assert.equal(request.body.body.head_branch, "feature/average");
  assert.equal(request.body.body.head_sha, FIXTURE.headSha, "the head buildFixture puts in fixture/remote.git");
  assert.match(request.body.body.summary, /OBI-E2E-DATA-CANARY/, "the canary the stub agent checks");
  assert.deepEqual({ who: answer.who, method: answer.method, path: answer.path, body: answer.body }, { who: "you", method: "POST", path: "/v1/inbox/i1/answer", body: { answer: "Accept" } });
  const [id, signedAt, signature] = answer.headers["x-oby-words"].split(" ");
  assert.equal(id, "d1");
  const digest = createHash("sha256").update(JSON.stringify(answer.body)).digest("hex");
  assert.ok(verify("sha256", Buffer.from(`oby-words\nPOST\n/v1/inbox/i1/answer\n${digest}\n${signedAt}`), device.publicKey, Buffer.from(signature, "base64")), "signed as the app signs the owner's words");
  assert.ok(lines.includes("Outcome: approved. " + RESULT.summary), lines.join("\n"));
  for (const f of RESULT.findings) assert.ok(lines.some((l) => l.includes(f.title)), f.title);
});

test("rig_25_review_hold_answers_nothing", async () => {
  const obids = twoObids();
  assert.equal(await reviewing(obids, aDevice(), true), "approved");
  assert.deepEqual(obids.calls.map((c) => c.who), ["dana"], "only Dana's request: your answer is yours, in the app");
});

test("rig_25_a_confirm_obid_asks_for_is_signed_by_the_rigs_device", async () => {
  const card = { id: "i1", kind: "confirm", title: "Approve dana's pull request on GitHub, as you", state: "open" };
  const obids = twoObids({ card, answers: [{ status: 403, text: '{"error": "Only your tap can confirm this"}' }, { status: 200, text: "{}" }] });
  const device = aDevice();
  assert.equal(await reviewing(obids, device), "approved");
  const confirmed = obids.calls[2];
  assert.equal(confirmed.path, "/v1/inbox/i1/answer");
  const { answer, approval } = confirmed.body;
  assert.equal(answer, "Confirm");
  assert.equal(approval.device, "d1");
  assert.equal(approval.action, card.title);
  assert.ok(verify("sha256", Buffer.from(`oby-confirm\ni1\n${card.title}\n${approval.signedAt}`), device.publicKey, Buffer.from(approval.signature, "base64")), "signed as the app signs a Confirm");
});

test("rig_25_review_fails_unless_approved", async () => {
  const device = aDevice();
  await assert.rejects(reviewing(twoObids({ ends: ended({ ...RESULT, outcome: "changes_requested" }) }), device), /came back changes_requested, not approved/);
  await assert.rejects(reviewing(twoObids({ ends: ended(null, "declined") }), device), /came back declined, not approved/);
  await assert.rejects(reviewing(twoObids({ answers: [{ status: 409, text: "gone" }] }), device), /Your Accept: 409 gone/);
});

test("rig_25_the_stand_in_reviews_with_make_e2es_reviewer", (t) => {
  const worktree = mkdtempSync(join(tmpdir(), "oby-rig-review-"));
  t.after(() => rmSync(worktree, { recursive: true, force: true }));
  mkdirSync(join(worktree, ".obi/inputs"), { recursive: true });
  writeFileSync(join(worktree, ".obi/inputs/obi-request.json"), "{}");
  const out = spawnSync(join(dirname(rig), "rig-agent"), ["--version"], { cwd: worktree, encoding: "utf8", input: "", timeout: 10_000 });
  assert.equal(out.status, 0, out.stderr);
  assert.match(out.stdout, /stub-agent/);
});
