#!/usr/bin/env node
// The launch list's Obysphere tests, L1 to L9 (docs/contracts/test-rig.md, RIG-27 to 34 and 36; the launch
// plan's "Launch list"): what a stranger does with Oby and a teammate's Oby, on the rig's local
// Sphere at the deployed server's commit (e2e/sphere.ref), with "you" driven through the Electron
// dev app with real clicks and the app's own keys (e2e/lib/app-driver.mjs). Dana is the rig's
// second obid, on stand-ins; only Dana's side is signed by the rig.
//
//   node e2e/launch.mjs [L1 L3 …] [--keep] [--gate]
//
// Each run starts from nothing: `reset`, then `start --app --stand-in` in its own rig folder
// (OBY_RIG_DIR, default ~/Library/Application Support/oby-rig-launch), so "you" is a fresh home that
// has never joined anything. It prints one line per check and a table, L1 to L8, green or red with
// why. Exit 0 only when every scenario run is green; with --gate, when the reds are exactly those in
// e2e/launch-red.json, each naming what turns it green (a list that may only shrink: a scenario
// that turned green and is still listed fails too). --keep leaves the rig up afterwards.
//
// Owner's real Oby, logins, GitHub and the live Sphere: never used (the rig's own guards).

import { spawn, spawnSync } from "node:child_process";
import { createPrivateKey, generateKeyPairSync, randomBytes } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { homedir, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { appOwnerKey, askInThread, firstVisible, goHome as home, openApp, openMember, openSphere, showing, showSidebar, threadText, until } from "./lib/app-driver.mjs";
import { confirmOf, reviewBody, RULES, wordsOf } from "./lib/rig-review.mjs";
import { loadSphereSpec } from "./lib/sphere-spec.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const APP_DIR = join(ROOT, "apps/electron");
const RIG = resolve(process.env.OBY_RIG_DIR || join(homedir(), "Library/Application Support/oby-rig-launch"));
const PORTS = { you: 8794, dana: 8795 };
const REPO = "obi-e2e/fixture-app";
const KNOWN_RED = join(HERE, "launch-red.json");
// A review or a handoff runs a job on your stand-in: about a minute alone, longer when the check
// builds and tests beside it on the same Mac (L6 took more than 5 minutes there)
const REVIEW_WAIT = 600_000;

const args = process.argv.slice(2);
const keep = args.includes("--keep"), gate = args.includes("--gate");
const only = new Set(args.filter((a) => /^L\d+$/.test(a)));
const say = (line) => console.log(line);
const plain = () => Object.fromEntries(["PATH", "HOME", "USER", "LOGNAME", "TMPDIR", "LANG"].filter((k) => process.env[k]).map((k) => [k, process.env[k]]));

// ---- the rig, by its own commands

function rig(...words) {
  say(`$ node e2e/rig.mjs ${words.join(" ")}`);
  const r = spawnSync(process.execPath, [join(HERE, "rig.mjs"), ...words], { stdio: "inherit", env: { ...process.env, OBY_RIG_DIR: RIG } });
  if (r.status !== 0) throw new Error(`node e2e/rig.mjs ${words.join(" ")} exited ${r.status}`);
}
const rigState = () => JSON.parse(readFileSync(join(RIG, "rig.json"), "utf8"));

function obid(who) {
  const base = `http://127.0.0.1:${PORTS[who]}`;
  const token = () => readFileSync(join(RIG, who, "home/api-token"), "utf8").trim();
  const call = async (method, path, body, extra = {}) => {
    try {
      const res = await fetch(base + path, { method, headers: { authorization: `Bearer ${token()}`, "content-type": "application/json", ...extra }, body: body === undefined ? undefined : typeof body === "string" ? body : JSON.stringify(body), signal: AbortSignal.timeout(60_000) });
      return { status: res.status, text: await res.text() };
    } catch (e) { return { status: 0, text: e.message }; }
  };
  const get = async (path) => {
    const r = await call("GET", path);
    if (r.status !== 200) throw new Error(`GET ${path} on ${who}: ${r.status} ${r.text}`);
    return JSON.parse(r.text);
  };
  return { call, get };
}
const you = obid("you"), dana = obid("dana");

// Dana's words, signed by the rig's device for Dana (Dana has no app here): the request's exact body.
async function danaSays(method, path, body) {
  const text = JSON.stringify(body);
  return dana.call(method, path, text, { "x-oby-words": wordsOf(rigState().devices.dana, method, path, text) });
}

function sphereApi() {
  const state = rigState();
  const call = async (path, { method = "GET", token, body, form } = {}) => {
    const r = await fetch(`http://127.0.0.1:${state.spherePort}/v0.1${path}`, { method, redirect: "error",
      headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), ...(body ? { "content-type": "application/json" } : form ? { "content-type": "application/x-www-form-urlencoded" } : {}) },
      body: body ? JSON.stringify(body) : form ? new URLSearchParams(form) : undefined });
    const text = await r.text();
    let parsed = null;
    try { parsed = text ? JSON.parse(text) : null; } catch { parsed = text; }
    return { status: r.status, body: parsed };
  };
  const admin = async () => {
    const r = await call("/oauth/token", { method: "POST", form: {
      grant_type: "urn:ietf:params:oauth:grant-type:token-exchange", subject_token_type: "urn:ietf:params:oauth:token-type:jwt",
      subject_token: spec.assertion(createPrivateKey(state.adminKey), "rig-admin", "rig", "admin directory:read audit:read") } });
    if (!r.body?.access_token) throw new Error(`the admin's token: ${r.status} ${JSON.stringify(r.body)}`);
    return r.body.access_token;
  };
  return { call, admin, state };
}
async function newLink() {
  const s = sphereApi();
  const i = await s.call("/spheres/rig/invitations", { method: "POST", token: await s.admin(), body: { expires_in: 86_400, max_uses: 5 } });
  if (i.status !== 201) throw new Error(`a new link: ${i.status} ${JSON.stringify(i.body)}`);
  return `http://127.0.0.1:${s.state.spherePort}/join#sphere=rig&name=The%20rig&by=rig-admin&id=${i.body.invitation_id}&secret=${i.body.secret}`;
}
async function writePolicy(rules) {
  const s = sphereApi(), token = await s.admin();
  const now = (await s.call("/spheres/rig/policy", { token })).body;
  const r = await s.call("/spheres/rig/policy", { method: "PUT", token, body: { expected_version: now.version, rules } });
  if (r.status !== 200) throw new Error(`the policy: ${r.status} ${JSON.stringify(r.body)}`);
}
const member = async (obi) => { const s = sphereApi(); return (await s.call(`/spheres/rig/members/${encodeURIComponent(obi)}`, { token: await s.admin() })).body; };

