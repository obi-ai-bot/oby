// Oby's control plane (#285): the sealed push relay (#398, the owner's decision of 2026-10-23) and
// its health check, nothing else. Its sign-in, installations, tunnels and hooks were removed by
// R03-02 (docs/contracts/retired.md), and git history keeps them.
//
// The relay is open to any phone: an iPhone registers its APNs token for a pushId and pushSecret,
// and its obid posts alerts sealed to the phone's own key, signed with a key made from that secret.
// The relay sends Apple a fixed alert carrying the sealed blob: it never sees what a push says, and
// never learns which Mac sent it. obid holds only a pushKey made from the secret, so only the phone
// can re-register or forget itself, and APNs' provider token is kept in D1 only sealed
// (src/apns.ts).

import { type APNs, type APNsAnswer, apnsKey, type Send, sendPush } from "./apns";
import type { Env } from "./env";
import { type Limit, takeOne } from "./limits";
import { addressBucket, fromBase64url, networkBucket, randomBytes, sha256Hex } from "./tokens";

export interface Deps {
  /// How a push goes to Apple (`fetch`, HTTP/2 on Cloudflare's edge); a test's stand-in plays APNs.
  apns?: Send;
  /// The clock, for tests of expiry.
  now?: () => Date;
}

export const LIMITS = {
  // The push relay (#398): new phones per client, network and everyone; a phone registering its
  // token again; a token checked with Apple; and pushes per phone (by pushId and by token),
  // counted only once their MAC checks out. Their windows are whole minutes, hours or days.
  phonesPerAddress: { max: 30, windowSeconds: 60 * 60 } satisfies Limit,
  phonesPerNetwork: { max: 200, windowSeconds: 60 * 60 } satisfies Limit,
  phonesEverywhere: { max: 2000, windowSeconds: 60 * 60 } satisfies Limit,
  reRegistrationsPerPhone: { max: 20, windowSeconds: 60 * 60 } satisfies Limit,
  tokenChecksPerToken: { max: 10, windowSeconds: 60 * 60 } satisfies Limit,
  pushesPerPhoneMinute: { max: 30, windowSeconds: 60 } satisfies Limit,
  pushesPerPhoneDay: { max: 1000, windowSeconds: 24 * 60 * 60 } satisfies Limit,
};
/// The push relay (#398): the biggest registration and push bodies, and the most base64url
/// characters `sealed` may have, so Apple's payload stays under its 4 KB.
export const PUSH = { registrationMax: 1024, bodyMax: 4096, sealedMax: 3072 };
/// What every push shows until the phone's Notification Service Extension opens what's sealed.
export const PUSH_ALERT = { title: "Oby", body: "Open Oby to see it." };
/// What a pushSecret is the HMAC of, with the pushId (`pushSecret`), and a pushKey (`pushKey`).
export const PUSH_SECRET_INFO = "oby-push-secret/1";
export const PUSH_KEY_INFO = "oby push key v1";
/// How far a push's `at` may be from the relay's clock, either way.
export const PUSH_SKEW_SECONDS = 5 * 60;
/// A pushId: 24 base64url characters when made here (18 random bytes, 144 bits).
const PUSH_ID = /^[A-Za-z0-9_-]{22,64}$/;

/// An answer with an error: thrown anywhere, sent as `{"error": …}` with its status.
class Reply extends Error {
  constructor(
    readonly status: number,
    message: string,
    readonly headers: Record<string, string> = {},
    /// More fields in the answer, beside `error`.
    readonly fields: Record<string, unknown> = {},
  ) {
    super(message);
  }
}

export async function handle(request: Request, env: Env, deps: Deps): Promise<Response> {
  const now = deps.now ?? (() => new Date());
  try {
    return await route(request, env, deps, now);
  } catch (error) {
    if (error instanceof Reply) return json({ error: error.message, ...error.fields }, error.status, error.headers);
    console.error(error instanceof Error ? error.message : String(error));
    return json({ error: "Something went wrong on the control plane." }, 500);
  }
}

async function route(request: Request, env: Env, deps: Deps, now: () => Date): Promise<Response> {
  const path = new URL(request.url).pathname.replace(/\/+$/, "") || "/";
  const method = request.method;
  if (path === "/v1/health" && method === "GET") return json({ ok: true });
  if (path === "/v1/push/devices" && method === "POST") return registerPhone(request, env, deps, now);
  const phone = /^\/v1\/push\/devices\/([A-Za-z0-9_-]{22,64})$/.exec(path);
  if (phone?.[1] && method === "DELETE") return forgetPhone(request, env, phone[1]);
  if (path === "/v1/push" && method === "POST") return push(request, env, deps, now);
  throw new Reply(404, "No such route.");
}

