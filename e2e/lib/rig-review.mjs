// Dana's Oby asking yours for a review on the test rig (docs/contracts/test-rig.md, RIG-25): what
// e2e/rig.mjs does with the two obids, apart from it, so e2e/rig.test.mjs drives it against
// stand-ins of their APIs.
import { createHash, createPrivateKey, sign } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// The Sphere's rule: alex and dana ask each other questions and for reviews, in any repository.
export const RULES = [["alex", "dana"], ["dana", "alex"]].map(([from_owner, to_owner]) => ({ id: `${from_owner}-asks-${to_owner}`, from_owner, to_owner, actions: ["question", "review_request"], repos: ["*"] }));

// How long Dana's request lives, and so how long `review --hold` waits for your answer.
export const REVIEW_SECONDS = 1800;

// What Dana's Oby asks yours to review: the fixture's feature/average head, with the canary the stub
// agent checks never reaches a prompt (e2e/stub-agent/stub-agent).
export const REVIEW_REQUEST = join(dirname(fileURLToPath(import.meta.url)), "../fixtures/messages/review_request.example.json");
export const reviewBody = () => JSON.parse(readFileSync(REVIEW_REQUEST, "utf8")).body;

// X-Oby-Words for one request's exact body, as the app's main signs the owner's words
// (main/words.ts). `device` is the rig's, {id, key} with its key in PEM.
export function wordsOf(device, method, path, body, signedAt = new Date().toISOString()) {
  const digest = createHash("sha256").update(body).digest("hex");
  const signature = sign("sha256", Buffer.from(`oby-words\n${method}\n${path}\n${digest}\n${signedAt}`), createPrivateKey(device.key)).toString("base64");
  return `${device.id} ${signedAt} ${signature}`;
}

// The owner's tap on an open `confirm` item's Confirm, as the app's main signs it (main/device.ts).
export function confirmOf(device, item, signedAt = new Date().toISOString().replace(/\.\d+Z$/, "Z")) {
  const signature = sign("sha256", Buffer.from(`oby-confirm\n${item.id}\n${item.title}\n${signedAt}`), createPrivateKey(device.key)).toString("base64");
  return { answer: "Confirm", approval: { device: device.id, action: item.title, signedAt, signature } };
}

const fail = (line) => { throw new Error(line); };

// Accept with your words signed. When obid wants your tap on a Confirm card in its place, the
// rig's device confirms that, as the app's main does.
async function accept({ you, device, say }, item) {
  const path = `/v1/inbox/${encodeURIComponent(item.id)}/answer`, body = { answer: "Accept" };
  const r = await you.call("POST", path, body, { "x-oby-words": wordsOf(device, "POST", path, JSON.stringify(body)) });
  if (r.status !== 403) return r.status < 300 ? say("You said Accept.") : fail(`Your Accept: ${r.status} ${r.text}`);
  const card = (await you.get("/v1/inbox")).find((i) => i.id === item.id && i.kind === "confirm" && i.state !== "done");
  if (!card) fail(`Your Accept: ${r.status} ${r.text}`);
  const yes = await you.call("POST", path, confirmOf(device, card));
  if (yes.status >= 300) fail(`Your Confirm of "${card.title}": ${yes.status} ${yes.text}`);
  say(`You said Accept, and confirmed "${card.title}".`);
}

// Dana's Oby asks yours to review `body`. Your Accept goes as the app's does on your click, unless
// `hold` leaves it to you in the app. Then what Dana's Oby was told: its outcome, or an error
// unless it's approved. `you` and `dana` are {call, get} on each obid; `until(what, probe,
// seconds)` waits for a probe to return something.
export async function review({ you, dana, device, body, hold, until, say }) {
  const { obiId } = await you.get("/v1/obid/sphere");
  const sent = await dana.call("POST", "/v1/obid/sphere/requests", { to: obiId, type: "review_request", body, expiresIn: REVIEW_SECONDS });
  if (sent.status !== 202) fail(`Dana's request: ${sent.status} ${sent.text}`);
  const { correlationId } = JSON.parse(sent.text);
  say(`Dana's Oby asked yours to review ${body.repo} at ${body.head_sha.slice(0, 7)}: exchange ${correlationId}`);
  const item = await until("the request in your inbox", async () => (await you.get("/v1/inbox")).find((i) => i.kind === "obiRequest" && i.obi?.correlationId === correlationId));
  say(`Your inbox: "${item.title}" (${item.id})`);
  if (hold) say("Held: answer it in the dev app (node e2e/rig.mjs app, opened before: this command holds the rig until the exchange ends).");
  else await accept({ you, device, say }, item);
  const ended = await until("Dana's exchange ending", async () => {
    const seen = await dana.get(`/v1/obid/sphere/exchanges/${encodeURIComponent(correlationId)}`);
    return !["sent", "accepted"].includes(seen.state) && seen;
  }, hold ? REVIEW_SECONDS : 300);
  const result = ended.messages.findLast((m) => m.envelope?.type === "result")?.envelope.body;
  const outcome = result?.outcome ?? ended.state;
  say(`--- Dana's exchange: ${ended.state}, ${JSON.stringify(ended.messages.map((m) => m.envelope?.type))}`);
  say(`Outcome: ${outcome}${result?.summary ? `. ${result.summary}` : ""}`);
  for (const f of result?.findings ?? []) say(`  ${f.severity} ${f.file}:${f.line} ${f.title}`);
  if (outcome !== "approved") fail(`Dana's review came back ${outcome}, not approved.`);
  return outcome;
}
