#!/usr/bin/env node
// Two Obys and a Sphere on this Mac, for testing what talking to a person's Oby does on the real
// thing (docs/contracts/test-rig.md): a local Sphere from a scratch clone of obi-sphere's dev, two
// scratch obids, you (owner alex) and Dana (a test teammate, owner dana), and Dana's long job. The
// owner's real Oby, their logins, GitHub and the live Sphere are never used.
//
//   node e2e/rig.mjs start [--stand-in | --real]   everything up, joined, the rule written, Dana's job running
//   node e2e/rig.mjs status                        what's up, in a few lines
//   node e2e/rig.mjs ask "What are you working on?"  you ask Dana's Oby; prints what came back
//   node e2e/rig.mjs app                           the Electron dev app, on you
//   node e2e/rig.mjs job [task]                    one more job for Dana
//   node e2e/rig.mjs policy on|off                 the Sphere's rule written, or none (PEER-8)
//   node e2e/rig.mjs dana stop|start               Dana's obid only (a Mac that's off, PEER-7)
//   node e2e/rig.mjs stop                          nothing left running; the folder stays
//   node e2e/rig.mjs reset                         stop, and start over (Dana's sign-in and the clone stay)
//
// Environment: OBY_RIG_DIR, the folder (default ~/Library/Application Support/oby-rig); OBID_BIN,
// obid (default: this checkout's, built first).
//
// Dana answers with the real Claude Code once the owner signed in the account start prints the
// command for; until then, or with --stand-in, stand-ins (e2e/rig-agent) give exact states. You
// are always on stand-ins: asking runs no model of yours.

import { execFileSync, spawn, spawnSync } from "node:child_process";
import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes, sign } from "node:crypto";
import { closeSync, existsSync, mkdirSync, openSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { buildFixture } from "./lib/fixture.mjs";
import { groupRunning, stopGroups } from "./lib/processes.mjs";
import { folderRefusal, portRefusal, rigRefusal } from "./lib/rig-guard.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const RIG = resolve(process.env.OBY_RIG_DIR || join(homedir(), "Library/Application Support/oby-rig"));
const AGENTS = join(ROOT, "core/crates/obid/tests/agents");
const SPHERE_REMOTE = "https://github.com/obi-ai-bot/obi-sphere.git";
const SPHERE_PORTS = [8791, 8792, 8793]; // 8799 is often another session's
const PEOPLE = { you: { owner: "alex", name: "Alex’s Oby", port: 8794 }, dana: { owner: "dana", name: "Dana’s Oby", port: 8795 } };
const REPO = "obi-e2e/fixture-app";
const LONG_TASK = "Add one small test file a minute, 30 times, committing each: tests/step-01.test.js, then tests/step-02.test.js and so on, with `sleep 60` between them. Each file holds one passing node:test. Nothing else.";
const RULES = [["alex", "dana"], ["dana", "alex"]].map(([from_owner, to_owner]) => ({ id: `${from_owner}-asks-${to_owner}`, from_owner, to_owner, actions: ["question"], repos: ["*"] }));

// This comes first, before anything is read, made or started.
const refused = rigRefusal(RIG);
if (refused) {
  console.error(`Refused: ${refused}`);
  process.exit(2);
}

const STATE = join(RIG, "rig.json");
const load = () => (existsSync(STATE) ? JSON.parse(readFileSync(STATE, "utf8")) : {});
const save = (state) => writeFileSync(STATE, JSON.stringify(state, null, 2), { mode: 0o600 });
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

// A process of the rig's: its own group, its output in a log, outliving this command.
function launch(name, cmd, args, opts) {
  const log = openSync(join(RIG, `${name}.log`), "a");
  const child = spawn(cmd, args, { ...opts, detached: true, stdio: ["ignore", log, log] });
  child.unref();
  closeSync(log);
  return child.pid;
}
const alive = (pid) => Boolean(pid) && groupRunning(pid);

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

const claude = () => {
  const found = spawnSync(process.env.SHELL || "/bin/sh", ["-lc", "command -v claude"], { encoding: "utf8" }).stdout?.trim().split("\n").pop();
  return found && existsSync(found) ? found : null;
};
// The tool itself says whether the account's folder has a login: Keychain or file, its own business.
function signedIn(bin, folder) {
  const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => !/^(CLAUDE|ANTHROPIC)/.test(k)));
  const out = spawnSync(bin, ["auth", "status", "--json"], { encoding: "utf8", env: { ...env, CLAUDE_CONFIG_DIR: folder }, timeout: 15_000 });
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
    repos: [{ repo: REPO, remote: `file://${join(RIG, "fixture/remote.git")}`, defaultBranch: "main", check: "true", pullRequests: false }],
  }, null, 2), { mode: 0o600 });
}

