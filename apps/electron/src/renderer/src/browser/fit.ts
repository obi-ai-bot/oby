// How the browser panel fits beside the conversation (docs/contracts/browser-app.md, BROWSERAPP-3):
// the conversation keeps 420px (the design's board 4, the window's --view-min); the panel gives way
// down to its narrowest; where the docked side panel would leave it narrower than that, the side
// panel floats (panelFit.ts) and the browser keeps its room.
import { PANEL_MIN } from './store';

/** the conversation's narrowest beside the browser: tokens.css --view-min */
export const CONVERSATION_MIN = 420;

// window: the window's width; sidebar: the docked side panel's width (0: none docked); chosen: the
// panel's width as the owner left it. sidebarDocks: whether the side panel may stay docked.
export function browserFit(window: number, sidebar: number, chosen: number): { width: number; sidebarDocks: boolean } {
  const room = window - sidebar - CONVERSATION_MIN;
  if (sidebar > 0 && room < PANEL_MIN) return { width: Math.max(PANEL_MIN, Math.min(chosen, window - CONVERSATION_MIN)), sidebarDocks: false };
  return { width: Math.max(PANEL_MIN, Math.min(chosen, room)), sidebarDocks: true };
}
