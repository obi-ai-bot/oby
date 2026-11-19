// How the side panel fits beside the chat (docs/contracts/chat-first.md, CHATFIRST-35), as T3 Code
// fits its own (apps/web/src/components/threadSidebarWidth.ts): the chat keeps at least CHAT_MIN, the
// docked panel gives way down to its narrowest, and where even that leaves the chat too narrow the
// panel floats over the chat instead of pushing it past the window's edge (T3 Code's sheet under 768px).

// the narrowest chat beside the docked panel: T3 Code's THREAD_MAIN_CONTENT_MIN_WIDTH (40rem)
export const CHAT_MIN = 640;
// floating, the strip of chat left showing at its right, to see what's under it and click back to it
const PEEK = 56;

// window: the window's width; chosen: the panel's width as the owner set it; min: its narrowest.
// docked: beside the chat, at `width`; else floating over it, at `width`.
export function panelFit(window: number, chosen: number, min: number): { docked: boolean; width: number } {
  const room = window - CHAT_MIN;
  if (room >= min) return { docked: true, width: Math.min(chosen, room) };
  return { docked: false, width: Math.max(0, Math.min(chosen, window - PEEK)) };
}
