// Words the owner typed or clicked, which main signs with this Mac's device key (X-Oby-Words,
// docs/runner-api.md, "Words the owner typed"; docs/contracts/signed-words.md). The preload asks
// main to sign a request only when its words are text the owner's own keystroke or click held
// (owners); main signs only these requests, in exactly the shapes the app sends them (wordsOf),
// and only right after a real keystroke or click it saw itself (main/words.ts).

import { MODES, SANDBOX, modeLabel } from '../renderer/src/modes';

type Body = Record<string, unknown>;
const isBody = (v: unknown): v is Body => typeof v === 'object' && v !== null && !Array.isArray(v);
const text = (v: unknown): v is string => typeof v === 'string';
const strings = (v: unknown) => Array.isArray(v) && v.every(text);
// a request key (docs/contracts/receipts.md): signed with the body, never one of its words
const requestKey = (v: unknown) => text(v) && v.length >= 1 && v.length <= 128;
// only these keys, each of the kind it must be: anything else and nothing is signed
const shaped = (body: Body, keys: Record<string, (v: unknown) => boolean>) =>
  Object.entries(body).every(([k, v]) => keys[k]?.(v) ?? false);
// the label of the button that answers a permission (live.ts, DECISIONS)
export const DECISIONS: Record<string, string> = { allow: 'Allow', always: 'Always allow', deny: 'Deny' };
// Waiting for you's Allow (WAITING, Sangam's screen 17): this label also stands for a permission's allow
export const ALLOW_ONCE = 'Allow once';
// The task menu's Try again (TASKS-25): this label also stands for obid's Try-again words to a
// job, the one message whose words needn't be the text a click held (docs/runner-api.md)
export const TRY_AGAIN_LABEL = 'Try again';
export const TRY_AGAIN = 'Try again, picking up where you left off. Finish with a short summary as asked.';

// The yes to work nobody's watching using a ChatGPT login (docs/contracts/chatgpt-sign-in.md): this
// button's label, its own so a click on any other button (a permission's Allow) can't stand for it.
export const BACKGROUND_YES = 'Yes, it can';

// A secret put in or removed on Settings › Vault (docs/contracts/vault.md): its buttons' labels,
// never the value, which sits in a password box the preload never reads. The signature still
// covers the exact body, the value included; a removal has no body. Remove Secret is its own, so a
// click on any other Remove can't stand for it.
export const VAULT_ADD = 'Add Secret';
export const VAULT_REMOVE = 'Remove Secret';

// The owner's edit of an artifact (docs/contracts/artifacts-app.md, ARTIFACTSAPP-8, WORDS-23): its
// Save button's own label, which names the version it makes (`Save as v3`, `base` + 1), so no other
// button's click can stand for it; never the text, which sits in the Code tab's editor. The signature
// still covers the exact body, the text included.
export const saveAs = (base: number) => `Save as v${base + 1}`;
// An artifact imported from a file dropped into a conversation (ARTIFACTSAPP-12, WORDS-21): its Import
// button's own label, the bytes signed as they are
export const IMPORT = 'Import';
const ARTIFACT_EDIT = /^\/v1\/artifacts\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\/versions$/;

