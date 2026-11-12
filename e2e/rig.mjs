#!/usr/bin/env node
// Two Obys and a Sphere on this Mac, for testing what talking to a person's Oby does on the real
// thing (docs/contracts/test-rig.md): a local Sphere from a scratch clone of obi-sphere at the commit
// in e2e/rig-sphere.ref, two scratch obids, you (owner alex) and Dana (a test teammate, owner dana), and Dana's long job. The
// owner's real Oby, their logins, GitHub and the live Sphere are never used.
//
//   node e2e/rig.mjs start [--stand-in | --real]   everything up, joined, the rule written, Dana's job running
//   node e2e/rig.mjs status                        what's up, in a few lines
//   node e2e/rig.mjs ask "What are you working on?"  you ask Dana's Oby; prints what came back
//   node e2e/rig.mjs review [--hold]               Dana's Oby asks yours for a review; you Accept, unless --hold
//   node e2e/rig.mjs app                           the Electron dev app, on you
//   node e2e/rig.mjs job [task]                    one more job for Dana
//   node e2e/rig.mjs job --personal [task]         one outside what Dana shares (PEER-15)
//   node e2e/rig.mjs policy on|off                 the Sphere's rule written, or none (PEER-8)
//   node e2e/rig.mjs dana stop|start               Dana's obid only (a Mac that's off, PEER-7)
//   node e2e/rig.mjs stop [--force]                nothing left running; the folder stays
//   node e2e/rig.mjs reset [--force]               stop, and start over (Dana's sign-in and the clone stay)
//
// Environment: OBY_RIG_DIR, the folder (default ~/Library/Application Support/oby-rig); OBID_BIN,
// obid (default: this checkout's, built first).
//
// Dana answers with the real Claude Code once the owner signed in the account start prints the
// command for; until then, or with --stand-in, stand-ins (e2e/rig-agent) give exact states. You
// are always on stand-ins: asking runs no model of yours.