/// A whole number from wrangler.jsonc, or the control plane refuses (fails closed) and says which.
function whole(value: string | undefined, name: string, min: number, max: number): number {
  const number = /^\d{1,9}$/.test((value ?? "").trim()) ? Number(value) : Number.NaN;
  if (!(number >= min && number <= max)) throw new Reply(503, `${name} in wrangler.jsonc isn't a whole number from ${min} to ${max}.`);
  return number;
}

/// The request's body, read as a stream and cut off past `max` bytes (null then): never buffered
/// whole when it's too large.
async function boundedBody(request: Request, max: number): Promise<Uint8Array | null> {
  if (!request.body) return new Uint8Array();
  const reader = request.body.getReader();
  const parts: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > max) {
      await reader.cancel();
      return null;
    }
    parts.push(value);
  }
  const body = new Uint8Array(size);
  let at = 0;
  for (const part of parts) {
    body.set(part, at);
    at += part.byteLength;
  }
  return body;
}

// MARK: The push relay (#398)

const notSetUp = "Pushes aren't set up on this control plane.";
const wrongSecret = "That pushId and pushSecret don't go together.";
const phoneGone = "That phone isn't registered for pushes any more.";

/// An iPhone registers its APNs token (`{apns, topic, sandbox}`, as `application/json`, so a web
/// page can't register for its visitors without a CORS preflight, which this Worker refuses) and
/// gets `{pushId, pushSecret}`; it gives its obid the pushId and the pushKey made from the secret
/// (`pushKey`). The same call with them added registers a changed token for the same id, and
/// brings back one the relay forgot: the secret proves it's that phone's. A token new to the
/// relay is checked with Apple first (one background push), so what's kept is a real phone's. A
/// token another pushId has moves to this one, so one phone never gets two of each push.
async function registerPhone(request: Request, env: Env, deps: Deps, now: () => Date): Promise<Response> {
  const apns = relayAPNs(env, deps);
  const topics = (env.APNS_TOPICS ?? "").split(",").map((topic) => topic.trim()).filter(Boolean);
  if (!topics.length) throw new Reply(503, notSetUp);
  if ((request.headers.get("Content-Type") ?? "").split(";")[0]?.trim().toLowerCase() !== "application/json") {
    throw new Reply(415, "Send the registration as application/json.");
  }
  const body = only(await smallJSON(request, PUSH.registrationMax), ["apns", "topic", "sandbox", "pushId", "pushSecret"]);
  const token = field(body.apns, "apns", /^(?:[0-9a-fA-F]{2}){32,100}$/, "the APNs device token, in hex").toLowerCase();
  if (typeof body.topic !== "string" || !topics.includes(body.topic)) throw new Reply(422, `topic is ${topics.join(" or ")}.`);
  if (typeof body.sandbox !== "boolean") throw new Reply(422, "sandbox is true or false.");
  let id: string;
  if (body.pushId !== undefined || body.pushSecret !== undefined) {
    if (typeof body.pushId !== "string" || !(await secretMatches(env, body.pushId, body.pushSecret))) throw new Reply(401, wrongSecret);
    id = body.pushId;
    await relayLimit(env.DB, "again", id, LIMITS.reRegistrationsPerPhone, now(), "This phone registered too often lately.");
  } else {
    const address = request.headers.get("CF-Connecting-IP") ?? "unknown";
    const busy = "Too many phones registered from this network lately.";
    await relayLimit(env.DB, "address", addressBucket(address), LIMITS.phonesPerAddress, now(), busy);
    await relayLimit(env.DB, "network", networkBucket(address), LIMITS.phonesPerNetwork, now(), busy);
    await relayLimit(env.DB, "everywhere", "", LIMITS.phonesEverywhere, now(), "Too many phones registered lately.");
    const max = whole(env.MAX_PUSH_DEVICES, "MAX_PUSH_DEVICES", 1, 100_000_000);
    const count = (await env.DB.prepare("SELECT n FROM push_device_count WHERE id = 1").first<{ n: number }>())?.n ?? 0;
    if (count >= max) throw new Reply(503, "This control plane can't take more phones now.");
    // Workers Logs keep it.
    if (count + 1 >= max * 0.8) console.warn(`push relay: ${count + 1} of ${max} phones, past 80% of MAX_PUSH_DEVICES`);
    id = base64url(randomBytes(18));
  }
  const known = await env.DB.prepare("SELECT 1 AS yes FROM push_devices WHERE id = ?1 AND apns_token = ?2").bind(id, token).first();
  if (!known) {
    await relayLimit(env.DB, "check", await sha256Hex(token), LIMITS.tokenChecksPerToken, now(), "This token was checked too often lately.");
    const payload = JSON.stringify({ aps: { "content-available": 1 } });
    const answer = await sendPush(apns, { token, topic: body.topic, sandbox: body.sandbox, type: "background", payload, urgent: false }, now());
    if (answer.status !== 200) {
      if (tokenGone(answer)) throw new Reply(422, "Apple doesn't know that device token for this app. Check `sandbox`.");
      appleFailed(answer, "check");
    }
  }
  await env.DB.batch([
    env.DB.prepare("DELETE FROM push_devices WHERE apns_token = ?1 AND id <> ?2").bind(token, id),
    env.DB.prepare(
      `INSERT INTO push_devices (id, apns_token, topic, sandbox) VALUES (?1, ?2, ?3, ?4)
       ON CONFLICT (id) DO UPDATE SET apns_token = excluded.apns_token, topic = excluded.topic, sandbox = excluded.sandbox`,
    ).bind(id, token, body.topic, body.sandbox ? 1 : 0),
  ]);
  return json({ pushId: id, pushSecret: await pushSecret(env, id) }, 200, noStore);
}