// The owner's hold on a browser tab (docs/contracts/browser-app.md, BROWSERAPP-9): Take control and
// Give back, each signed by its own button's label, an empty body. A take is also the page dialog's
// Take control to answer, and a choice's I'll choose in the page.
export const TAKE_CONTROL = 'Take control';
export const TAKE_LABELS = [TAKE_CONTROL, 'Take control to answer', 'I’ll choose in the page'];
export const GIVE_BACK = 'Give back';
const BROWSER_TAB = /^\/v1\/browser\/tabs\/[^/]+\/(take|giveBack)$/;
// An agent's ask answered (BROWSERAPP-13): a choice by its own button's label, `{choice}`; none by
// Skip (or the CAPTCHA card's Skip this page), `{}`. A password or code never goes here: Fill in
// (FILL_IN) has main put it in the page itself (main/device.ts, fillSecret), never as words.
export const SKIP = 'Skip';
const SKIP_LABELS = [SKIP, 'Skip this page'];
export const FILL_IN = 'Fill in';
const BROWSER_ANSWER = /^\/v1\/browser\/asks\/[^/]+\/answer$/;
// Settings › General's two browser switches (BROWSERAPP-18): PUT /v1/browser/settings with one key, a
// boolean, signed by that switch's own label (its aria-label: a switch has no words of its own)
export const BROWSER_SWITCHES = { agentsUseBrowser: 'Agents can use the browser', agentsUseMac: 'Agents can use this Mac' } as const;
// An agent's download copied into the owner's Downloads (BROWSERAPP-16): an empty body, by its button's label
export const SAVE_A_COPY = 'Save a copy';
const BROWSER_SAVE = /^\/v1\/browser\/downloads\/[^/]+\/saveCopy$/;

// Verify, in the dialog that asks whether a member read the owner their key's fingerprint
// (docs/contracts/obysphere.md, SPHERE-11): trusting a member's key is the owner's alone, as the
// Confirm card's is (SPHERE-9), so it signs exactly `{fingerprint}` by this button's label.
export const PIN_VERIFY = 'Verify';

// A computer removed (COMPUTERSAPP-11): no body, by its dialog's own Remove computer, never a bare
// Remove, so a click on any other Remove can't stand for it (the owner's word of 2026-11-21)
export const REMOVE_COMPUTER = 'Remove computer';
const COMPUTER = /^\/v1\/computers\/[^/]+$/;
// A computer added with a code (docs/contracts/computers-app.md, COMPUTERSAPP-6): exactly `{link,
// code}`, signed by the code the owner typed in its sheet, which Enter in its box and the sheet's
// Add computer (a click holds its form's boxes) both hold. The sheet sends its six digits alone, so
// a held text stands for it by its digits: "482 913", as the other computer shows it
const COMPUTER_ADD = '/v1/computers';
const digitsOf = (s: string) => s.replace(/\D/g, '');

// whether `words` @-name `owner` (a whole word, case aside), as the message box sets a mention apart
const named = (words: string, owner: string) =>
  /^[a-z0-9-]{1,63}$/.test(owner) && new RegExp(`(?<![\\w@])@${owner}(?![\\w-])`, 'i').test(words);

const MESSAGES = /^\/v1\/jobs\/[^/]+\/messages$/;
const ROOM = /^\/v1\/spheres\/[^/]+\/room$/;
const PEER_ASK = /^\/v1\/obid\/sphere\/peers\/[^/]+\/ask$/;
const VAULT = /^\/v1\/obid\/vault\/[^/]+$/;
const PERMISSION = /^\/v1\/jobs\/[^/]+\/permission$/;
const PIN = /^\/v1\/obid\/sphere\/pins\/[^/]+$/;

