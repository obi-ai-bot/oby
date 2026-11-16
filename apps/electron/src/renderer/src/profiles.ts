import { useSyncExternalStore } from 'react';
import { api, ApiError, type Profile } from './obid';
import { MOCK_PROFILES } from './mock';
import { onObid } from './lean';
import type { Chat } from './chats';

// The profiles on this Mac (docs/contracts/profiles.md, PROFILES-19 to 25): the side panel's title is
// the open one, and its menu switches or makes one. obid's list (GET /v1/profiles), or the mock's.
// list null: none yet, or an obid without profiles (404) or not answering: the title stays plain.
// kept: the open one's id, the panel's own choice (oby.profile), kept even while obid doesn't list
// it, until the owner picks another
const KEPT = 'oby.profile';
const keptId = () => { try { return localStorage.getItem(KEPT); } catch { return null; } };
let state: { list: Profile[] | null; kept: string | null } = { list: onObid ? null : MOCK_PROFILES, kept: keptId() };
const listeners = new Set<() => void>();
const set = (next: Partial<typeof state>) => { state = { ...state, ...next }; listeners.forEach((l) => l()); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export const useProfiles = () => useSyncExternalStore(subscribe, () => state);

// Which one is open: the kept one if it's listed, else the default, else the first
export const openProfile = (list: Profile[], kept: string | null) =>
  list.find((p) => p.id === kept) ?? list.find((p) => p.default) ?? list[0];

// obid's list again: when the app starts, and each time the menu opens. An obid without profiles
// (404) or not answering (0) has none; any other failure leaves the list as it was
export async function loadProfiles() {
  if (!onObid) return;
  try {
    set({ list: await api.profiles() });
  } catch (e) {
    if (e instanceof ApiError && (e.status === 404 || e.status === 0)) set({ list: null });
  }
}
if (onObid) void loadProfiles();

export function chooseProfile(id: string) {
  set({ kept: id });
  try { localStorage.setItem(KEPT, id); } catch { /* not remembered, that's all */ }
}

// A new profile: obid makes it (its refusal, an ApiError, goes to the form as it said it), or on the
// mock, the mock; then it's listed and open
export async function createProfile(name: string, color: string) {
  const made = onObid
    ? await api.createProfile(name, color)
    : { id: `p${Date.now().toString(36)}`, name: name.trim(), color, default: false, waiting: false };
  set({ list: [...(state.list ?? []), made] });
  chooseProfile(made.id);
  return made;
}

// The panel's conversations under the open profile: on the mock only, each its own. On obid a
// profile holds nothing of its own yet, so all of them, as they are
export function profileChats(chats: Chat[], list: Profile[] | null, kept: string | null) {
  if (onObid || !list?.length) return chats;
  const open = openProfile(list, kept).id, first = list.find((p) => p.default)?.id;
  return chats.filter((c) => (c.profile ?? first) === open);
}