/// The phone forgets itself: `Authorization: Bearer <pushSecret>`. 204, also when it was gone.
/// obid holds only the pushKey, so it can't.
async function forgetPhone(request: Request, env: Env, id: string): Promise<Response> {
  pushSecretKey(env);
  const bearer = /^Bearer\s+(\S+)$/i.exec(request.headers.get("Authorization") ?? "")?.[1];
  if (!(await secretMatches(env, id, bearer))) throw new Reply(401, wrongSecret);
  await env.DB.prepare("DELETE FROM push_devices WHERE id = ?1").bind(id).run();
  return new Response(null, { status: 204 });
}

/// obid's push: `{pushId, sealed, collapse?, urgent?, at}` with `Oby-Push-Mac`, the HMAC-SHA-256
/// of the body's bytes under the pushKey's 32 raw bytes, base64url; `at` (Unix seconds, in the body, so
/// the MAC covers it) within PUSH_SKEW_SECONDS of the relay's clock. The MAC is checked before
/// anything is read or counted, so a stranger's push costs no D1 and can't spend the phone's
/// allowance. Apple gets the fixed alert with `sealed` beside `aps`; Apple saying the token is gone
/// forgets the phone (410). Nothing of a push is kept or logged but Apple's status and reason, and
/// the counts the rate limits need.
async function push(request: Request, env: Env, deps: Deps, now: () => Date): Promise<Response> {
  const apns = relayAPNs(env, deps);
  if (Number(request.headers.get("Content-Length") ?? "0") > PUSH.bodyMax) throw new Reply(413, "That push is too large.");
  const raw = await boundedBody(request, PUSH.bodyMax);
  if (!raw) throw new Reply(413, "That push is too large.");
  const body = only(parseJSON(raw), ["pushId", "sealed", "collapse", "urgent", "at"]);
  const id = field(body.pushId, "pushId", PUSH_ID, "the phone's pushId");
  const sealed = field(body.sealed, "sealed", /^[A-Za-z0-9_-]+$/, "the sealed alert, base64url");
  if (sealed.length > PUSH.sealedMax) throw new Reply(413, `sealed is at most ${PUSH.sealedMax} characters.`);
  const collapse = body.collapse === undefined ? undefined : field(body.collapse, "collapse", /^[\x21-\x7e]{1,64}$/, "1 to 64 printable characters, no spaces");
  if (body.urgent !== undefined && typeof body.urgent !== "boolean") throw new Reply(422, "urgent is true or false.");
  if (typeof body.at !== "number" || !Number.isSafeInteger(body.at)) throw new Reply(422, "at is the push's time, in Unix seconds.");
  if (!(await macMatches(env, id, request.headers.get("Oby-Push-Mac"), raw))) throw new Reply(401, "This push isn't signed with that phone's pushKey.");
  if (Math.abs(body.at - Math.floor(now().getTime() / 1000)) > PUSH_SKEW_SECONDS) throw new Reply(401, "This push's `at` is more than five minutes from the relay's clock.");
  const many = "Too many pushes to this phone.";
  await relayLimit(env.DB, "id", id, LIMITS.pushesPerPhoneMinute, now(), many);
  await relayLimit(env.DB, "id", id, LIMITS.pushesPerPhoneDay, now(), `${many} today`);
  const phone = await env.DB.prepare("SELECT apns_token, topic, sandbox FROM push_devices WHERE id = ?1").bind(id)
    .first<{ apns_token: string; topic: string; sandbox: number }>();
  if (!phone) throw new Reply(410, phoneGone);
  // Per token too: the same phone registered under new pushIds shares one allowance.
  const hashed = await sha256Hex(phone.apns_token);
  await relayLimit(env.DB, "token", hashed, LIMITS.pushesPerPhoneMinute, now(), many);
  await relayLimit(env.DB, "token", hashed, LIMITS.pushesPerPhoneDay, now(), `${many} today`);
  const payload = JSON.stringify({ aps: { alert: PUSH_ALERT, "mutable-content": 1, sound: "default" }, sealed });
  const answer = await sendPush(apns, { token: phone.apns_token, topic: phone.topic, sandbox: phone.sandbox === 1, type: "alert", payload, urgent: body.urgent === true, collapse }, now());
  if (answer.status === 200) return json({ ok: true });
  if (tokenGone(answer)) {
    console.log(`push: APNs ${answer.status} ${answer.reason ?? ""}, phone forgotten`);
    // Only that token: the phone may have registered a new one meanwhile.
    await env.DB.prepare("DELETE FROM push_devices WHERE id = ?1 AND apns_token = ?2").bind(id, phone.apns_token).run();
    throw new Reply(410, phoneGone);
  }
  appleFailed(answer, "push");
}