// The words a request carries, for the requests the app sends obid with the owner's words in
// them; null for any other request, or any other shape (main never signs it). A key the app
// doesn't send (a job's check, mode or repository, say) is never signed along with the words.
export function wordsOf(method: string, path: string, body: unknown): string[] | null {
  // the vault's removal and a computer's, the two requests signed with no body
  if (VAULT.test(path.split('?')[0])) {
    if (method === 'DELETE') return body === undefined ? [VAULT_REMOVE] : null;
    if (method === 'PUT') return isBody(body) && text(body.value) && shaped(body, { value: text }) ? [VAULT_ADD] : null;
  }
  if (method === 'DELETE' && COMPUTER.test(path.split('?')[0]) && path.split('?')[0] !== '/v1/computers/current') return body === undefined ? [REMOVE_COMPUTER] : null;
  // an artifact imported from a dropped file: its bytes as they are, by its own Import button (WORDS-21)
  if (method === 'POST' && path === '/v1/artifacts/import') return body instanceof Uint8Array && body.byteLength > 0 ? [IMPORT] : null;
  if (!isBody(body)) return null;
  const p = path.split('?')[0];
  // the owner's edit: the version it's made on, and some of its files' new text, nothing else
  if (method === 'POST' && ARTIFACT_EDIT.test(p)) {
    const files = body.files;
    return Number.isSafeInteger(body.base) && (body.base as number) >= 1 && isBody(files) && Object.keys(files).length > 0
      && Object.values(files).every(text) && shaped(body, { base: (b) => Number.isSafeInteger(b), files: isBody })
      ? [saveAs(body.base as number)] : null;
  }
  // a browser tab's take or give back: an empty body, nothing else
  const hold = method === 'POST' ? BROWSER_TAB.exec(p) : null;
  if (hold) return Object.keys(body).length === 0 ? [hold[1] === 'take' ? TAKE_CONTROL : GIVE_BACK] : null;
  if (method === 'PUT' && p === '/v1/browser/settings') {
    const keys = Object.keys(body);
    const key = keys[0] as keyof typeof BROWSER_SWITCHES;
    return keys.length === 1 && key in BROWSER_SWITCHES && typeof body[key] === 'boolean' ? [BROWSER_SWITCHES[key]] : null;
  }
  if (method === 'POST' && BROWSER_SAVE.test(p)) return Object.keys(body).length === 0 ? [SAVE_A_COPY] : null;
  if (method === 'POST' && BROWSER_ANSWER.test(p)) {
    if (Object.keys(body).length === 0) return [SKIP];
    return text(body.choice) && body.choice.trim() && shaped(body, { choice: text }) ? [body.choice] : null;
  }
  const id = text, flag = (v: unknown) => v === true;
  if ((method === 'POST' && /^\/v1\/jobs\/[^/]+\/mode$/.test(p)) || (method === 'PUT' && p === '/v1/settings/permissions')) {
    return text(body.mode) && [...MODES, SANDBOX].some((m) => m.id === body.mode) && shaped(body, { mode: text }) ? [modeLabel(body.mode)] : null;
  }
  if (method === 'POST' && p === '/v1/plans') {
    const answer = body.answer;
    const ok = text(body.said) && shaped(body, {
      said: text, answer: (a) => isBody(a) && text(a.question) && text(a.option) && shaped(a, { question: text, option: text }),
      conversation: id, new: flag, folders: strings, id, attachments: strings, jobs: strings,
      // the project the words start a conversation in: signed with the body, never one of the words
      project: id,
      // the box's chips (COMPOSER-3): signed with the body, never one of the words
      defaults: (d) => isBody(d) && shaped(d, { mode: text, agent: text, model: text, effort: text }),
      // the members the words @-name (ASK-39): only owners the words name, so a page can't add one
      ask: (a) => strings(a) && (a as string[]).length > 0 && (a as string[]).every((o) => named(body.said as string, o)),
      // the skill picked in the box (COMPOSER-27): signed with the body, never one of the words
      skill: text,
      // where the conversation the words start works (WORKSPACE-3, WORDS-20): "This folder" or
      // "A new worktree", with new only; signed with the body, never one of the words
      workspace: (w) => (w === 'folder' || w === 'worktree') && body.new === true,
      // the artifact open beside the conversation (ARTIFACTS-12, WORDS-22): its id and version, in
      // a conversation only; signed with the body, never one of the words
      artifact: (a) => isBody(a) && typeof a.id === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(a.id)
        && Number.isSafeInteger(a.version) && (a.version as number) >= 1 && shaped(a, { id: text, version: (v) => Number.isSafeInteger(v) }) && body.new !== true,
    });
    // an answer to Oby's question: the words it answers, and the owner's answer
    return ok ? [body.said as string, ...(isBody(answer) ? [answer.option as string] : [])] : null;
  }
  if (method === 'POST' && MESSAGES.test(p)) {
    return text(body.text) && shaped(body, { text, attachments: strings, conversation: flag, clientRequestId: requestKey, skill: text }) ? [body.text as string] : null;
  }
  // the owner's `say` in the Sphere's room: its words alone (the app sends no about or replyTo)
  if (method === 'POST' && ROOM.test(p)) return text(body.text) && shaped(body, { text }) ? [body.text as string] : null;
  // the owner's question from a member's thread: its words alone (the path says to whom)
  if (method === 'POST' && PEER_ASK.test(p)) return text(body.text) && shaped(body, { text }) ? [body.text as string] : null;
  // a member's key the owner compared: the fingerprint alone (the path says whose), by Verify
  if (method === 'POST' && PIN.test(p)) return text(body.fingerprint) && shaped(body, { fingerprint: text }) ? [PIN_VERIFY] : null;
  // the yes alone: a no needs no signature (obid keeps it either way)
  if (method === 'POST' && p === COMPUTER_ADD) return text(body.link) && text(body.code) && /^\d{6}$/.test(body.code) && shaped(body, { link: text, code: text }) ? [body.code] : null;
  if (method === 'POST' && /^\/v1\/accounts\/[^/]+\/background$/.test(p)) return body.given === true && shaped(body, { given: flag }) ? [BACKGROUND_YES] : null;
  if (method === 'PATCH' && /^\/v1\/jobs\/[^/]+$/.test(p)) return text(body.title) && shaped(body, { title: text }) ? [body.title] : null;
  if (method === 'POST' && /^\/v1\/inbox\/[^/]+\/answer$/.test(p)) return text(body.answer) && shaped(body, { answer: text, clientRequestId: requestKey }) ? [body.answer] : null;
  if (method === 'POST' && /^\/v1\/jobs\/[^/]+\/permission$/.test(p)) {
    const label = DECISIONS[body.decision as string];
    return label && shaped(body, { id, decision: text, message: text }) ? [label, ...(text(body.message) ? [body.message] : [])] : null;
  }
  return null;
}