import { execFileSync, spawn, spawnSync } from "node:child_process";
import { createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes } from "node:crypto";
import { closeSync, existsSync, mkdirSync, openSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { homedir, userInfo } from "node:os";
import { dirname, join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { buildFixture } from "./lib/fixture.mjs";
import { requireNodeVersion } from "./lib/node-version.mjs";
import { stopGroups } from "./lib/processes.mjs";
import { aLink, rigRefusal } from "./lib/rig-guard.mjs";
import { confirmOf, review as reviewOf, reviewBody, RULES, wordsOf } from "./lib/rig-review.mjs";
import { family, newExchange, orphans, ours, record, stillUp, table } from "./lib/rig-state.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const RIG = resolve(process.env.OBY_RIG_DIR || join(homedir(), "Library/Application Support/oby-rig"));
const AGENTS = join(ROOT, "core/crates/obid/tests/agents");
const SPHERE_REMOTE = "https://github.com/obi-ai-bot/obi-sphere.git";
const SPHERE_PORTS = [8791, 8792, 8793]; // 8799 is often another session's
const PEOPLE = { you: { owner: "alex", name: "Alex’s Oby", port: 8794 }, dana: { owner: "dana", name: "Dana’s Oby", port: 8795 } };
const REPO = "obi-e2e/fixture-app";
// A repository of its owner's that Dana's yes leaves out (PEER-15): the same remote is enough.
const PERSONAL = "obi-e2e/personal-taxes";
const LONG_TASK = "Add one small test file a minute, 30 times, committing each: tests/step-01.test.js, then tests/step-02.test.js and so on, with `sleep 60` between them. Each file holds one passing node:test. Nothing else.";

// This comes first, before anything is read, made or started.
const refused = rigRefusal(RIG);
if (refused) {
  console.error(`Refused: ${refused}`);
  process.exit(2);
}

const STATE = join(RIG, "rig.json");
const LOCK = join(RIG, "rig.lock");
// `anyway`: for what must work without it (stop, reset): what it held is lost, and they say so.
function load({ anyway = false } = {}) {
  if (!existsSync(STATE)) return {};
  try { return JSON.parse(readFileSync(STATE, "utf8")); }
  catch {
    if (anyway) { say(`${STATE} doesn't read, so what's running from it isn't known.`); return {}; }
    return fail(`${STATE} doesn't read: node e2e/rig.mjs reset starts over.`);
  }
}
function save(state) {
  writeFileSync(`${STATE}.new`, JSON.stringify(state, null, 2), { mode: 0o600 });
  renameSync(`${STATE}.new`, STATE);
}
const dirOf = (who) => join(RIG, who);
const homeOf = (who) => join(dirOf(who), "home");
const say = (line) => console.log(line);
const fail = (line) => { throw new Error(line); };

async function until(what, probe, seconds = 60) {
  const end = Date.now() + seconds * 1000;
  let last;
  while (Date.now() < end) {
    try { const got = await probe(); if (got) return got; } catch (e) { last = e; }
    await sleep(500);
  }
  fail(`${what}: nothing within ${seconds} s${last ? ` (${last.message})` : ""}`);
}

const free = (port) => new Promise((done) => {
  const server = createServer();
  server.once("error", () => done(false));
  server.listen(port, "127.0.0.1", () => server.close(() => done(true)));
});

// What a tool the rig runs for itself gets of the person's environment, and no more.
const plain = () => Object.fromEntries(["PATH", "HOME", "USER", "LOGNAME", "TMPDIR", "LANG"].filter((k) => process.env[k]).map((k) => [k, process.env[k]]));

// A process of the rig's: its own group, its output in a log, outliving this command. Remembered
// with when it started, so a pid that's someone else's by now is never signalled.
function launch(state, name, log, cmd, args, opts) {
  const out = openSync(join(RIG, `${log}.log`), "a");
  const child = spawn(cmd, args, { ...opts, detached: true, stdio: ["ignore", out, out] });
  child.on("error", () => {});
  closeSync(out);
  if (!child.pid) fail(`${cmd} didn't start: is it there? Its log: ${join(RIG, `${log}.log`)}`);
  child.unref();
  state.running = { ...state.running, [name]: record(child.pid) ?? fail(`${cmd} ended at once. Its log: ${join(RIG, `${log}.log`)}`) };
  save(state);
}
const alive = (state, name) => ours(state.running?.[name], RIG);

// ---- obid

function api(who) {
  const base = `http://127.0.0.1:${PEOPLE[who].port}`;
  const headers = () => ({ authorization: `Bearer ${readFileSync(join(homeOf(who), "api-token"), "utf8").trim()}`, "content-type": "application/json" });
  // What shared/sphere.ts calls obid with: status 0 when it isn't there.
  const call = async (method, path, body, extra = {}) => {
    try {
      const res = await fetch(base + path, { method, headers: { ...headers(), ...extra }, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(60_000) });
      return { status: res.status, text: await res.text() };
    } catch (e) { return { status: 0, text: e.message }; }
  };
  const get = async (path) => {
    const r = await call("GET", path);
    return r.status === 200 ? JSON.parse(r.text) : fail(`GET ${path} on ${who}: ${r.status} ${r.text}`);
  };
  return { call, get };
}

// By the login shell the password database names (AGENTS.md, "PATH"): $SHELL can differ.
const claude = () => {
  const found = spawnSync(userInfo().shell, ["-l", "-c", "command -v claude"], { encoding: "utf8", env: plain(), timeout: 15_000 }).stdout?.trim().split("\n").pop();
  return found && existsSync(found) ? found : null;
};
// The tool itself says whether the account's folder has a login: Keychain or file, its own business.
function signedIn(bin, folder) {
  const out = spawnSync(bin, ["auth", "status", "--json"], { encoding: "utf8", env: { ...plain(), CLAUDE_CONFIG_DIR: folder }, timeout: 15_000 });
  try { return JSON.parse(out.stdout).loggedIn === true; } catch { return false; }
}

// Every tool names an account of the rig's, so a first start lists no login found on this Mac
// (docs/runner-api.md, "Logins already on the Mac"): none of the owner's reaches a scratch obid.
function writeConfig(who, { bin, real }) {
  const home = homeOf(who);
  const own = (id) => [{ id, name: id, configDir: join(home, "accounts", id) }];
  for (const id of ["stand-in", "no-codex", "no-cursor"]) mkdirSync(join(home, "accounts", id), { recursive: true, mode: 0o700 });
  const login = join(home, "accounts/stand-in/.credentials.json");
  if (real) rmSync(login, { force: true });
  else writeFileSync(login, JSON.stringify({ claudeAiOauth: { accessToken: "rig-stand-in", expiresAt: Date.now() + 86_400_000 * 365 } }), { mode: 0o600 });
  writeFileSync(join(dirOf(who), "config.json"), JSON.stringify({
    listen: `127.0.0.1:${PEOPLE[who].port}`, home, owner: PEOPLE[who].owner, telemetry: false, allowFileRemotes: true,
    claudeKeychain: real, cursorKeychain: false,
    agents: { claude: { bin, accounts: own("stand-in") }, codex: { bin: "/usr/bin/false", accounts: own("no-codex") }, cursor: { bin: "/usr/bin/false", accounts: own("no-cursor") } },
    conversation: { agent: "claude" },
    repos: [REPO, PERSONAL].map((repo) => ({ repo, remote: `file://${join(RIG, "fixture/remote.git")}`, defaultBranch: "main", check: "true", pullRequests: false })),
  }, null, 2), { mode: 0o600 });
}

function startObid(state, who, { real }) {
  const dir = dirOf(who);
  for (const d of ["env-home", "tmp", "gh"]) mkdirSync(join(dir, d), { recursive: true, mode: 0o700 });
  // A scratch HOME and a stand-in Keychain, unless Dana's real login is what's being run: macOS
  // finds the login Keychain by HOME. Either way no GitHub login: an empty gh folder.
  launch(state, who, `${who}-obid`, state.obid, ["serve", "--config", join(dir, "config.json")], { env: {
    PATH: `${AGENTS}:${dirname(process.execPath)}:${process.env.PATH}`, USER: process.env.USER, LOGNAME: process.env.LOGNAME,
    HOME: real ? homedir() : join(dir, "env-home"), TMPDIR: join(dir, "tmp"), GH_CONFIG_DIR: join(dir, "gh"),
    ...(real ? {} : { OBID_SECURITY_BIN: join(AGENTS, "fake-security") }),
    OBID_PRESTART: "off", OBID_USAGE_READS: "off", OBID_LOAD: "0", OBID_MEMORY_AVAILABLE: "1099511627776", // other sessions' load doesn't hold a job back
  } });
}
const answering = (who) => until(`${who}'s obid (its log: ${join(RIG, `${who}-obid.log`)})`, async () => (await api(who).call("GET", "/v1/status")).status === 200);

// A device of the rig's, as the app registers its own (main/device.ts): what signs the owner's words.
async function deviceOf(who, state) {
  if (state.devices?.[who]) return;
  const key = generateKeyPairSync("ec", { namedCurve: "prime256v1" }).privateKey;
  const { x, y } = createPublicKey(key).export({ format: "jwk" });
  const publicKey = Buffer.concat([Buffer.from([4]), Buffer.from(x, "base64url"), Buffer.from(y, "base64url")]).toString("base64");
  const pairing = join(homeOf(who), "pairing");
  writeFileSync(pairing, publicKey, { mode: 0o600 });
  const r = await api(who).call("POST", "/v1/devices", { name: "The rig (test)", publicKey });
  rmSync(pairing, { force: true });
  if (r.status >= 300) fail(`${who}'s device: ${r.status} ${r.text}`);
  state.devices = { ...state.devices, [who]: { id: JSON.parse(r.text).id, key: key.export({ type: "pkcs8", format: "pem" }) } };
  save(state);
}

// Dana's ticks and click on Confirm on the card that lists a job's step and report line
// (docs/contracts/peer-thread.md, PEER-38), once, so that what Dana's Oby answers is real progress in
// the rig's repository and Dana's inbox stays empty.
async function danaSaysYes(state) {
  if (state.danaSaidYes) return;
  const opened = await api("dana").call("POST", "/v1/obid/sharing/offer", {});
  if (opened.status >= 300) fail(`Dana's card: ${opened.status} ${opened.text}`);
  const offered = await api("dana").call("POST", "/v1/obid/sharing/offer", { repos: [REPO] });
  if (offered.status >= 300) fail(`Dana's card: ${offered.status} ${offered.text}`);
  const item = JSON.parse(offered.text);
  const yes = await api("dana").call("POST", `/v1/inbox/${encodeURIComponent(item.id)}/answer`, confirmOf(state.devices.dana, item));
  if (yes.status >= 300) fail(`Dana's yes: ${yes.status} ${yes.text}`);
  state.danaSaidYes = true;
  save(state);
}

// ---- the Sphere

function sphereCall(state) {
  return async (path, { method = "GET", token, body, form } = {}) => {
    const r = await fetch(`http://127.0.0.1:${state.spherePort}/v0.1${path}`, { method, redirect: "error",
      headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), ...(body ? { "content-type": "application/json" } : form ? { "content-type": "application/x-www-form-urlencoded" } : {}) },
      body: body ? JSON.stringify(body) : form ? new URLSearchParams(form) : undefined });
    const text = await r.text();
    return { status: r.status, body: text ? JSON.parse(text) : null };
  };
}