/// Apple saying a device token is gone: the app was deleted, or it isn't this app's or this
/// environment's.
function tokenGone(answer: APNsAnswer): boolean {
  return answer.status === 410 || (answer.status === 400 && (answer.reason === "BadDeviceToken" || answer.reason === "DeviceTokenNotForTopic"));
}

/// Any other answer than 200, logged by its status and reason only, as an error to send back:
/// the relay's own problems (its provider token renewed too often, Apple down) apart from the
/// phone's (too many pushes to it).
function appleFailed(answer: APNsAnswer, what: "push" | "check"): never {
  console.log(`${what}: APNs ${answer.status || "didn't answer"}${answer.reason ? ` ${answer.reason}` : ""}`);
  if (answer.status === 429 && answer.reason === "TooManyProviderTokenUpdates") throw new Reply(503, "The relay's sign-in with Apple was renewed too often. Try again in a while.");
  if (answer.status === 429) throw new Reply(429, "Apple says too many pushes to this phone. Try again later.", answer.retryAfter ? { "Retry-After": answer.retryAfter } : {});
  if (answer.status === 0 || answer.status >= 500) throw new Reply(502, "Apple didn't answer. Try again later.");
  throw new Reply(502, `Apple refused the ${what === "push" ? "push" : "token check"}${answer.reason ? `: ${answer.reason}` : ""}.`);
}

/// What sending to Apple needs, or the relay is off (503).
function relayAPNs(env: Env, deps: Deps): APNs {
  const secret = pushSecretKey(env);
  const key = apnsKey(env);
  if (!key) throw new Reply(503, notSetUp);
  return { send: deps.apns ?? ((url: string, init: RequestInit) => fetch(url, init)), key, db: env.DB, secret };
}

/// One of the relay's rate limits: its window aligned to whole minutes, hours or days, and its
/// window's length in the key, so the sweep removes it as soon as it ends (`sweep`). It keeps a
/// count and which window it counts, never when a push or registration came.
async function relayLimit(db: D1Database, what: string, of: string, rule: Limit, now: Date, said: string): Promise<void> {
  const verdict = await takeOne(db, `push:${rule.windowSeconds}:${what}:${of}`, rule, now, true);
  if (verdict.allowed) return;
  const minutes = Math.ceil(verdict.retryAfterSeconds / 60);
  throw new Reply(429, `${said} Try again in ${minutes} minute${minutes === 1 ? "" : "s"}.`, { "Retry-After": String(verdict.retryAfterSeconds) });
}

/// PUSH_SECRET_KEY, or the relay is off (503).
function pushSecretKey(env: Env): string {
  const key = env.PUSH_SECRET_KEY?.trim() ?? "";
  if (key.length < 32) throw new Reply(503, notSetUp);
  return key;
}

