import { useSyncExternalStore } from 'react';
import { api, ApiError, type Helper } from './obid';
import { MOCK_HELPERS } from './mock';
import { onObid } from './lean';
import { addChat } from './chats';

// The helpers, shown as Agents beside Projects in the side panel (docs/contracts/helpers.md,
// HELPERS-12 to 20): obid's list (GET /v1/helpers), or the mock's. An obid without them (404) or
// not answering (0) has none, with nothing shown as an error
let list: Helper[] = onObid ? [] : MOCK_HELPERS;
const listeners = new Set<() => void>();
const set = (next: Helper[]) => { list = next; listeners.forEach((l) => l()); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export const useHelpers = () => useSyncExternalStore(subscribe, () => list);

// obid's list again: when the app starts, each time Agents is shown, and after a create; any
// failure but those two leaves the list as it was
export async function loadHelpers() {
  if (!onObid) return;
  try {
    set(await api.helpers());
  } catch (e) {
    if (e instanceof ApiError && (e.status === 404 || e.status === 0)) set([]);
  }
}
if (onObid) void loadHelpers();

// A new helper: obid makes it, with its conversation (its refusal, an ApiError, goes to the sheet as
// it said it), or on the mock, the mock, with a conversation of the app's own
export type NewHelper = { name: string; color: string; instructions: string; tool: string; connectors: string[]; profile?: string };
export async function createHelper(body: NewHelper) {
  if (onObid) {
    const made = await api.createHelper(body);
    set([...list.filter((h) => h.id !== made.id), made]);
    void loadHelpers();
    return made;
  }
  const id = `h${Date.now().toString(36)}`;
  const made: Helper = { ...body, id, profile: body.profile ?? 'default', conversation: `c-${id}`, created: new Date().toISOString() };
  // (moved just now, as obid's is when it's made)
  addChat({ id: made.conversation, title: made.name, said: [], at: 0, local: true, helper: id, profile: made.profile });
  set([...list, made]);
  return made;
}