// The clone at the commit e2e/rig-sphere.ref names, so a result says what it ran on. Its marker is
// written last: whatever was cut short is done again.
function cloneSphere() {
  const pin = readFileSync(join(ROOT, "e2e/rig-sphere.ref"), "utf8").trim();
  const clone = join(RIG, "obi-sphere"), ready = join(RIG, "obi-sphere.ready");
  if (!existsSync(ready) || readFileSync(ready, "utf8").trim() !== pin) {
    rmSync(ready, { force: true });
    rmSync(clone, { recursive: true, force: true });
    mkdirSync(clone, { recursive: true });
    say(`Cloning obi-sphere at ${pin.slice(0, 7)} and installing its dependencies…`);
    const git = (...args) => execFileSync("git", ["-C", clone, ...args], { stdio: "inherit", env: plain() });
    git("init", "-q");
    git("fetch", "-q", "--depth", "1", SPHERE_REMOTE, pin);
    git("-c", "advice.detachedHead=false", "checkout", "-q", "FETCH_HEAD");
    execFileSync("pnpm", ["install", "--frozen-lockfile", "--silent"], { cwd: join(clone, "server"), stdio: "inherit", env: plain() });
    writeFileSync(ready, `${pin}\n`);
  }
  return pin.slice(0, 7);
}