/// HMAC-SHA-256 of `message` under the raw bytes `key`, base64url with no padding. A pushSecret's
/// or a pushKey's raw bytes are its base64url decoded (32 bytes), never its text.
async function hmac(key: Uint8Array, message: Uint8Array): Promise<string> {
  const imported = await crypto.subtle.importKey("raw", key, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return base64url(new Uint8Array(await crypto.subtle.sign("HMAC", imported, message)));
}

/// A phone's pushSecret: the HMAC of PUSH_SECRET_INFO, a newline and its pushId, under
/// PUSH_SECRET_KEY's text (the relay's own, so no one else makes it), made again whenever it's
/// needed, so D1 keeps no secret.
async function pushSecret(env: Env, id: string): Promise<string> {
  return hmac(new TextEncoder().encode(pushSecretKey(env)), new TextEncoder().encode(`${PUSH_SECRET_INFO}\n${id}`));
}

/// The pushKey the phone gives obid: the HMAC of PUSH_KEY_INFO under the pushSecret's 32 raw
/// bytes. obid signs pushes with it, and can't register or forget the phone.
async function pushKey(env: Env, id: string): Promise<string> {
  return hmac(fromBase64url(await pushSecret(env, id))!, new TextEncoder().encode(PUSH_KEY_INFO));
}

/// Whether two texts are the same, compared in constant time.
function sameText(given: string, expected: string): boolean {
  const a = new TextEncoder().encode(given);
  const b = new TextEncoder().encode(expected);
  return a.byteLength === b.byteLength && crypto.subtle.timingSafeEqual(a, b);
}

/// Whether `secret` is `id`'s pushSecret.
async function secretMatches(env: Env, id: string, secret: unknown): Promise<boolean> {
  if (typeof secret !== "string" || !PUSH_ID.test(id)) return false;
  return sameText(secret, await pushSecret(env, id));
}

/// Whether `mac` is the HMAC of `body` under `id`'s pushKey's 32 raw bytes.
async function macMatches(env: Env, id: string, mac: string | null, body: Uint8Array): Promise<boolean> {
  if (!mac) return false;
  return sameText(mac, await hmac(fromBase64url(await pushKey(env, id))!, body));
}

/// A JSON object of at most `max` bytes (413 past it).
async function smallJSON(request: Request, max: number): Promise<Record<string, unknown>> {
  if (Number(request.headers.get("Content-Length") ?? "0") > max) throw new Reply(413, "That's too large.");
  const raw = await boundedBody(request, max);
  if (!raw) throw new Reply(413, "That's too large.");
  return parseJSON(raw);
}

/// `body`, when it has no field but `names` (422 otherwise).
function only(body: Record<string, unknown>, names: string[]): Record<string, unknown> {
  if (Object.keys(body).some((name) => !names.includes(name))) throw new Reply(422, `This call takes only ${names.join(", ")}.`);
  return body;
}

// MARK: The sweep

/// Cron's job, every minute (wrangler.jsonc): rate-limit windows a day old go, and the push relay's
/// as soon as they end. It asks nothing of anyone.
export async function sweep(env: Env, deps: Deps): Promise<void> {
  const now = (deps.now ?? (() => new Date()))();
  const seconds = Math.floor(now.getTime() / 1000);
  await env.DB.batch([
    // Every window is an hour or less: a day-old one counts nothing.
    env.DB.prepare("DELETE FROM rate_limits WHERE window_start < ?1").bind(seconds - 86_400),
    // The push relay's windows (`relayLimit`) go once they end, by their keys' range.
    ...[60, 3_600, 86_400].map((window) =>
      env.DB.prepare("DELETE FROM rate_limits WHERE key >= ?1 AND key < ?2 AND window_start <= ?3").bind(`push:${window}:`, `push:${window};`, seconds - window),
    ),
  ]);
}

// MARK: Shapes and checks

function field(value: unknown, name: string, shape: RegExp, said: string): string {
  if (typeof value !== "string" || !shape.test(value)) throw new Reply(422, `${name} is ${said}.`);
  return value;
}

function parseJSON(bytes: Uint8Array): Record<string, unknown> {
  try {
    const body = JSON.parse(new TextDecoder().decode(bytes));
    if (body && typeof body === "object" && !Array.isArray(body)) return body as Record<string, unknown>;
  } catch {
    // Said below.
  }
  throw new Reply(400, "Send a JSON object.");
}

const noStore = { "Cache-Control": "no-store" };

function base64url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json", ...headers } });
}

