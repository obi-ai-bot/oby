// The window's keyboard shortcuts (docs/contracts/shortcuts.md, SHORTCUTS-1 to 6), as T3 Code keeps
// its own (packages/shared/src/keybindings.ts, apps/web/src/shortcuts/dispatcher.ts): one table of
// keys to commands (shared/shortcuts.ts), one keydown listener on the window that runs the command of the newest screen
// that handles it, and every label drawn from the same table, so a menu never says a key the window
// doesn't answer. The Mac's menu bar (shared/menu.ts) shows them too: a key the page leaves alone
// reaches its menu item, whose click comes here as the same command (window.app.onShortcut).

import { KEYBINDINGS, type Command } from '../../shared/shortcuts';

export { KEYBINDINGS, type Command };
export const COMMANDS = new Set<Command>(KEYBINDINGS.map((b) => b.command));

const mac = () => typeof navigator === 'undefined' || /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

// Whether a keydown is this binding: its one key with mod and no other modifier
export function matches(e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>, key: string, onMac = mac()) {
  const want = key.split('+').pop()!;
  const mod = onMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  return mod && !e.altKey && !e.shiftKey && e.key.toLowerCase() === want;
}
export const commandOf = (e: Parameters<typeof matches>[0], onMac = mac()) =>
  KEYBINDINGS.find((b) => matches(e, b.key, onMac))?.command ?? null;

// A command's key as a menu or a tooltip shows it: ⌘N on a Mac (T3's ⌃⌥⇧⌘ order), Ctrl+N elsewhere
export function shortcutLabel(command: Command, onMac = mac()) {
  const key = KEYBINDINGS.find((b) => b.command === command)?.key;
  if (!key) return '';
  const name = key.split('+').pop()!.toUpperCase();
  return onMac ? `⌘${name}` : `Ctrl+${name}`;
}
// aria-keyshortcuts' words for it ("Meta+N")
export const ariaKeys = (command: Command, onMac = mac()) =>
  `${onMac ? 'Meta' : 'Control'}+${KEYBINDINGS.find((b) => b.command === command)!.key.split('+').pop()!.toUpperCase()}`;
// "Settings ⌘,": a tooltip's words with the key after them
export const withShortcut = (words: string, command: Command) => `${words} ${shortcutLabel(command)}`;

// The screens that answer commands, newest last: the newest that has a command runs it (T3's scopes)
type Handlers = Partial<Record<Command, () => void>>;
const scopes: Handlers[] = [];
export function run(command: Command) {
  const scope = [...scopes].reverse().find((s) => s[command]);
  if (!scope) return false;
  scope[command]!();
  return true;
}
function onKey(e: KeyboardEvent) {
  // a word being composed (an IME) is the typist's, and an alert dialog keeps its keys
  if (e.isComposing || e.keyCode === 229 || (e.target as Element | null)?.closest?.('[role="alertdialog"]')) return;
  const command = commandOf(e);
  // the page took it: the Mac's menu item for the same key doesn't run it again
  if (command && run(command)) e.preventDefault();
}
let listening = false;
// A screen's handlers, while it's shown: gives back what takes them away
export function registerCommands(handlers: Handlers) {
  if (!listening && typeof addEventListener === 'function') {
    listening = true;
    addEventListener('keydown', onKey);
    // the Mac's menu bar, clicked or by its key when the page left it alone
    window.app?.onShortcut?.((command) => { if (COMMANDS.has(command as Command)) run(command as Command); });
  }
  scopes.push(handlers);
  return () => { const i = scopes.lastIndexOf(handlers); if (i >= 0) scopes.splice(i, 1); };
}