function startSphere(state) {
  const dir = join(RIG, "sphere");
  for (const d of ["state", "home", "tmp"]) mkdirSync(join(dir, d), { recursive: true, mode: 0o700 });
  launch(state, "sphere", "sphere", process.execPath, ["scripts/local.mjs", "--port", String(state.spherePort), "--persist-to", join(dir, "state")], {
    cwd: join(RIG, "obi-sphere/server"),
    env: { SPHERE_OPERATOR_TOKEN: state.operator, HOME: join(dir, "home"), TMPDIR: join(dir, "tmp"), PATH: process.env.PATH, WRANGLER_SEND_METRICS: "false" },
  });
}

const adminKey = (state) => createPrivateKey(state.adminKey);
const adminToken = async (state, spec) => {
  const r = await sphereCall(state)("/oauth/token", { method: "POST", form: {
    grant_type: "urn:ietf:params:oauth:grant-type:token-exchange", subject_token_type: "urn:ietf:params:oauth:token-type:jwt",
    subject_token: spec.assertion(adminKey(state), "rig-admin", "rig", "admin directory:read audit:read") } });
  return r.body?.access_token ?? fail(`the admin's token: ${r.status} ${JSON.stringify(r.body)}`);
};
async function setPolicy(state, spec, rules) {
  const call = sphereCall(state), token = await adminToken(state, spec);
  const now = (await call("/spheres/rig/policy", { token })).body;
  const r = await call("/spheres/rig/policy", { method: "PUT", token, body: { expected_version: now.version, rules } });
  if (r.status !== 200) fail(`the policy: ${r.status} ${JSON.stringify(r.body)}`);
  return r.body;
}

// The Sphere "rig", once: its admin and the private link both Obys join by. A start cut short goes
// on from what it kept.
async function makeSphere(state, spec) {
  if (state.link) return;
  const call = sphereCall(state);
  state.adminKey ??= generateKeyPairSync("ed25519").privateKey.export({ type: "pkcs8", format: "pem" });
  save(state);
  const made = await call("/spheres", { method: "POST", token: state.operator, body: { sphere: "rig", name: "The rig", admin: { owner: "rig-admin", owner_key: spec.publicOf(adminKey(state)) } } });
  if (made.status !== 201 && made.status !== 409) fail(`the Sphere: ${made.status} ${JSON.stringify(made.body)}`);
  const invitation = await call("/spheres/rig/invitations", { method: "POST", token: await adminToken(state, spec), body: { expires_in: 2_592_000, max_uses: 10 } });
  if (invitation.status !== 201) fail(`the link: ${invitation.status} ${JSON.stringify(invitation.body)}`);
  state.link = `http://127.0.0.1:${state.spherePort}/join#sphere=rig&name=The%20rig&by=rig-admin&id=${invitation.body.invitation_id}&secret=${invitation.body.secret}`;
  save(state);
}

async function joinBoth(state, spec) {
  for (const who of Object.keys(PEOPLE)) {
    const { call, get } = api(who);
    if ((await get("/v1/obid/sphere")).state !== "joined") {
      state.ownerKeys = { ...state.ownerKeys, [who]: state.ownerKeys?.[who] ?? generateKeyPairSync("ed25519").privateKey.export({ type: "pkcs8", format: "pem" }) };
      save(state);
      await spec.join(call, () => createPrivateKey(state.ownerKeys[who]), spec.parseInvite(state.link), PEOPLE[who].name);
    }
  }
  await until("sealing on, on both", async () => {
    for (const who of Object.keys(PEOPLE)) await spec.keepMembership(api(who).call, () => createPrivateKey(state.ownerKeys[who]));
    const both = await Promise.all(Object.keys(PEOPLE).map((who) => api(who).get("/v1/obid/sphere")));
    return both.every((m) => m.sealing === "on");
  }, 120);
}

// ---- Dana's account and job

async function danaAccount(state) {
  if (state.danaAccount) return state.danaAccount;
  const r = await api("dana").call("POST", "/v1/accounts", { agent: "claude", name: "Dana" });
  if (r.status >= 300) fail(`Dana's account: ${r.status} ${r.text}`);
  state.danaAccount = JSON.parse(r.text).id;
  save(state);
  return state.danaAccount;
}
const signInCommand = (folder) => `CLAUDE_CONFIG_DIR="${folder}" claude auth login`;

async function startJob(state, task, personal = false) {
  const job = { repo: personal ? PERSONAL : REPO, kind: "fix", title: personal ? "File my taxes" : "Billing retries", task: task ?? (state.real ? LONG_TASK : "SLEEP"), account: state.real ? state.danaAccount : "stand-in" };
  const r = await api("dana").call("POST", "/v1/jobs", { job });
  if (r.status !== 201) fail(`Dana's job: ${r.status} ${r.text}`);
  return JSON.parse(r.text);
}
const running = (job) => job.status === "running";

// ---- commands

