import type { Agent } from './mock';
import { ApiError, api, type SignIn, type Status } from './obid';

// Settings › Agents' Add account, what isn't the sheet's (docs/contracts/accounts.md): a row per
// tool with how many it has and the logins found on this Mac; picking a tool makes its account,
// with no name (obid's first sign-in names it), and starts its own sign-in at once.
const TOOLS: Agent[] = ['claude', 'codex', 'cursor'];
export type Found = NonNullable<Status['availableLogins']>[number];

export const toolRows = (accounts: { agent: Agent }[], found: Found[]) => TOOLS.map((agent) => ({
  agent,
  added: accounts.filter((a) => a.agent === agent).length,
  found: found.filter((l) => l.agent === agent),
}));

// a try ended: its sign-in ended at obid and the account it made taken off, so only what was
// connected shows in Settings › Agents
export async function stopAdding(account: string) {
  await api.endSignIn(account).catch(() => { /* ended already */ });
  await api.removeAccount(account).catch(() => { /* gone already, or Settings shows it signed out */ });
}

export async function startAdding(agent: Agent): Promise<{ account: string; page: SignIn }> {
  const { id: account } = await api.addAccount({ agent });
  try {
    const page = await api.signIn(account);
    window.app?.openExternal(page.url);
    return { account, page };
  } catch (e) {
    await stopAdding(account);
    throw e;
  }
}

// obid took the account off while it signed in: the account it turned out to be, if that's why
export const sameAs = (e: unknown) => (e instanceof ApiError && e.reason === 'sameAccount' ? e.sameAs ?? null : null);
export const whoseAgain = (account: string) => api.signIn(account).then(() => null, sameAs);
export const again = (accounts: { id: string; name: string }[], id: string) => `That’s ${accounts.find((a) => a.id === id)?.name ?? 'that account'} again`;