function startObid(who, { real }) {
  const dir = dirOf(who);
  for (const d of ["env-home", "tmp", "gh"]) mkdirSync(join(dir, d), { recursive: true, mode: 0o700 });
  // A scratch HOME and a stand-in Keychain, unless Dana's real login is what's being run: macOS
  // finds the login Keychain by HOME. Either way no GitHub login: an empty gh folder.
  return launch(`${who}-obid`, OBID, ["serve", "--config", join(dir, "config.json")], { env: {
    PATH: `${AGENTS}:${dirname(process.execPath)}:${process.env.PATH}`, USER: process.env.USER, LOGNAME: process.env.LOGNAME,
    HOME: real ? homedir() : join(dir, "env-home"), TMPDIR: join(dir, "tmp"), GH_CONFIG_DIR: join(dir, "gh"),
    ...(real ? {} : { OBID_SECURITY_BIN: join(AGENTS, "fake-security") }),
    OBID_PRESTART: "off", OBID_USAGE_READS: "off", OBID_LOAD: "0", OBID_MEMORY_AVAILABLE: "1099511627776", // other sessions' load doesn't hold a job back
  } });
}
const answering = (who) => until(`${who}'s obid`, async () => (await api(who).call("GET", "/v1/status")).status === 200);

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

function cloneSphere() {
  const clone = join(RIG, "obi-sphere");
  if (!existsSync(join(clone, "server/package.json"))) {
    rmSync(clone, { recursive: true, force: true });
    say("Cloning obi-sphere's dev…");
    execFileSync("git", ["clone", "-q", "--depth", "1", "--branch", "dev", SPHERE_REMOTE, clone], { stdio: "inherit" });
  }
  if (!existsSync(join(clone, "server/node_modules"))) {
    say("Installing the Sphere's dependencies…");
    execFileSync("pnpm", ["install", "--frozen-lockfile", "--silent"], { cwd: join(clone, "server"), stdio: "inherit" });
  }
  return execFileSync("git", ["-C", clone, "rev-parse", "--short", "HEAD"], { encoding: "utf8" }).trim();
}

