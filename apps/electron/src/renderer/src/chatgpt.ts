import { REAL_CHATGPT_SIGN_IN_REPORT } from './chatgptGate';
import { api, type Status } from './obid';

// Continue with ChatGPT in the app (docs/contracts/chatgpt-sign-in.md): obid holds the tokens, and
// this only opens its page and shows how it stands. These words are OpenAI's (its UI/UX guidelines);
// where they show is Sangam's.
export const CONTINUE = 'Continue with ChatGPT';
export const USING_PLAN = 'Using ChatGPT plan';
export const MANAGE_USAGE = 'Manage usage';
export const LIMIT_REACHED = 'Usage limit reached';
export const LIMIT_WORDS = 'Review your plan or this app’s limit in ChatGPT settings.';
export const USAGE_URL = 'https://chatgpt.com/settings/usage';
export const manageUsage = () => window.app?.openExternal(USAGE_URL);
// Off obid, Sangam's mock: its stand-in sign-in shows the welcome
export const MOCK_WELCOME = 'oby:chatgpt-welcome';

// A new ChatGPT sign-in is offered in a development build, so the real one can be run, or a build
// with the gate set (chatgptGate.ts). Decided when it's built: ?everything and stored defaults don't
// reach it. An account already signed in this way isn't asked: it keeps its own way back.
const wayIn = import.meta.env.DEV || !!REAL_CHATGPT_SIGN_IN_REPORT;
// Add account's row for a tool, with the logins found on this Mac
export const addsWithChatGpt = (agent: string, found: unknown[]) => wayIn && agent === 'codex' && !found.length;

// The button shows only with no Codex login on this Mac: one that's there is adopted with no click
// (or, removed here, offered by name), and one signed out is the tool's own to sign in.
export function offersChatGpt(status: Pick<Status, 'accounts' | 'availableLogins'> | null | undefined) {
  if (!wayIn) return false;
  if (!status) return true;
  if (status.availableLogins?.some((l) => l.agent === 'codex')) return false;
  return !status.accounts.some((a) => a.agent === 'codex' && (a.signedIn || (!a.login && !!a.label)));
}

// A sign-in's end is obid's `signingIn` going, not a `label`: an account can have one already (plan
// use off, too old). What a sign-in that's waiting takes as over, on an account as obid says it.
export const waitingOver = (account: { signedIn?: boolean; signingIn?: boolean }, signing: { chatgpt?: boolean }) =>
  !!account.signedIn || (!!signing.chatgpt && !account.signingIn);
// The account as obid has it right now: an `accounts` event sent before the sign-in began can still arrive.
export const accountNow = async (id: string) => (await api.status()).accounts.find((x) => x.id === id);

// The first sign-in's welcome, once: obid keeps that it was shown, so emptied browser storage doesn't
// bring it back.
export const welcomeDue = async (status: Pick<Status, 'accounts'> | null | undefined) =>
  !!status?.accounts.some((a) => a.login === 'chatgpt' && a.signedIn) && !(await api.chatgptSettings()).welcomed;
export const welcomed = () => api.setChatgptWelcomed();

// Work nobody's watching uses a ChatGPT login only with a yes (docs/contracts/chatgpt-sign-in.md,
// CHATGPT-20): obid keeps and enforces it, and this asks once per account, a new one or one from
// before there was a question. Oby's own words, Sangam's to change.
// What a task waiting says: a wait for the owner's yes says it whole, its step, with what to do.
export const waitSays = (j: { step?: string; waiting?: { text?: string; reserved?: string[] | null } | null }) =>
  (j.waiting?.reserved?.length === 0 ? j.step : j.waiting?.text) || j.step;
export const BACKGROUND_TITLE = 'Can work I run by myself use this plan?';
export const backgroundWords = (name: string) => `Routines, my own reviews and anything else nobody’s watching would use the ChatGPT plan on ${name}. You can turn this off in Settings › Agents.`;
export const BACKGROUND_LABEL = 'Work I run by myself can use this plan';
export const backgroundDue = (status: Pick<Status, 'accounts'> | null | undefined) =>
  status?.accounts.find((a) => a.login === 'chatgpt' && a.signedIn && a.backgroundConsent === undefined);