// Dana's job, kept going: the job from before a stop takes a moment to go on, and a second beside
// it would be a lie. The real Claude Code with no login runs none: answers go to Dana.
async function keepDanaWorking(state) {
  if (state.real && !state.signed) return;
  const goesOn = state.danaJob && (await until("Dana's job going on", async () => running(await api("dana").get(`/v1/jobs/${state.danaJob}`)), 15).catch(() => false));
  if (goesOn) return;
  const job = await startJob(state);
  state.danaJob = job.id;
  save(state);
  await until("Dana's job running", async () => {
    const seen = await api("dana").get(`/v1/jobs/${job.id}`);
    if (["failed", "interrupted", "done"].includes(seen.status)) fail(`Dana's job ended ${seen.status}: ${seen.reason ?? seen.summary ?? ""}`);
    return running(seen);
  });
}

const ASKED_MODE = { "--stand-in": false, "--real": true };

async function start(flags) {
  for (const flag of flags) if (!(flag in ASKED_MODE)) fail(`What's ${flag}? node e2e/rig.mjs start [--stand-in | --real]`);
  if (flags.size > 1) fail("--stand-in or --real, not both.");
  const asked = flags.has("--stand-in") ? false : flags.has("--real") ? true : undefined;
  const state = load();
  const up = ["sphere", "you", "dana"].filter((name) => alive(state, name));
  if (up.length === 3 && state.ready) {
    if (asked !== undefined && asked !== Boolean(state.real)) fail(`The rig is up with Dana on ${state.real ? "the real Claude Code" : "stand-ins"}: stop it first.`);
    await keepDanaWorking(state);
    say("The rig is running already.");
    return status();
  }
  if (up.length) fail(`${state.ready ? "Only part of the rig is running" : "A start was cut short"} (${up.join(", ")} up): node e2e/rig.mjs stop, then start again.`);
  state.ready = false;
  state.checkout = ROOT;
  save(state);
  state.obid = theObid();
  for (const who of Object.keys(PEOPLE)) if (!(await free(PEOPLE[who].port))) fail(`Port ${PEOPLE[who].port}, ${who}'s obid's, is taken by something else.`);
  if (!state.spherePort) {
    for (const port of SPHERE_PORTS) if (!state.spherePort && (await free(port))) state.spherePort = port;
    if (!state.spherePort) fail(`Ports ${SPHERE_PORTS.join(", ")} are all taken: another session's Sphere?`);
    state.operator = randomBytes(32).toString("base64url");
    save(state);
  } else if (!(await free(state.spherePort))) fail(`Port ${state.spherePort}, the Sphere's, is taken by something else, and both Obys' memberships name it. node e2e/rig.mjs reset picks another.`);

  const spec = await import(join(ROOT, "apps/electron/src/shared/sphere.ts"));
  if (!existsSync(join(RIG, "fixture/remote.git"))) buildFixture(join(RIG, "fixture"));
  state.sphereCommit = cloneSphere();
  save(state);

  try {
    startSphere(state);
    await until(`the Sphere (its log: ${join(RIG, "sphere.log")})`, async () => (await sphereCall(state)("/health")).status === 200, 120);
    await makeSphere(state, spec);
    say(`Sphere: http://127.0.0.1:${state.spherePort} (obi-sphere ${state.sphereCommit})`);

    const standIn = join(ROOT, "e2e/rig-agent");
    writeConfig("you", { bin: standIn, real: false });
    startObid(state, "you", { real: false });

    // Stand-ins asked for: no need to run the real tool at all.
    const real = asked === false ? null : claude();
    const accountKnown = state.danaAccount ? join(homeOf("dana"), "accounts", state.danaAccount) : null;
    state.signed = Boolean(real && accountKnown && signedIn(real, accountKnown));
    const wasReal = Boolean(state.real);
    state.real = asked ?? state.signed;
    if (state.real && !real) fail("--real needs Claude Code, and `claude` isn't on the login shell's PATH.");
    // A job from the other kind of run would go on there, a stand-in's words to the real model, so
    // it's gone before Dana's obid starts: asking it to stop after would be too late.
    if (wasReal !== state.real) {
      for (const name of ["jobs", "worktrees", "clones"]) rmSync(join(homeOf("dana"), name), { recursive: true, force: true });
      delete state.danaJob;
    }
    writeConfig("dana", { bin: state.real ? real : standIn, real: state.real });
    startObid(state, "dana", { real: state.real });
    await answering("you");
    await answering("dana");
    await danaAccount(state);
    await deviceOf("you", state);
    await deviceOf("dana", state);
    await joinBoth(state, spec);
    // Only now: a policy can name only owners who have joined.
    if (!state.policyWritten) {
      await setPolicy(state, spec, RULES);
      state.policyWritten = true;
      save(state);
    }
    await danaSaysYes(state);
    say("Joined: alex (you) and dana, by a private link. The Sphere's rule lets them ask each other questions and for reviews.");
    say(`Dana's sharing: ${JSON.stringify((await api("dana").get("/v1/obid/sharing")).rules?.map((r) => ({ for: r.for, repos: r.repos, tells: r.tells, starter: r.starter })))}`);
    await keepDanaWorking(state);
    state.ready = true;
    save(state);
  } catch (e) {
    await stop();
    throw e;
  }
  await status();
  say(!state.real ? "Dana is on stand-ins: exact states, no model." : state.signed ? "Dana is on the real Claude Code: its job and answers cost real usage on the account signed in." : "Dana is on the real Claude Code with no login: no job, and every question waits on Dana.");
  if (asked !== false && !state.signed) {
    say("");
    say("THE OWNER'S STEP: Dana's account, signed in once, makes Dana's job and answers real. In a terminal:");
    say("");
    say(`  ${signInCommand(join(homeOf("dana"), "accounts", state.danaAccount))}`);
    say("");
    say("Sign in with the account whose usage this may spend, then: node e2e/rig.mjs stop && node e2e/rig.mjs start");
  }
}

