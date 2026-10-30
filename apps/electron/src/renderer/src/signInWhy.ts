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

export const UNFINISHED = 'Didn’t finish signing in. Try again.';

export function signInWhy(name: string, e: unknown): string {
  const words = e instanceof ApiError && e.reason ? WORDS[e.reason] : undefined;
  return words ? words(name) : `Couldn’t connect ${name}. Try again.`;
}
