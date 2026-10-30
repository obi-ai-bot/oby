import { ApiError } from './obid';

// Why a sign-in didn't take, in a sentence under its row (docs/contracts/sign-in.md): obid's
// stable `reason` picks the words; anything else (no reason, one this app doesn't know yet, obid
// not answering) gets the plain one. obid's own `error` is never shown here: it can name a folder.
// Sangam's to rewrite.
const WORDS: Record<string, (name: string) => string> = {
  notInstalled: (name) => `${name} isn’t on this Mac.`,
  ownLogin: (name) => `Sign in with ${name} itself on this Mac, then Connect again.`,
  folderUnsafe: (name) => `${name}’s login folder isn’t one Oby can sign in through.`,
  keychainOff: () => 'Oby can’t use Claude Code’s login in your Keychain yet.',
  busy: (name) => `${name} is busy with a task. Try again when it’s done.`,
  didNotStart: (name) => `${name}’s sign-in didn’t start. Try again.`,
  codeRefused: () => 'That code didn’t work. Connect again for a new one.',
  badAddress: () => 'That isn’t a Tetrix address Oby can use.',
  noSignIn: () => 'That Tetrix doesn’t say where to sign in.',
};

// GitHub's own, where what's missing or busy is gh, not an agent
const GITHUB: Record<string, string> = {
  notInstalled: 'GitHub’s own tool (gh) isn’t on this Mac.',
  busy: 'GitHub’s sign-in is already starting. Try again in a moment.',
};

export const UNFINISHED = 'Didn’t finish signing in. Try again.';

// the sentence for a reason, or the plain one
export function signInSaid(name: string, reason?: string): string {
  const own = name === 'GitHub' && reason ? GITHUB[reason] : undefined;
  const words = reason ? WORDS[reason] : undefined;
  return own ?? (words ? words(name) : `Couldn’t connect ${name}. Try again.`);
}

export const signInWhy = (name: string, e: unknown) => signInSaid(name, e instanceof ApiError ? e.reason : undefined);
