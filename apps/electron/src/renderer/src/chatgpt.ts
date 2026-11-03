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

// The button shows only with no Codex login on this Mac: one that's there is adopted with no click
// (or, removed here, offered by name), and one signed out is the tool's own to sign in.
export function offersChatGpt(status: Pick<Status, 'accounts' | 'availableLogins'> | null | undefined) {
  if (!status) return true;
  if (status.availableLogins?.some((l) => l.agent === 'codex')) return false;
  return !status.accounts.some((a) => a.agent === 'codex' && (a.signedIn || (!a.login && !!a.label)));
}

// The first sign-in's welcome, once: obid keeps that it was shown, so emptied browser storage doesn't
// bring it back.
export const welcomeDue = async (status: Pick<Status, 'accounts'> | null | undefined) =>
  !!status?.accounts.some((a) => a.login === 'chatgpt' && a.signedIn) && !(await api.chatgptSettings()).welcomed;
export const welcomed = () => api.setChatgptWelcomed();