// ---- what a check is

class Red extends Error {}
const red = (why) => { throw new Red(why); };
const check = (ok, why) => { if (!ok) red(why); };
const ok = (line) => say(`  ok  ${line}`);
const exchangesOf = async (o) => o.get("/v1/obid/sphere/exchanges?limit=100");
const textOf = (e) => JSON.stringify(e.messages?.map((m) => m.envelope?.body) ?? []);

// Words in Dana's thread, by a real click on Send; the exchange it made on your obid.
async function askDana(words) {
  const before = new Set((await exchangesOf(you)).map((e) => e.correlationId));
  await openMember(ctx.page, "dana");
  await askInThread(ctx.page, words);
  return until("the exchange your ask made", async () => (await exchangesOf(you)).find((e) => !before.has(e.correlationId) && textOf(e).includes(words)), 30_000)
    .catch(async () => red(`no exchange on your obid for "${words}" within 30 s; the thread says: ${(await threadText(ctx.page)).slice(-300)}`));
}

// The members both ends list
const membersOf = async (o) => o.get("/v1/obid/sphere/members");

// ---- joining you: in the app (L1), or by the app's oby:// path when the app has no field yet

// The app's own field for an invite link, wherever it puts it: the side panel's Obysphere, or the
// Obysphere's place in Settings. Null when there's none.
async function linkField(page) {
  const field = () => firstVisible([page.getByRole("textbox", { name: /invite|link/i }), page.getByPlaceholder(/invite|link|https?:\/\//i)], 1500);
  let found = await field();
  if (found) return found;
  // the side panel's Obysphere place, and a Join there (Sangam's words aren't settled: any of these)
  const join = await firstVisible([page.getByRole("button", { name: /Obysphere|Join/i })], 1500);
  if (join) { await join.click({ timeout: 5000 }).catch(() => {}); found = await field(); if (found) return found; }
  // Settings › Obysphere (#2515), from a side panel shown first: a fresh app opens with it folded
  await showSidebar(page);
  const settings = await firstVisible([page.getByRole("button", { name: "Settings", exact: true })], 1500);
  if (settings) {
    await settings.click({ timeout: 5000 }).catch(() => {});
    const place = await firstVisible([page.getByRole("button", { name: "Obysphere", exact: true })], 3000);
    if (place) await place.click({ timeout: 5000 }).catch(() => {});
    found = await field();
    if (!found) await home(page);
  }
  return found;
}

// The link, opened as macOS hands an oby:// link to the app (main/sphere.ts, `open-url`), and
// main's own "Join …?" answered Join for the person: the rig's one stand-in for a click, until the
// app has a field of its own (L1). The join itself is main's, with the app's owner key.
// main's next native message box answered with its first button, Join; answers a function that
// says what it asked, or null while it hasn't
async function answerMainsJoin() {
  await ctx.app.evaluate(({ dialog }) => {
    const was = dialog.showMessageBox;
    globalThis.__rigAsked = null;
    dialog.showMessageBox = async (...a) => {
      dialog.showMessageBox = was;
      const o = a.find((x) => x && typeof x === "object" && "message" in x) ?? {};
      globalThis.__rigAsked = { message: String(o.message ?? ""), detail: String(o.detail ?? "") };
      return { response: 0, checkboxChecked: false };
    };
  });
  return () => ctx.app.evaluate(() => globalThis.__rigAsked);
}

async function joinByOpenUrl(link) {
  await ctx.app.evaluate(({ app, dialog }, url) => {
    const was = dialog.showMessageBox;
    dialog.showMessageBox = async (...a) => { dialog.showMessageBox = was; return { response: 0, checkboxChecked: false }; };
    app.emit("open-url", { preventDefault() {} }, url);
  }, link);
}

// Each scenario starts with the whole rig up: one that took the Sphere or Dana's obid down (L7) and
// ended red before bringing it back leaves the next one nothing to talk to ("fetch failed")
async function stackUp() {
  const answers = async (url) => { try { return (await fetch(url, { signal: AbortSignal.timeout(5000) })).status > 0; } catch { return false; } };
  if (!(await answers(`http://127.0.0.1:${rigState().spherePort}/v0.1/health`))) { say("  (the Sphere was down: started again)"); rig("sphere", "start"); }
  if ((await dana.call("GET", "/v1/status")).status !== 200) { say("  (Dana's obid was down: started again)"); rig("dana", "start"); }
  if ((await you.call("GET", "/v1/status")).status !== 200) throw new Error("your obid is down: nothing after this can run");
}

// Everything after L1 needs you joined and the rule written: by the app's field if L1 joined, else
// by the app's oby:// path (main's own join and key; its dialog answered by the rig), said so.
async function joinedForTheRest() {
  if (ctx.ruled) return;
  if ((await you.get("/v1/obid/sphere")).state !== "joined") {
    say("\n(you join by the app's oby:// path: main's own join and owner key, its \"Join …?\" answered by the rig, as the app has no field for a link)");
    await joinByOpenUrl(ctx.link);
    if (!(await joined(60))) throw new Error("the app's oby:// join didn't join either: nothing after L1 can run");
    ctx.joinedBy = "the app's oby:// path";
  }
  rig("policy", "on");
  ctx.ruled = true;
}

async function joined(seconds = 30) {
  return until("you joined", async () => (await you.get("/v1/obid/sphere")).state === "joined", seconds * 1000).catch(() => false);
}

// ---- the scenarios

const SCENARIOS = [
  { id: "L1", rig: "RIG-27", title: "A fresh home joins from a private link, in the app, with the app's key", run: async () => {
    const page = ctx.page;
    check((await you.get("/v1/obid/sphere")).state !== "joined", "you were joined before the app did anything");
    check(!appOwnerKey(ctx.userData), "the app made an owner key before anyone joined");
    ok("a fresh home: not joined, no owner key yet");
    const field = await linkField(page);
    if (!field) red("no field in the app to enter an invite link (B2 wave 1: Join with a link in the app and on first run)");
    await field.click();
    await page.keyboard.insertText(ctx.link);
    // the owner id it will join as, shown once the link reads, before any key is made
    await showing(page, /\balex\b/, 10).catch(() => red("with the link in, the app doesn't show the owner id it joins as (alex)"));
    check(!appOwnerKey(ctx.userData), "the owner key was made before Join");
    ok("the link in the app's field, the owner id alex shown, no key made yet");
    const join = await firstVisible([page.getByRole("button", { name: /^Join\b/ })], 5000);
    if (!join) red("the link is in, and there's no Join button");
    // Join hands the link to main, whose own native "Join …?" no click of the rig's can reach: the rig
    // answers it Join, the one click it stands in for, and keeps what it said
    const asked = await answerMainsJoin();
    await sleep(600);
    await join.click();
    const confirm = await firstVisible([page.getByRole("dialog").getByRole("button", { name: /^Join\b/ })], 2000);
    if (confirm) await confirm.click();
    if (!(await joined(30))) red(`Join was clicked and you didn't join within 30 s${(await asked()) ? "" : " (main asked nothing)"}`);
    const said = await asked();
    if (said) ok(`main's own dialog asked "${said.message}" (${said.detail.replace(/\s+/g, " ").slice(0, 120)}), answered Join by the rig`);
    ok("joined from the link in the app, with a real click on Join");
    const card = await member((await you.get("/v1/obid/sphere")).obiId);
    check(card?.owner_key && card.owner_key === appOwnerKey(ctx.userData), "the Sphere binds alex to a key that isn't the app's");
    ok("the Sphere binds alex to the key the app's main made");
    ctx.joinedBy = "the app's link field";
  } },
  { id: "L2", rig: "RIG-28", title: "Members see each other within 60 s; a look-alike is named in the Confirm", run: async () => {
    const me = await you.get("/v1/obid/sphere");
    const card = await member(me.obiId);
    check(card?.owner_key && card.owner_key === appOwnerKey(ctx.userData), `the Sphere binds alex to ${card?.owner_key?.slice(0, 8)}, not the app's owner key ${appOwnerKey(ctx.userData)?.slice(0, 8)}`);
    ok("alex is bound to the app's own owner key");
    await until("you seeing dana", async () => (await membersOf(you)).some((m) => m.owner === "dana"), 60_000).catch(() => red("your obid doesn't list dana within 60 s"));
    await until("dana seeing alex", async () => (await membersOf(dana)).some((m) => m.owner === "alex"), 60_000).catch(() => red("Dana's obid doesn't list alex within 60 s"));
    await openSphere(ctx.page);
    await until("dana in the side panel", async () => { await openSphere(ctx.page); return ctx.page.getByRole("button", { name: /^Chat with dana’s Oby/ }).count(); }, 60_000)
      .catch(async () => red(`the app's side panel doesn't show dana within 60 s of joining; it shows: ${JSON.stringify(await buttons())}`));
    ok("each sees the other, and the app shows dana");
    await until("sealing on, on both", async () => (await you.get("/v1/obid/sphere")).sealing === "on" && (await dana.get("/v1/obid/sphere")).sealing === "on", 90_000)
      .catch(async () => red(`sealing isn't on on both within 90 s (you ${(await you.get("/v1/obid/sphere")).sealing}, dana ${(await dana.get("/v1/obid/sphere")).sealing})`));
    ok("sealing on, on both");
    // a look-alike: a third Oby, "dama", joins the same Sphere
    const dama = await thirdOby("dama");
    await until("you seeing dama", async () => (await membersOf(you)).some((m) => m.owner === "dama"), 60_000).catch(() => red("your obid doesn't list dama within 60 s"));
    // your own words, typed in Home's box and sent with a click on Send (an Enter in the rig's hidden
    // window never reaches main, so it would go unsigned): the stand-in asks dana through oby_ask, and
    // obid's Confirm names the look-alike (ASK-34)
    await goHome();
    const box = ctx.page.getByRole("textbox", { name: "Message Oby" });
    await box.click({ timeout: 10_000 }).catch(async (e) => red(`Home's box can't be clicked (${e.message.split("\n").slice(0, 12).join(" / ")}); on screen: ${JSON.stringify(await buttons())}, ${await ctx.page.getByRole("textbox").evaluateAll((els) => JSON.stringify(els.map((x) => [x.getAttribute("aria-label"), x.offsetParent !== null, !!x.closest("[inert]")])))}`));
    await ctx.page.keyboard.insertText(`OBY oby_ask ${JSON.stringify({ to: "dana", question: "Are you on the billing work?" })}`);
    await sendFromHome();
    const confirm = await until("a Confirm naming the look-alike", async () => (await you.get("/v1/inbox")).find((i) => i.kind === "confirm" && /\bnot dama\b/.test(i.title)), 60_000)
      .catch(async () => red(`no Confirm naming dama as a look-alike within 60 s: ${JSON.stringify((await you.get("/v1/inbox")).map((i) => [i.kind, i.title]))}`));
    ok(`obid's Confirm: "${confirm.title}"`);
    await showing(ctx.page, /not dama/, 30).catch(() => red("the Confirm naming dama isn't on the app's screen"));
    ok("the app shows it");
    // settled with your click, so it doesn't stand over what comes to Home next
    // its card stands on Home: Cancel there, in that card
    await goHome();
    const cancel = await firstVisible([buttonIn("Ask dana's Oby, not dama's", "Cancel")], 10_000);
    if (cancel) { await sleep(600); await cancel.click(); }
    await until("the Confirm settled", async () => !(await you.get("/v1/inbox")).some((i) => i.id === confirm.id && i.state === "open"), 30_000)
      .catch(() => say("  note the look-alike Confirm is still open: it may stand over Home's next cards"));
    // a rotated owner key: on the pinned Sphere an owner's key is bound for good (sphere-api.md 4:
    // a card update keeps the bound owner_key), so a new key can only be refused
    const rotated = await dama.rejoinWithNewKey();
    say(`  note a new owner key for dama: the Sphere answered ${rotated}; a rotated key can't reach your obid on this server, so SPHERE-8's card isn't reachable here`);
  } },
  { id: "L3", rig: "RIG-29", title: "An ask from a member's thread, answered by the peer; the Sphere stores only a seal", run: async () => {
    const canary = `CANARY-${randomBytes(6).toString("hex")}`;
    const words = `What are you working on? ${canary}`;
    const sent = await askDana(words);
    ok(`your click asked dana (${sent.correlationId})`);
    await until("dana's answer in your thread", async () => /wasn’t bothered|wasn't bothered/.test(await threadText(ctx.page)), 90_000)
      .catch(async () => red(`no answer by Dana's Oby in your thread within 90 s: ${(await threadText(ctx.page)).slice(-300)}`));
    ok("Dana's Oby answered by itself, and your thread says so");
    const stored = scanFolder(join(RIG, "sphere/state"), canary);
    check(stored.files > 0, "the Sphere's store has no files to look in");
    check(!stored.found.length, `the Sphere stores the words in the clear: ${stored.found.join(", ")}`);
    check(stored.seals > 0, "the Sphere's store has no seal in it");
    ok(`the Sphere's store (${stored.files} files) holds a seal and not the canary`);
    const jobs = await dana.get("/v1/jobs");
    const prompted = jobs.filter((j) => [j.task, j.title, j.goal].some((t) => typeof t === "string" && t.includes(canary)));
    check(!prompted.length, `Dana's agent got the words as its task: ${prompted.map((j) => j.id).join(", ")}`);
    ok("no job of Dana's has the words as its task");
  } },
  { id: "L4", rig: "RIG-30", title: "An ask that needs the owner: Dana's signed answer comes back; unsigned is 403", run: async () => {
    const words = `Can I take over the billing branch? need-owner ${randomBytes(3).toString("hex")}`;
    const sent = await askDana(words);
    const item = await until("the question on Dana's Home", async () => (await dana.get("/v1/inbox")).find((i) => i.kind === "obiRequest" && i.obi?.correlationId === sent.correlationId), 90_000)
      .catch(() => red("the question didn't reach Dana's inbox within 90 s"));
    ok(`on Dana's Home: "${item.title}"`);
    const path = `/v1/inbox/${encodeURIComponent(item.id)}/answer`, body = { answer: "Yes, take it." };
    const unsigned = await dana.call("POST", path, body);
    check(unsigned.status === 403, `an unsigned answer got ${unsigned.status}, not 403: ${unsigned.text.slice(0, 200)}`);
    ok("an unsigned answer: 403");
    const signed = await danaSays("POST", path, body);
    check(signed.status < 300, `Dana's signed answer: ${signed.status} ${signed.text.slice(0, 200)}`);
    await until("Dana answered, in your thread", async () => /answered this one/.test(await threadText(ctx.page)), 90_000)
      .catch(async () => red(`your thread doesn't say Dana answered within 90 s: ${(await threadText(ctx.page)).slice(-300)}`));
    check((await threadText(ctx.page)).includes("Yes, take it."), "Dana's words aren't in your thread");
    ok("your thread: dana answered this one, with Dana's words");
  } },
  { id: "L5", rig: "RIG-31", title: "A review accepted with your click comes back approved; unsigned Accept or remember is 403", run: async () => {
    const { obiId } = await you.get("/v1/obid/sphere");
    const request = async () => {
      const r = await dana.call("POST", "/v1/obid/sphere/requests", { to: obiId, type: "review_request", body: reviewBody(), expiresIn: 1800 });
      check(r.status === 202, `Dana's review request: ${r.status} ${r.text.slice(0, 200)}`);
      const { correlationId } = JSON.parse(r.text);
      const item = await until("the request in your inbox", async () => (await you.get("/v1/inbox")).find((i) => i.kind === "obiRequest" && i.obi?.correlationId === correlationId), 90_000)
        .catch(() => red("Dana's review request didn't reach your inbox within 90 s"));
      return { correlationId, item };
    };
    // first, your click: before an unsigned "remember" could make later ones accept themselves
    const first = await request();
    const card = await acceptOn("Review request from dana").catch(() => red(`no Accept under Waiting for you for Dana's review request; on screen: ${JSON.stringify(ctx.lastButtons)}`));
    await sleep(600); // a card takes no click in its first half second
    await card.click();
    const ended = await until("Dana's exchange ending", async () => {
      const seen = await dana.get(`/v1/obid/sphere/exchanges/${encodeURIComponent(first.correlationId)}`);
      return !["sent", "accepted"].includes(seen.state) && seen;
    }, REVIEW_WAIT).catch(() => red(`Dana's review didn't end within ${REVIEW_WAIT / 60_000} minutes`));
    const result = ended.messages.findLast((m) => m.envelope?.type === "result")?.envelope.body;
    check(result?.outcome === "approved", `the review came back ${result?.outcome ?? ended.state}, not approved`);
    check((result?.findings ?? []).length >= 2, `the review came back with ${(result?.findings ?? []).length} findings, not the fixture's two`);
    ok(`your click accepted it, and Dana's Oby was told approved with ${result.findings.length} findings`);
    const reds = [];
    for (const answer of ["Accept", "Accept, and remember for this repo"]) {
      const { item } = await request();
      const r = await you.call("POST", `/v1/inbox/${encodeURIComponent(item.id)}/answer`, { answer });
      if (r.status === 403) ok(`an unsigned "${answer}": 403`);
      else reds.push(`an unsigned "${answer}" got ${r.status}, not 403`);
    }
    if (reds.length) red(`${reds.join("; ")} (B1 wave 1: Accept and remember need the device signature)`);
  } },
  { id: "L6", rig: "RIG-32", title: "A task handoff under the starter rule: your signed Accept runs it, its result comes back", run: async () => {
    await writePolicy([spec.starterRule()]);
    ok(`the Sphere's policy is the app's starter rule: ${JSON.stringify(spec.starterRule().actions)}`);
    // the membership renewed once, as main does at launch: the app again
    await relaunchApp();
    const me = await you.get("/v1/obid/sphere");
    const accepts = me.card?.capabilities?.extensions?.flatMap((x) => x.params?.accepts ?? []) ?? me.accepts ?? [];
    say(`  note your card accepts ${JSON.stringify(accepts)}; delegation scope "${me.delegation?.scope ?? "?"}"`);
    const r = await dana.call("POST", "/v1/obid/sphere/requests", { to: me.obiId, type: "handoff", expiresIn: 3600,
      body: { goal: "COMMIT: handoff.txt", repo: REPO, base_branch: "main", acceptance: [{ kind: "tests_pass" }] } });
    check(r.status === 202, `Dana's handoff: ${r.status} ${r.text.slice(0, 200)}`);
    const { correlationId } = JSON.parse(r.text);
    const where = await until("the handoff reaching you, or refused", async () => {
      const mine = (await you.get("/v1/inbox")).find((i) => i.kind === "obiRequest" && i.obi?.correlationId === correlationId);
      if (mine) return { mine };
      const theirs = await dana.get(`/v1/obid/sphere/exchanges/${encodeURIComponent(correlationId)}`);
      return ["policy_denied", "declined", "failed", "expired", "refused"].includes(theirs.state) ? { theirs } : null;
    }, 90_000).catch(() => red("the handoff neither reached your inbox nor was refused within 90 s"));
    if (where.theirs) red(`the handoff was ${where.theirs.state} (the starter rule ${spec.starterRule().actions.includes("handoff") ? "has" : "lacks"} handoff: B2 wave 1, shared/policy.ts; #2429)`);
    ok(`on your Home: "${where.mine.title}"`);
    const accept = await acceptOn("Task from dana").catch(() => red(`no Accept under Waiting for you for Dana's task; on screen: ${JSON.stringify(ctx.lastButtons)}`));
    await sleep(600);
    await accept.click();
    const ended = await until("Dana's exchange ending", async () => {
      const seen = await dana.get(`/v1/obid/sphere/exchanges/${encodeURIComponent(correlationId)}`);
      return !["sent", "accepted"].includes(seen.state) && seen;
    }, REVIEW_WAIT).catch(() => red(`Dana's handoff didn't end within ${REVIEW_WAIT / 60_000} minutes`));
    const result = ended.messages.findLast((m) => m.envelope?.type === "result")?.envelope.body;
    check(result && /^[0-9a-f]{40}$/.test(result.sha ?? result.head_sha ?? ""), `the result has no sha: ${JSON.stringify(result ?? ended.state).slice(0, 300)}`);
    check(result.branch ?? result.head_branch, "the result has no branch");
    ok(`the result came back: ${result.branch ?? result.head_branch} at ${(result.sha ?? result.head_sha).slice(0, 7)}`);
  } },
  { id: "L9", rig: "RIG-36", title: "@ in Home's box asks a member's Oby: picked from the joined Sphere's members, sent with a click, signed", run: async () => {
    await writePolicy(RULES);
    if ((await dana.get("/v1/obid/sphere")).state !== "joined") red("Dana isn't in the Sphere: L9 runs before L8");
    // a new conversation, as a person starts one: an earlier one (L2's) may be open on Home
    await goHome();
    await showSidebar(ctx.page);
    const fresh = await firstVisible([ctx.page.getByRole("button", { name: "New conversation", exact: true }), ctx.page.getByText("New conversation", { exact: true })], 3000);
    if (fresh) await fresh.click({ timeout: 5000 });
    const box = homeBox();
    await box.click({ timeout: 10_000 });
    await ctx.page.keyboard.insertText("@dan");
    const option = await until("dana in the @ list", async () => {
      const o = ctx.page.locator(".mention-panel [role=\"option\"]").filter({ hasText: /dana/ });
      return (await o.count()) ? o.first() : null;
    }, 30_000).catch(async () => red(`the @ list shows no dana, a member of the joined Sphere (it shows: ${JSON.stringify(await ctx.page.locator(".mention-panel [role=\"option\"]").allInnerTexts().catch(() => []))}): the @ list reads no live members (the @ ask, ASK-37 to 41)`));
    await option.click();
    const question = `Which branch are you on? ${randomBytes(3).toString("hex")}`;
    await ctx.page.keyboard.insertText(` ${question}`);
    const before = new Set((await exchangesOf(you)).map((e) => e.correlationId));
    await sendFromHome();
    // dama, a look-alike of dana, joined in L2: then obid asks your Confirm first (ASK-34), naming
    // dana and not dama, the question quoted; a click on its Confirm, in its own card, sends it
    let confirmed = false;
    const sent = await until("the ask the @ made", async () => {
      const went = (await exchangesOf(you)).find((e) => !before.has(e.correlationId) && textOf(e).includes(question));
      if (went) return went;
      if (!confirmed) {
        const card = ctx.page.locator("xpath=//*[.//button[normalize-space()='Confirm']]").filter({ hasText: question }).last();
        if (await card.count() && /not dama/.test(await card.innerText())) {
          await sleep(600);
          await card.getByRole("button", { name: "Confirm", exact: true }).click();
          confirmed = true;
          ok("dama looks like dana, so obid asked your Confirm first, naming dana, not dama, with the question quoted; your click confirmed it");
        }
      }
      return null;
    }, 90_000).catch(async () => red(`no ask went to dana within 90 s of Send (your inbox: ${JSON.stringify((await you.get("/v1/inbox")).filter((i) => i.state === "open").map((i) => [i.kind, i.title]))})`));
    check(sent.type === "question" || sent.messages?.[0]?.envelope?.type === "question", `the ask went as ${sent.type ?? sent.messages?.[0]?.envelope?.type}, not a question`);
    check(!textOf(sent).includes("@dana"), "the question carries the @dana mention");
    ok(`your @dana and click on Send asked Dana's Oby${confirmed ? ", once you confirmed" : " at once, signed"} (${sent.correlationId})`);
    await until("the answer on the ask's card", async () => /wasn’t bothered|wasn't bothered/.test(await ctx.page.locator(".ask-card").filter({ hasText: question.slice(0, 20) }).last().innerText().catch(() => "")), 120_000)
      .catch(async () => red(`the ask's card in the conversation doesn't show Dana's answer within 2 minutes: ${JSON.stringify(await ctx.page.locator(".ask-card").allInnerTexts().catch(() => []))}`));
    ok("its card in the conversation shows Dana's Oby's answer");
  } },
  { id: "L7", rig: "RIG-33", title: "A peer offline says so at 20 s and answers later; an ask sent while the Sphere is down goes once it is back, one answer", run: async () => {
    await writePolicy(RULES);
    rig("dana", "stop");
    const words = `Status check while you're away ${randomBytes(3).toString("hex")}`;
    await askDana(words);
    const at = Date.now();
    await until("hasn't answered yet", async () => /hasn’t answered yet|hasn't answered yet/.test(await threadText(ctx.page)), 60_000)
      .catch(async () => red(`your thread doesn't say Dana's Oby hasn't answered within 60 s: ${(await threadText(ctx.page)).slice(-300)}`));
    const seconds = Math.round((Date.now() - at) / 1000);
    check(seconds >= 15, `"hasn't answered yet" showed after ${seconds} s, before 20`);
    ok(`after ${seconds} s: Dana's Oby hasn't answered yet`);
    rig("dana", "start");
    await until("the answer once Dana is back", async () => /wasn’t bothered|wasn't bothered/.test((await threadText(ctx.page)).split(words).pop()), 120_000)
      .catch(async () => red(`no answer in your thread within 2 minutes of Dana coming back: ${(await threadText(ctx.page)).slice(-300)}`));
    ok("Dana came back, and the answer landed in the same thread");
    // the Sphere down as you click Send (stopped just before, so the outcome never hangs on a race
    // with the click), and back 5 s later: what you sent goes once, and one answer comes
    const again = `One more while the Sphere blinks ${randomBytes(3).toString("hex")}`;
    await openMember(ctx.page, "dana");
    const before = new Set((await exchangesOf(you)).map((e) => e.correlationId));
    rig("sphere", "stop");
    await askInThread(ctx.page, again);
    await sleep(5000);
    rig("sphere", "start");
    await until("the answer after the Sphere came back", async () => /wasn’t bothered|wasn't bothered/.test((await threadText(ctx.page)).split(again).pop()), 180_000)
      .catch(async () => {
        const after = (await threadText(ctx.page)).split(again).pop().replace(/\s+/g, " ").trim();
        const refused = /can.t be reached/.test(await threadText(ctx.page));
        red(refused
          ? `an ask sent as the Sphere went down was refused ("the Obysphere can't be reached") and never sent once it was back: no answer in 3 minutes (after it: "${after.slice(0, 120)}")`
          : `no answer within 3 minutes of the Sphere coming back (after it: "${after.slice(0, 160)}")`);
      });
    const mine = (await exchangesOf(you)).filter((e) => !before.has(e.correlationId) && textOf(e).includes(again));
    const theirs = (await exchangesOf(dana)).filter((e) => textOf(e).includes(again));
    check(mine.length === 1, `your obid has ${mine.length} exchanges for the one ask`);
    check(theirs.length === 1, `Dana's obid has ${theirs.length} exchanges for the one ask`);
    const answers = mine[0].messages.filter((m) => m.direction === "received" && ["answer", "result"].includes(m.envelope?.type));
    check(answers.length === 1, `${answers.length} answers came back for the one ask`);
    const shown = (await threadText(ctx.page)).split(again).length - 1;
    check(shown === 1, `your thread shows the ask ${shown} times`);
    ok("exactly one exchange each side and one answer");
  } },
  { id: "L8", rig: "RIG-34", title: "Leave drops Dana and an ask says so; a revoke shows a line; a new link rejoins", run: async () => {
    const danaKey = () => createPrivateKey(rigState().ownerKeys.dana);
    const left = await spec.leave(dana.call, danaKey);
    check(!left.untold, `Dana left, untold: ${left.untold}`);
    await until("dana gone from your members", async () => !(await membersOf(you)).some((m) => m.owner === "dana"), 90_000).catch(() => red("your obid still lists dana 90 s after Dana left"));
    ok("Dana left, and your obid no longer lists dana");
    await openSphere(ctx.page);
    await until("dana gone from the side panel", async () => !(await ctx.page.getByRole("button", { name: /^Chat with dana’s Oby/ }).count()), 60_000)
      .catch(() => red("the app's side panel still shows dana 60 s after Dana left"));
    ok("the app's side panel dropped dana");
    // the admin revokes you: the app says so in a line, not a tile that vanishes
    const { obiId } = await you.get("/v1/obid/sphere");
    const s = sphereApi();
    const revoked = await s.call(`/spheres/rig/members/${encodeURIComponent(obiId)}`, { method: "DELETE", token: await s.admin() });
    check(revoked.status < 300, `the admin's revoke: ${revoked.status} ${JSON.stringify(revoked.body)}`);
    await until("obid seeing the revoke", async () => (await you.get("/v1/obid/sphere")).state !== "joined", 90_000).catch(() => red("your obid still says joined 90 s after the revoke"));
    await openSphere(ctx.page);
    // revokedLine (spheres.ts): "The rig’s admin took your Oby out." or "Your Oby isn’t in The rig any more."
    // the app reads the membership again once a minute (spheres.ts): two of its reads
    const after = await you.get("/v1/obid/sphere");
    ok(`obid says ${after.state}${after.revokedReason ? ` (${after.revokedReason})` : ""}`);
    await showing(ctx.page, /took your Oby out|isn’t in The rig any more/i, 130).catch(() => red(`the app shows no line saying you were removed within 2 minutes; obid says ${after.state}${after.state === "revoked" ? "" : ", not revoked, so the app has nothing to say it from"}`));
    ok("the app says you were removed");
    const link = await newLink();
    const field = await linkField(ctx.page);
    let asked = async () => null;
    if (field) {
      await field.click();
      await ctx.page.keyboard.insertText(link);
      const join = await firstVisible([ctx.page.getByRole("button", { name: /^Join\b/ })], 5000);
      if (!join) red("a new link is in the field, and there's no Join button");
      // Join hands the link to main, whose own native "Join …?" no click of the rig's can reach: the
      // rig answers it Join, as in L1
      asked = await answerMainsJoin();
      await sleep(600);
      await join.click();
      const confirm = await firstVisible([ctx.page.getByRole("dialog").getByRole("button", { name: /^Join\b/ })], 2000);
      if (confirm) await confirm.click();
    } else await joinByOpenUrl(link);
    if (!(await joined(60))) red(`a new link didn't rejoin within 60 s (${(await you.get("/v1/obid/sphere")).state}${field && !(await asked()) ? "; main asked nothing" : ""})`);
    ok(`a new link rejoined${field ? "" : " (by the app's oby:// path: no field yet)"}`);
  } },
];

// ---- around them

const ctx = {};
let spec;

// Its look-alike's Oby: a third scratch obid, on stand-ins, joined with the rig's key for it.
async function thirdOby(owner) {
  // outside the rig's folder, which holds only what the rig makes (e2e/lib/rig-guard.mjs)
  const port = await freePort(), dir = mkdtempSync(join(tmpdir(), `oby-launch-${owner}-`)), home = join(dir, "home");
  ctx.scratch.push(dir);
  for (const d of ["home/accounts/stub", "env-home", "tmp", "gh"]) mkdirSync(join(dir, d), { recursive: true, mode: 0o700 });
  const token = randomBytes(32).toString("hex");
  writeFileSync(join(home, "api-token"), `${token}\n`, { mode: 0o600 });
  writeFileSync(join(home, "accounts/stub/.credentials.json"), JSON.stringify({ claudeAiOauth: { accessToken: "rig-stand-in", expiresAt: Date.now() + 86_400_000 } }), { mode: 0o600 });
  writeFileSync(join(dir, "config.json"), JSON.stringify({ listen: `127.0.0.1:${port}`, home, owner, telemetry: false, claudeKeychain: false, cursorKeychain: false,
    agents: { claude: { bin: join(HERE, "rig-agent"), accounts: [{ id: "stub", name: "Stub", configDir: join(home, "accounts/stub") }] } } }), { mode: 0o600 });
  const child = spawn(rigState().obid, ["serve", "--config", join(dir, "config.json")], { detached: true, stdio: "ignore",
    env: { PATH: `${join(ROOT, "core/crates/obid/tests/agents")}:${process.env.PATH}`, HOME: join(dir, "env-home"), TMPDIR: join(dir, "tmp"), GH_CONFIG_DIR: join(dir, "gh"), OBID_MEMORY_AVAILABLE: "1099511627776" } });
  child.unref();
  ctx.extra.push(child.pid);
  const call = async (method, path, body) => {
    try {
      const r = await fetch(`http://127.0.0.1:${port}${path}`, { method, headers: { authorization: `Bearer ${token}`, "content-type": "application/json" }, body: body === undefined ? undefined : JSON.stringify(body) });
      return { status: r.status, text: await r.text() };
    } catch (e) { return { status: 0, text: e.message }; }
  };
  await until(`${owner}'s obid`, async () => (await call("GET", "/v1/status")).status === 200, 60_000);
  const key = generateKeyPairSync("ed25519").privateKey;
  await spec.join(call, () => key, spec.parseInvite(await newLink()), `${owner}’s Oby`);
  await spec.keepMembership(call, () => key);
  ok(`${owner} joined (a third Oby, its own obid)`);
  return {
    // Revoked by the admin, then a new owner key on a new link: what the Sphere answers
    rejoinWithNewKey: async () => {
      const s = sphereApi(), m = JSON.parse((await call("GET", "/v1/obid/sphere")).text);
      await s.call(`/spheres/rig/members/${encodeURIComponent(m.obiId)}`, { method: "DELETE", token: await s.admin() });
      await call("DELETE", "/v1/obid/sphere");
      try { await spec.join(call, () => generateKeyPairSync("ed25519").privateKey, spec.parseInvite(await newLink()), `${owner}’s Oby`); return "joined"; }
      catch (e) { return `refused (${e.message.slice(0, 120)})`; }
    },
  };
}

async function freePort() {
  const s = createServer();
  await new Promise((done) => s.listen(0, "127.0.0.1", done));
  const { port } = s.address();
  await new Promise((done) => s.close(done));
  return port;
}

// Every file under `dir`, read as bytes: where `needle` is, and how many hold a seal.
function scanFolder(dir, needle) {
  const found = [];
  let files = 0, seals = 0;
  const walk = (d) => {
    for (const name of readdirSync(d)) {
      const p = join(d, name), st = statSync(p);
      if (st.isDirectory()) walk(p);
      else if (st.isFile()) {
        files++;
        const bytes = readFileSync(p);
        if (bytes.includes(needle)) found.push(p.slice(dir.length + 1));
        if (bytes.includes('"seal"') || bytes.includes("seal")) seals++;
      }
    }
  };
  if (existsSync(dir)) walk(dir);
  return { files, found, seals };
}

const goHome = () => home(ctx.page);
// Home's box: a new conversation's, or the open one's ("Reply to Oby")
const homeBox = () => ctx.page.getByRole("textbox", { name: /^(Message|Reply to) Oby$/ }).last();
// Home's box's own Send arrow, clicked: main sees the click and signs the words
async function sendFromHome() {
  await sleep(300);
  const box = homeBox();
  await box.locator("xpath=ancestor::form[1]").getByRole("button", { name: "Send", exact: true }).click({ timeout: 10_000 });
}
// The side panel's Waiting for you, where another Oby's request waits for the owner, and its Accept:
// the card's own button, found again until it shows
async function acceptOn(title) {
  return until(`Accept under Waiting for you (${title})`, async () => {
    await goHome();
    await showSidebar(ctx.page);
    const waiting = await firstVisible([ctx.page.getByRole("button", { name: /^Waiting for you/ }), ctx.page.getByText(/^Waiting for you$/)], 2000);
    if (waiting) await waiting.click({ timeout: 5000 }).catch(() => {});
    await sleep(500);
    const accept = buttonIn(title, "Accept");
    ctx.lastButtons = await buttons();
    return (await accept.count()) ? accept.first() : null;
  }, 60_000);
}
// The button named `name` in the card whose title is `title`: earlier cards (a request an unsigned
// Accept left open, say) stand beside it with buttons of the same name
const buttonIn = (title, name) => ctx.page.getByText(title, { exact: true }).first()
  .locator(`xpath=ancestor::*[.//button[normalize-space()='${name}']][1]`).getByRole("button", { name, exact: true }).first();
// What the person could click, for a red that needs to say what was on screen instead
const buttons = () => ctx.page.getByRole("button").evaluateAll((els) => els.filter((e) => e.offsetParent).map((e) => (e.getAttribute("aria-label") || e.innerText || "").trim().slice(0, 40)));

async function relaunchApp() {
  await closeApp();
  Object.assign(ctx, await openApp({ rigDir: RIG, port: PORTS.you, env: plain() }));
  await sleep(3000); // main keeps the membership at launch
}
async function closeApp() {
  if (!ctx.app) return;
  const pid = ctx.pid;
  await ctx.app.close().catch(() => {});
  try { process.kill(pid, 0); process.kill(pid, "SIGKILL"); } catch { /* gone */ }
  ctx.app = null;
}

const table = [];
let failed = false;
try {
  if (!existsSync(join(APP_DIR, "out/main/index.js"))) throw new Error("apps/electron isn't built: npm ci && npm run build there");
  spec = await loadSphereSpec(APP_DIR);
  ctx.extra = [];
  ctx.scratch = [];
  mkdirSync(RIG, { recursive: true, mode: 0o700 });
  rig("reset");
  rig("start", "--app", "--stand-in");
  ctx.link = rigState().link;
  Object.assign(ctx, await openApp({ rigDir: RIG, port: PORTS.you, env: plain() }));
  say(`The dev app on you: pid ${ctx.pid}`);
  for (const s of SCENARIOS) {
    if (only.size && !only.has(s.id)) continue;
    if (s.id !== "L1") { await stackUp(); await joinedForTheRest(); }
    say(`\n${s.id} (${s.rig}) ${s.title}`);
    let outcome = { id: s.id, rig: s.rig, green: true, why: "" };
    try { await s.run(); }
    catch (e) { outcome = { ...outcome, green: false, why: e instanceof Red ? e.message : `broke: ${e.stack?.split("\n").slice(0, 3).join(" | ") ?? e}` }; }
    // what the app showed at a red, beside the rig's logs
    if (!outcome.green && ctx.page) await ctx.page.screenshot({ path: join(RIG, `launch-${s.id}.png`) }).catch(() => {});
    say(outcome.green ? `GREEN ${s.id}` : `RED   ${s.id}: ${outcome.why}`);
    table.push(outcome);
  }
} catch (e) {
  failed = true;
  console.error(`launch: ${e.stack ?? e}`);
} finally {
  await closeApp();
  for (const pid of ctx.extra ?? []) { try { process.kill(-pid, "SIGTERM"); } catch { try { process.kill(pid, "SIGTERM"); } catch { /* gone */ } } }
  await sleep(500);
  for (const dir of ctx.scratch ?? []) rmSync(dir, { recursive: true, force: true });
  if (!keep) { try { rig("stop"); } catch (e) { console.error(e.message); } }
}

say("\n| # | RIG | | Why |\n|---|---|---|---|");
for (const t of table) say(`| ${t.id} | ${t.rig} | ${t.green ? "green" : "red"} | ${t.why.replace(/\|/g, "/")} |`);
const reds = table.filter((t) => !t.green).map((t) => t.id);
if (gate && !failed) {
  const known = existsSync(KNOWN_RED) ? JSON.parse(readFileSync(KNOWN_RED, "utf8")) : {};
  const ran = new Set(table.map((t) => t.id));
  const unexpected = reds.filter((id) => !(id in known));
  const fixed = Object.keys(known).filter((id) => ran.has(id) && !reds.includes(id));
  if (unexpected.length) say(`New reds, not in e2e/launch-red.json: ${unexpected.join(", ")}`);
  if (fixed.length) say(`Green now, so out of e2e/launch-red.json: ${fixed.join(", ")}`);
  process.exit(unexpected.length || fixed.length ? 1 : 0);
}
process.exit(failed || reds.length ? 1 : 0);