// OBID_BIN, or this checkout's, built.
function theObid() {
  if (process.env.OBID_BIN) {
    const bin = resolve(process.env.OBID_BIN);
    return existsSync(bin) ? bin : fail(`OBID_BIN names ${bin}, and nothing is there.`);
  }
  execFileSync("cargo", ["build", "-q", "-p", "obid"], { cwd: join(ROOT, "core"), stdio: "inherit" });
  const target = process.env.CARGO_TARGET_DIR ? resolve(ROOT, "core", process.env.CARGO_TARGET_DIR) : join(ROOT, "core/target");
  const bin = join(target, "debug/obid");
  return existsSync(bin) ? bin : fail(`cargo built, and obid isn't at ${bin}.`);
}

async function status() {
  const state = load();
  say(`The rig's folder: ${RIG}`);
  if (state.checkout) say(`From: ${state.checkout}; obid ${state.obid}; obi-sphere ${state.sphereCommit}`);
  if (state.ready === false && ["sphere", "you", "dana"].some((name) => alive(state, name))) say("A start was cut short: node e2e/rig.mjs stop, then start again.");
  say(`Sphere: ${alive(state, "sphere") ? `up, http://127.0.0.1:${state.spherePort}` : "down"}`);
  for (const who of Object.keys(PEOPLE)) {
    if (!alive(state, who)) { say(`${who}: down`); continue; }
    const { get } = api(who);
    const [membership, jobs, inbox] = await Promise.all([get("/v1/obid/sphere"), get("/v1/jobs"), get("/v1/inbox")]);
    say(`${who} (owner ${PEOPLE[who].owner}): up on ${PEOPLE[who].port}, ${membership.state}, sealing ${membership.sealing}, ${jobs.filter(running).length} job(s) running, inbox ${inbox.length}, OBID_HOME="${homeOf(who)}" OBID_PORT=${PEOPLE[who].port}`);
  }
  if (alive(state, "dana")) {
    const jobs = (await api("dana").get("/v1/jobs")).filter(running);
    for (const job of jobs) say(`Dana's job: "${job.title}", ${job.status}, ${job.id}`);
    const accounts = (await api("dana").get("/v1/status")).accounts ?? [];
    say(`Dana's account: ${accounts.filter((a) => a.id === state.danaAccount).map((a) => (a.signedIn ? "signed in" : "not signed in")).join() || "none yet"}; ${state.real ? "real Claude Code" : "stand-ins"}`);
  }
  if (alive(state, "app")) say(`The dev app: up, pid ${state.running.app.pid}`);
}

// You ask Dana's Oby. Your words go signed, as the app's do, and your stand-in calls oby_ask with
// them; asking from a thread (SPH-3) will need no such stand-in.
async function ask(words) {
  if (!words) fail('What to ask: node e2e/rig.mjs ask "What are you working on?"');
  const state = load();
  if (!alive(state, "you")) fail("You aren't up: node e2e/rig.mjs start");
  const { get } = api("you");
  const exchanges = () => get("/v1/obid/sphere/exchanges?limit=50");
  const before = await exchanges();
  const body = JSON.stringify({ said: `OBY oby_ask ${JSON.stringify({ to: "dana", question: words })}`, new: true });
  const res = await fetch(`http://127.0.0.1:${PEOPLE.you.port}/v1/plans`, { method: "POST", body, signal: AbortSignal.timeout(120_000),
    headers: { authorization: `Bearer ${readFileSync(join(homeOf("you"), "api-token"), "utf8").trim()}`, "content-type": "application/json", "x-oby-words": wordsOf(state.devices.you, "POST", "/v1/plans", body) } });
  const reply = await res.json();
  if (!res.ok) fail(`Asking: ${res.status} ${JSON.stringify(reply)}`);
  say(`You asked. Your stand-in said: ${reply.text}`);
  const exchange = async () => newExchange(before, await exchanges(), words);
  // What obid tells you comes as a turn after the one that asked: an answer, a wait, a no, by 2
  // minutes at most. A question that wasn't sent keeps no exchange, and that turn was all (ASK-8).
  const conversation = () => get(`/v1/conversation?id=${reply.conversation}`);
  if (await exchange()) await until("a word back", async () => (await conversation()).filter((t) => t.who === "oby").length >= 2, 150).catch((e) => say(e.message));
  say("--- your conversation");
  for (const turn of await conversation()) say(`${turn.who}: ${turn.text}`);
  const sent = await exchange();
  say(`--- the exchange: ${sent ? JSON.stringify({ state: sent.state, messages: sent.messages.map((m) => m.envelope?.type) }) : "none: it wasn't sent"}`);
  if (alive(state, "dana")) {
    const inbox = await api("dana").get("/v1/inbox");
    say(`--- Dana's inbox: ${inbox.length ? JSON.stringify(inbox.map((i) => [i.kind, i.title])) : "empty"}`);
  }
}

// Dana's Oby asks yours to review the fixture's change (e2e/lib/rig-review.mjs); your stand-in is
// then the stub agent (e2e/rig-agent). Exits 1 unless Dana's Oby is told approved.
async function review(flags) {
  for (const flag of flags) if (flag !== "--hold") fail(`What's ${flag}? node e2e/rig.mjs review [--hold]`);
  const state = load();
  if (!alive(state, "you")) fail("You aren't up: node e2e/rig.mjs start");
  if (!alive(state, "dana")) fail("Dana isn't up: node e2e/rig.mjs start");
  const body = reviewBody();
  const head = spawnSync("git", ["-C", join(RIG, "fixture/remote.git"), "rev-parse", `refs/heads/${body.head_branch}`], { encoding: "utf8", env: plain() }).stdout?.trim();
  if (head !== body.head_sha) fail(`The fixture's ${body.head_branch} is ${head || "missing"}, not ${body.head_sha}: node e2e/rig.mjs reset, then start.`);
  await reviewOf({ you: api("you"), dana: api("dana"), device: state.devices.you, body, hold: flags.includes("--hold"), until, say });
}

async function job(words) {
  const state = load();
  if (!alive(state, "dana")) fail("Dana isn't up: node e2e/rig.mjs start");
  const personal = words.startsWith("--personal");
  const task = words.replace(/^--personal ?/, "") || undefined;
  const made = await startJob(state, task, personal);
  say(`Dana's job: ${made.id}`);
}

async function policy(word) {
  if (!["on", "off"].includes(word)) fail("node e2e/rig.mjs policy on|off");
  const state = load();
  if (!alive(state, "sphere")) fail("The Sphere isn't up: node e2e/rig.mjs start");
  const spec = await import(join(ROOT, "apps/electron/src/shared/sphere.ts"));
  const now = await setPolicy(state, spec, word === "on" ? RULES : []);
  say(`The Sphere's policy, version ${now.version}: ${now.rules?.length ?? 0} rule(s)`);
}

async function dana(word) {
  const state = load();
  if (word === "stop") {
    await stopThese(state, ["dana"]);
    save(state);
    say("Dana's obid is stopped: its Mac is off.");
  } else if (word === "start") {
    if (alive(state, "dana")) return say("Dana's obid is up already.");
    state.obid = theObid();
    startObid(state, "dana", { real: state.real });
    await answering("dana");
    say("Dana's obid is up.");
  } else fail("node e2e/rig.mjs dana stop|start");
}

async function app() {
  const state = load();
  if (!alive(state, "you")) fail("You aren't up: node e2e/rig.mjs start");
  if (alive(state, "app")) return say(`The dev app is up already, pid ${state.running.app.pid}.`);
  const dir = join(ROOT, "apps/electron");
  if (!existsSync(join(dir, "node_modules"))) execFileSync("npm", ["ci"], { cwd: dir, stdio: "inherit", env: plain() });
  execFileSync("npm", ["run", "build"], { cwd: dir, stdio: "inherit", env: plain() });
  const data = join(dirOf("you"), "electron-data");
  mkdirSync(data, { recursive: true, mode: 0o700 });
  // Its own app folder, so a device key and settings of the rig's, and Home, not the first run.
  const launcher = join(dirOf("you"), "electron-launcher.cjs");
  writeFileSync(launcher, `const { app } = require('electron');
app.setPath('userData', ${JSON.stringify(data)});
app.on('web-contents-created', (_e, page) => page.once('did-finish-load', async () => {
  const fresh = await page.executeJavaScript("!localStorage.getItem('oby.onboarded')");
  if (fresh) { await page.executeJavaScript("for (const k of ['oby.onboarded', 'oby.setUp', 'oby.welcomed']) localStorage.setItem(k, '1')"); page.reload(); }
}));
import(${JSON.stringify(`file://${join(dir, "out/main/index.js")}`)});
`);
  const binary = execFileSync(process.execPath, ["-e", "console.log(require('electron'))"], { cwd: dir, encoding: "utf8", env: plain() }).trim();
  // The real HOME: Electron's safeStorage, which keeps the rig's keys sealed, is in the login Keychain.
  launch(state, "app", "app", binary, [launcher], { cwd: dir, env: { ...plain(), OBID_HOME: homeOf("you"), OBID_PORT: String(PEOPLE.you.port) } });
  say(`The dev app, on you: pid ${state.running.app.pid}. Its log: ${join(RIG, "app.log")}. node e2e/rig.mjs stop closes it.`);
}

// These of the rig's processes, and what they started: nothing else, whatever its command line says.
async function stopThese(state, names) {
  const roots = names.filter((name) => alive(state, name)).map((name) => state.running[name].pid);
  const started = family(roots);
  await stopGroups(roots.map((pid) => ({ pid })));
  for (const { pid } of stillUp(started)) { try { process.kill(pid, "SIGTERM"); } catch {} }
  await until("what they started gone", async () => !stillUp(started).length, 5).catch(() => {
    for (const { pid } of stillUp(started)) { try { process.kill(pid, "SIGKILL"); } catch {} }
  });
  for (const name of names) if (state.running) delete state.running[name];
}

async function stop() {
  const state = load({ anyway: true });
  await stopThese(state, ["app", "you", "dana", "sphere"]);
  if (existsSync(STATE)) save(state);
  const left = orphans(RIG);
  say(left.length ? `The rig is stopped. Not its own to stop, with no parent left (an obid that was killed?): ${left.join(", ")}. Look, and kill them by pid.` : "The rig is stopped. Nothing is left running.");
}

// Everything but what costs to get back: the clone, and Dana's signed-in account (its Keychain item
// goes by the folder's path, and obid knows it by accounts.json).
async function reset() {
  await stop();
  // Again, just before deleting: stopping takes time, and the check at the top was before it.
  const link = aLink(RIG);
  if (link) {
    console.error(`Refused: ${link}`);
    process.exit(2);
  }
  const { danaAccount } = load({ anyway: true });
  for (const name of ["you", "sphere", "fixture", "sphere.log", "you-obid.log", "dana-obid.log", "app.log"]) rmSync(join(RIG, name), { recursive: true, force: true });
  if (existsSync(homeOf("dana"))) for (const name of readdirSync(homeOf("dana"))) if (name !== "accounts" && name !== "accounts.json") rmSync(join(homeOf("dana"), name), { recursive: true, force: true });
  save(danaAccount ? { danaAccount } : {});
  say("The rig starts over at its next start.");
}

// One command at a time changes the rig. A lock whose process has gone is nobody's.
function takeLock() {
  const mine = JSON.stringify(record(process.pid));
  try { writeFileSync(LOCK, mine, { flag: "wx", mode: 0o600 }); }
  catch (e) {
    if (e.code !== "EEXIST") throw e;
    let holder = null;
    try { holder = JSON.parse(readFileSync(LOCK, "utf8")); } catch {}
    if (holder && stillUp([holder], table()).length) fail(`Another rig command is running (pid ${holder.pid}).`);
    writeFileSync(LOCK, mine, { mode: 0o600 });
  }
  process.on("exit", () => rmSync(LOCK, { force: true }));
}

// A rig up from another checkout runs that one's obid: a test from here wouldn't test this code,
// and stopping it pulls it from under whoever is using it. A checkout that's gone asks nothing.
function fromThisCheckout(force) {
  const state = load({ anyway: true });
  if (force || !state.checkout || state.checkout === ROOT || !existsSync(state.checkout)) return;
  if (["sphere", "you", "dana", "app"].some((name) => alive(state, name))) fail(`The rig is up from ${state.checkout}, with that checkout's obid. Run this there; or node e2e/rig.mjs stop --force.`);
}

const [command, ...args] = process.argv.slice(2);
const CHANGES = new Set(["start", "stop", "reset", "job", "review", "policy", "dana", "app"]);
try {
  requireNodeVersion(process.versions.node);
  if (command === "start") {
    mkdirSync(RIG, { recursive: true, mode: 0o700 });
    if (!existsSync(STATE)) save({});
  } else if (!existsSync(RIG) && (command === "stop" || command === "reset")) {
    say("Nothing's here.");
    process.exit(0);
  }
  if (CHANGES.has(command) || command === "ask") {
    if (!existsSync(RIG)) fail("Nothing's here: node e2e/rig.mjs start");
    if (CHANGES.has(command)) takeLock();
    fromThisCheckout((command === "stop" || command === "reset") && args.includes("--force"));
  }
  if (command === "start") await start(new Set(args));
  else if (command === "stop") await stop();
  else if (command === "status") await status();
  else if (command === "ask") await ask(args.join(" "));
  else if (command === "job") await job(args.join(" "));
  else if (command === "review") await review(args);
  else if (command === "policy") await policy(args[0]);
  else if (command === "dana") await dana(args[0]);
  else if (command === "app") await app();
  else if (command === "reset") await reset();
  else fail("node e2e/rig.mjs start | status | ask <words> | review [--hold] | app | job [task] | policy on|off | dana stop|start | stop | reset");
} catch (e) {
  console.error(`The rig: ${e.message}`);
  process.exit(1);
}
