import { useSyncExternalStore } from 'react';
import { OBYS } from './mock';
import type { SphereMember } from '../../shared/obid-wire';

// Whom `@` in the message box asks (docs/contracts/ask-an-oby.md, ASK-37 to 39), the "Obys" of
// Sangam's composer: on obid, the Obysphere's live members whose card takes questions, each by exact
// owner id (a card's name proves nothing, ASK-17), set by spheres.ts from GET /v1/obid/sphere/members;
// in the mock, its Obys. name: what a mention says (@dana), their exact owner id on obid; card: their
// card's name as obid gives it (Dana Scully), shown with the owner id beside it when they differ;
// lookalike: the owner ids theirs looks like (ASK-35), shown under their row.
export type Askable = { name: string; card?: string; lookalike?: string[] };

let askable: Askable[] = OBYS.map((o) => ({ name: o.name }));
const listeners = new Set<() => void>();
export const askableNow = () => askable;
// told when whom `@` asks changes (textTokens.ts, which isn't React)
export const onAskable = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export const useAskable = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => askable);
export function setAskable(next: Askable[]) {
  if (JSON.stringify(next) === JSON.stringify(askable)) return;
  askable = next;
  listeners.forEach((l) => l());
}
// the members `@` lists: those whose card takes questions, each owner once, as obid sorts them
export const askableOf = (members: SphereMember[]): Askable[] => {
  const seen = new Set<string>();
  return members.flatMap((m) => {
    if (!m.accepts?.includes('question') || seen.has(m.owner)) return [];
    seen.add(m.owner);
    const card = m.name?.trim();
    return [{ name: m.owner, ...(card && card.toLowerCase() !== m.owner.toLowerCase() ? { card } : {}), ...(m.lookalike?.length ? { lookalike: m.lookalike } : {}) }];
  });
};
// what a look-alike's row says under it, as the ASK-34 Confirm names them ("not dima's"): proposed
// words until Sangam words them
export const notThem = (alike: string[]) =>
  `Not ${alike.length > 1 ? `${alike.slice(0, -1).map((a) => `${a}’s`).join(', ')} or ${alike.at(-1)}’s` : `${alike[0]}’s`} Oby`;
// the Obys a name typed so far could be, by owner id or card name: those either starts, then those
// either holds
export function askableFor(typed: string, all = askable) {
  const q = typed.trim().toLowerCase();
  const names = (o: Askable) => [o.name, ...(o.card ? [o.card] : [])].map((n) => n.toLowerCase());
  const starts = (o: Askable) => names(o).some((n) => n.startsWith(q) || n.split(/\s+/).some((w) => w.startsWith(q)));
  return [...all.filter(starts), ...all.filter((o) => !starts(o) && names(o).some((n) => n.includes(q)))];
}
// what a member's row says under it: their owner id beside a card name that isn't it, as a thread
// names them (peers.ts, whoseOby), and who a look-alike isn't
export const aboutOf = (o: Askable) => [o.card ? o.name : '', o.lookalike?.length ? notThem(o.lookalike) : ''].filter(Boolean).join(' · ');