const norm = (s: string) => s.replace(/\s+/g, ' ').trim();

// Which of `held` (a box's text or a button's label at the owner's keystroke or click) the
// request's words are, each held text used once: their indexes, or null when they aren't all
// there. An answer to Oby's question sends again the words it answers: those may be words to Oby
// signed before (`earlier`, POST /v1/plans's `said`), never anything else. Spaces don't count: the
// box's lines come back as the page joins them.
export function owners(method: string, path: string, body: unknown, held: string[], earlier: string[]): number[] | null {
  const words = wordsOf(method, path, body);
  if (!words) return null;
  const answers = isBody(body) && body.answer !== undefined && path.split('?')[0] === '/v1/plans';
  const used: number[] = [];
  for (const [i, w] of words.entries()) {
    const n = norm(w);
    if (!n) return null;
    if (answers && i === 0 && earlier.some((e) => norm(e) === n)) continue;
    const round = w === TRY_AGAIN && MESSAGES.test(path.split('?')[0]);
    const once = i === 0 && w === DECISIONS.allow && PERMISSION.test(path.split('?')[0]);
    const take = w === TAKE_CONTROL && /\/take$/.test(path.split('?')[0]);
    const skip = w === SKIP && BROWSER_ANSWER.test(path.split('?')[0]);
    const code = i === 0 && method === 'POST' && path.split('?')[0] === COMPUTER_ADD;
    const at = held.findIndex((h, j) => !used.includes(j) && (norm(h) === n || (round && norm(h) === norm(TRY_AGAIN_LABEL)) || (once && norm(h) === ALLOW_ONCE) || (take && TAKE_LABELS.some((l) => norm(l) === norm(h))) || (skip && SKIP_LABELS.some((l) => norm(l) === norm(h))) || (code && digitsOf(h) === n)));
    if (at < 0) return null;
    used.push(at);
  }
  return used;
}

// How long a keystroke or click holds its text (preload) and its one signature (main): the words
// go at once. A keyed send obid didn't answer, sent again byte for byte, main signs again, within
// 10 s of its first signature and at most twice (main/device.ts, WORDS-16); any other send that
// goes again later, or words that wait for their files to upload, go unsigned (files mark them anyway).
export const WITHIN_MS = 2_000;