function startSphere(state) {
  const dir = join(RIG, "sphere");
  for (const d of ["state", "home", "tmp"]) mkdirSync(join(dir, d), { recursive: true, mode: 0o700 });
  return launch("sphere", process.execPath, ["scripts/local.mjs", "--port", String(state.spherePort), "--persist-to", join(dir, "state")], {
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

async function startJob(state, task) {
  const job = { repo: REPO, kind: "fix", title: "Billing retries", task: task ?? (state.real ? LONG_TASK : "SLEEP"), account: state.real ? state.danaAccount : "stand-in" };
  const r = await api("dana").call("POST", "/v1/jobs", { job });
  if (r.status !== 201) fail(`Dana's job: ${r.status} ${r.text}`);
  return JSON.parse(r.text);
}
const running = (job) => job.status === "running";

// ---- commands

async function start(flags) {
  mkdirSync(RIG, { recursive: true, mode: 0o700 });
  for (const who of Object.keys(PEOPLE)) {
    const refusal = folderRefusal(homeOf(who)) ?? portRefusal(PEOPLE[who].port);
    if (refusal) { console.error(`Refused: ${refusal}`); process.exit(2); }
    mkdirSync(homeOf(who), { recursive: true, mode: 0o700 });
  }
  const state = load();
  const up = ["spherePid", "youPid", "danaPid"].filter((k) => alive(state[k]));
  if (up.length) {
    if (up.length === 3) { say("The rig is running already."); return status(); }
    fail(`Only part of the rig is running (${up.join(", ")}): node e2e/rig.mjs stop, then start again.`);
  }
  for (const who of Object.keys(PEOPLE)) if (!(await free(PEOPLE[who].port))) fail(`Port ${PEOPLE[who].port}, ${who}'s obid's, is taken by something else.`);
  if (!state.spherePort) {
    for (const port of SPHERE_PORTS) if (!state.spherePort && (await free(port))) state.spherePort = port;
    state.operator = randomBytes(32).toString("base64url");
    if (!state.spherePort) fail(`Ports ${SPHERE_PORTS.join(", ")} are all taken: another session's Sphere?`);
    save(state);
  } else if (!(await free(state.spherePort))) fail(`Port ${state.spherePort}, the Sphere's, is taken by something else, and both Obys' memberships name it. node e2e/rig.mjs reset picks another.`);

  OBID = process.env.OBID_BIN ? resolve(process.env.OBID_BIN) : buildObid();
  const spec = await import(join(ROOT, "apps/electron/src/shared/sphere.ts"));
  if (!existsSync(join(RIG, "fixture/remote.git"))) buildFixture(join(RIG, "fixture"));
  const sphereCommit = cloneSphere();

  let signed = false;
  try {
    state.spherePid = startSphere(state);
    save(state);
    await until("the Sphere", async () => (await sphereCall(state)("/health")).status === 200, 120);
    await makeSphere(state, spec);
    say(`Sphere: http://127.0.0.1:${state.spherePort} (obi-sphere dev ${sphereCommit})`);

    const standIn = join(ROOT, "e2e/rig-agent");
    writeConfig("you", { bin: standIn, real: false });
    state.youPid = startObid("you", { real: false });
    save(state);

    const real = claude();
    const accountKnown = state.danaAccount ? join(homeOf("dana"), "accounts", state.danaAccount) : null;
    signed = Boolean(real && accountKnown && signedIn(real, accountKnown));
    const wasReal = Boolean(state.real);
    state.real = !flags.has("--stand-in") && (flags.has("--real") || signed);
    if (state.real && !real) fail("--real needs Claude Code, and `claude` isn't on the login shell's PATH.");
    // A job from the other kind of run would go on there, a stand-in's words to the real model, so
    // it's gone before Dana's obid starts: asking it to stop after would be too late.
    if (wasReal !== state.real) {
      for (const name of ["jobs", "worktrees", "clones"]) rmSync(join(homeOf("dana"), name), { recursive: true, force: true });
      delete state.danaJob;
    }
    writeConfig("dana", { bin: state.real ? real : standIn, real: state.real });
    state.danaPid = startObid("dana", { real: state.real });
    save(state);
    await answering("you");
    await answering("dana");
    // A job from the other kind of run would go on there: a stand-in's words to the real model.
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
    say("Joined: alex (you) and dana, by a private link. The Sphere's rule lets them ask each other.");
    say(`Dana's sharing: ${JSON.stringify((await api("dana").get("/v1/obid/sharing")).rules?.map((r) => ({ for: r.for, tells: r.tells, starter: r.starter })))}`);

    // The real Claude Code with no login runs no job: answers go to Dana, and the step below is why.
    if (!state.real || signed) {
      // The job from before a stop takes a moment to go on: a second beside it would be a lie.
      const goesOn = state.danaJob && (await until("Dana's job going on", async () => running(await api("dana").get(`/v1/jobs/${state.danaJob}`)), 15).catch(() => false));
      if (!goesOn) {
        const job = await startJob(state);
        state.danaJob = job.id;
        save(state);
        await until("Dana's job running", async () => {
          const seen = await api("dana").get(`/v1/jobs/${job.id}`);
          if (["failed", "interrupted", "done"].includes(seen.status)) fail(`Dana's job ended ${seen.status}: ${seen.reason ?? seen.summary ?? ""}`);
          return running(seen);
        });
      }
    }
  } catch (e) {
    await stop();
    throw e;
  }
  await status();
  say(!state.real ? "Dana is on stand-ins: exact states, no model." : signed ? "Dana is on the real Claude Code: its job and answers cost real usage on the account signed in." : "Dana is on the real Claude Code with no login: no job, and every question waits on Dana.");
  if (!state.real || !signed) {
    say("");
    say("THE OWNER'S STEP: Dana's account, signed in once, makes Dana's job and answers real. In a terminal:");
    say("");
    say(`  ${signInCommand(join(homeOf("dana"), "accounts", state.danaAccount))}`);
    say("");
    say("Sign in with the account whose usage this may spend, then: node e2e/rig.mjs stop && node e2e/rig.mjs start");
  }
}

let OBID;
function buildObid() {
  execFileSync("cargo", ["build", "-q", "-p", "obid"], { cwd: join(ROOT, "core"), stdio: "inherit" });
  const target = process.env.CARGO_TARGET_DIR ? resolve(ROOT, "core", process.env.CARGO_TARGET_DIR) : join(ROOT, "core/target");
  return join(target, "debug/obid");
}

async function status() {
  const state = load();
  say(`The rig's folder: ${RIG}`);
  say(`Sphere: ${alive(state.spherePid) ? `up, http://127.0.0.1:${state.spherePort}` : "down"}`);
  for (const who of Object.keys(PEOPLE)) {
    if (!alive(state[`${who}Pid`])) { say(`${who}: down`); continue; }
    const { get } = api(who);
    const [membership, jobs, inbox] = await Promise.all([get("/v1/obid/sphere"), get("/v1/jobs"), get("/v1/inbox")]);
    say(`${who} (owner ${PEOPLE[who].owner}): up on ${PEOPLE[who].port}, ${membership.state}, sealing ${membership.sealing}, ${jobs.filter(running).length} job(s) running, inbox ${inbox.length}, OBID_HOME="${homeOf(who)}" OBID_PORT=${PEOPLE[who].port}`);
  }
  if (alive(state.danaPid)) {
    const jobs = (await api("dana").get("/v1/jobs")).filter(running);
    for (const job of jobs) say(`Dana's job: "${job.title}", ${job.status}, ${job.id}`);
    const accounts = (await api("dana").get("/v1/status")).accounts ?? [];
    say(`Dana's account: ${accounts.filter((a) => a.id === state.danaAccount).map((a) => (a.signedIn ? "signed in" : "not signed in")).join() || "none yet"}; ${state.real ? "real Claude Code" : "stand-ins"}`);
  }
  if (alive(state.appPid)) say(`The dev app: up, pid ${state.appPid}`);
}

// You ask Dana's Oby. Your words go signed, as the app's do, and your stand-in calls oby_ask with
// them; asking from a thread (SPH-3) will need no such stand-in.
async function ask(words) {
  if (!words) fail('What to ask: node e2e/rig.mjs ask "What are you working on?"');
  const state = load();
  if (!alive(state.youPid)) fail("You aren't up: node e2e/rig.mjs start");
  const { get } = api("you");
  const body = JSON.stringify({ said: `OBY oby_ask ${JSON.stringify({ to: "dana", question: words })}`, new: true });
  const signedAt = new Date().toISOString();
  const digest = createHash("sha256").update(body).digest("hex");
  const { id, key } = state.devices.you;
  const signature = sign("sha256", Buffer.from(`oby-words\nPOST\n/v1/plans\n${digest}\n${signedAt}`), createPrivateKey(key)).toString("base64");
  const res = await fetch(`http://127.0.0.1:${PEOPLE.you.port}/v1/plans`, { method: "POST", body, signal: AbortSignal.timeout(120_000),
    headers: { authorization: `Bearer ${readFileSync(join(homeOf("you"), "api-token"), "utf8").trim()}`, "content-type": "application/json", "x-oby-words": `${id} ${signedAt} ${signature}` } });
  const reply = await res.json();
  if (!res.ok) fail(`Asking: ${res.status} ${JSON.stringify(reply)}`);
  say(`You asked. Your stand-in said: ${reply.text}`);
  const exchange = async () => (await get("/v1/obid/sphere/exchanges?limit=50")).find((e) => e.messages?.[0]?.envelope?.body?.text === words);
  // What obid tells you comes as a turn after the one that asked: an answer, a wait, a no, by 2
  // minutes at most. A question that wasn't sent keeps no exchange, and that turn was all (ASK-8).
  const conversation = () => get(`/v1/conversation?id=${reply.conversation}`);
  if (await exchange()) await until("a word back", async () => (await conversation()).filter((t) => t.who === "oby").length >= 2, 150).catch((e) => say(e.message));
  say("--- your conversation");
  for (const turn of await conversation()) say(`${turn.who}: ${turn.text}`);
  const sent = await exchange();
  say(`--- the exchange: ${sent ? JSON.stringify({ state: sent.state, messages: sent.messages.map((m) => m.envelope?.type) }) : "none: it wasn't sent"}`);
  if (alive(state.danaPid)) {
    const inbox = await api("dana").get("/v1/inbox");
    say(`--- Dana's inbox: ${inbox.length ? JSON.stringify(inbox.map((i) => [i.kind, i.title])) : "empty"}`);
  }
}

async function job(task) {
  const state = load();
  if (!alive(state.danaPid)) fail("Dana isn't up: node e2e/rig.mjs start");
  const made = await startJob(state, task);
  say(`Dana's job: ${made.id}`);
}

async function policy(word) {
  if (!["on", "off"].includes(word)) fail("node e2e/rig.mjs policy on|off");
  const state = load();
  if (!alive(state.spherePid)) fail("The Sphere isn't up: node e2e/rig.mjs start");
  const spec = await import(join(ROOT, "apps/electron/src/shared/sphere.ts"));
  const now = await setPolicy(state, spec, word === "on" ? RULES : []);
  say(`The Sphere's policy, version ${now.version}: ${now.rules?.length ?? 0} rule(s)`);
}

async function dana(word) {
  const state = load();
  if (word === "stop") {
    await stopGroups([{ pid: state.danaPid }].filter((c) => alive(c.pid)));
    say("Dana's obid is stopped: its Mac is off.");
  } else if (word === "start") {
    if (alive(state.danaPid)) return say("Dana's obid is up already.");
    OBID = process.env.OBID_BIN ? resolve(process.env.OBID_BIN) : buildObid();
    state.danaPid = startObid("dana", { real: state.real });
    save(state);
    await answering("dana");
    say("Dana's obid is up.");
  } else fail("node e2e/rig.mjs dana stop|start");
}

async function app() {
  const state = load();
  if (!alive(state.youPid)) fail("You aren't up: node e2e/rig.mjs start");
  if (alive(state.appPid)) return say(`The dev app is up already, pid ${state.appPid}.`);
  const dir = join(ROOT, "apps/electron");
  if (!existsSync(join(dir, "node_modules"))) execFileSync("npm", ["ci"], { cwd: dir, stdio: "inherit" });
  execFileSync("npm", ["run", "build"], { cwd: dir, stdio: "inherit" });
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
  const { OBID_HOME: _home, OBID_PORT: _port, ...env } = process.env;
  const binary = execFileSync(process.execPath, ["-e", "console.log(require('electron'))"], { cwd: dir, encoding: "utf8" }).trim();
  state.appPid = launch("app", binary, [launcher], { cwd: dir, env: { ...env, OBID_HOME: homeOf("you"), OBID_PORT: String(PEOPLE.you.port) } });
  save(state);
  say(`The dev app, on you: pid ${state.appPid}. Its log: ${join(RIG, "app.log")}. node e2e/rig.mjs stop closes it.`);
}

// Whatever still has the rig's folder on its command line: a job's agent in a group of its own.
function strays() {
  const rows = execFileSync("ps", ["-axo", "pid=,command="], { encoding: "utf8", maxBuffer: 1 << 26 }).split("\n");
  return rows.filter((row) => row.includes(RIG)).map((row) => Number(row.trim().split(/\s+/)[0])).filter((pid) => pid && pid !== process.pid);
}

async function stop() {
  const state = load();
  await stopGroups(["appPid", "youPid", "danaPid", "spherePid"].map((k) => ({ pid: state[k] })).filter((c) => c.pid));
  for (const pid of strays()) { try { process.kill(pid, "SIGTERM"); } catch {} }
  await until("everything stopped", async () => !strays().length, 15).catch(() => {
    for (const pid of strays()) { try { process.kill(pid, "SIGKILL"); } catch {} }
  });
  for (const k of ["appPid", "youPid", "danaPid", "spherePid"]) delete state[k];
  if (existsSync(RIG)) save(state);
  const left = strays();
  say(left.length ? `Still running: ${left.join(", ")}` : "The rig is stopped. Nothing is left running.");
}

// Everything but what costs to get back: the clone, and Dana's signed-in account (its Keychain item
// goes by the folder's path, and obid knows it by accounts.json).
async function reset() {
  if (!existsSync(RIG)) return say("Nothing to reset.");
  await stop();
  const { danaAccount } = load();
  for (const name of ["you", "sphere", "fixture", "rig.json", "sphere.log", "you-obid.log", "dana-obid.log", "app.log"]) rmSync(join(RIG, name), { recursive: true, force: true });
  if (existsSync(homeOf("dana"))) for (const name of readdirSync(homeOf("dana"))) if (name !== "accounts" && name !== "accounts.json") rmSync(join(homeOf("dana"), name), { recursive: true, force: true });
  if (danaAccount) save({ danaAccount });
  say("The rig starts over at its next start.");
}

const [command, ...args] = process.argv.slice(2);
try {
  if (command === "start") await start(new Set(args));
  else if (command === "stop") await stop();
  else if (command === "status") await status();
  else if (command === "ask") await ask(args.join(" "));
  else if (command === "job") await job(args.join(" ") || undefined);
  else if (command === "policy") await policy(args[0]);
  else if (command === "dana") await dana(args[0]);
  else if (command === "app") await app();
  else if (command === "reset") await reset();
  else fail("node e2e/rig.mjs start | status | ask <words> | app | job [task] | policy on|off | dana stop|start | stop | reset");
} catch (e) {
  console.error(`The rig: ${e.message}`);
  process.exit(1);
}
