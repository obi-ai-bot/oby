// The window's keyboard shortcuts (docs/contracts/shortcuts.md, SHORTCUTS-1 to 11), as T3 Code keeps
// its own (packages/shared/src/keybindings.ts, apps/web/src/shortcuts/dispatcher.ts): one table of
// keys to commands (shared/shortcuts.ts), one keydown listener on the window that runs the command of the newest screen
// that handles it, and every label drawn from the same table, so a menu never says a key the window
// doesn't answer. The Mac's menu bar (shared/menu.ts) shows them too: a key the page leaves alone
// reaches its menu item, whose click comes here as the same command (window.app.onShortcut).

import { bindingOf, JUMPS, keyParts, KEYBINDINGS, type Command, type Jump } from '../../shared/shortcuts';

export { JUMPS, KEYBINDINGS, type Command, type Jump };
export const COMMANDS = new Set<Command>(KEYBINDINGS.map((b) => b.command));

const mac = () => typeof navigator === 'undefined' || /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

// What a key reads as without ⇧ on a US keyboard (T3's US_SHIFTED_TO_UNSHIFTED_KEYS): ⇧[ is "{"
const UNSHIFTED: Record<string, string> = { '{': '[', '}': ']' };

// Whether a keydown is this binding: its one key with mod, ⇧ only when the binding has it, and no other modifier
export function matches(e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>, key: string, onMac = mac()) {
  const want = keyParts(key);
  const mod = onMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  const pressed = e.key.toLowerCase();
  return mod && !e.altKey && e.shiftKey === want.shift && (UNSHIFTED[pressed] ?? pressed) === want.key;
}
export const commandOf = (e: Parameters<typeof matches>[0], onMac = mac()) =>
  KEYBINDINGS.find((b) => matches(e, b.key, onMac))?.command ?? null;

// A command's key as a menu or a tooltip shows it: ⌘N or ⇧⌘[ on a Mac (T3's ⌃⌥⇧⌘ order), Ctrl+N elsewhere
export function shortcutLabel(command: Command, onMac = mac()) {
  const binding = KEYBINDINGS.find((b) => b.command === command);
  if (!binding) return '';
  const { shift, key } = keyParts(binding.key);
  return onMac ? `${shift ? '⇧' : ''}⌘${key.toUpperCase()}` : `Ctrl+${shift ? 'Shift+' : ''}${key.toUpperCase()}`;
}
// aria-keyshortcuts' words for it ("Meta+N", "Meta+Shift+[")
export const ariaKeys = (command: Command, onMac = mac()) => {
  const { shift, key } = keyParts(bindingOf(command).key);
  return `${onMac ? 'Meta' : 'Control'}+${shift ? 'Shift+' : ''}${key.toUpperCase()}`;
};
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
// A word being composed (an input method) is the typist's: its Enter confirms the word and its Esc
// drops it, in any field of the window, never the field's own Enter or Esc (T3 Code 4a2f8b04bb, 54c1f1007c)
export const composing = (e: { keyCode: number; isComposing?: boolean; nativeEvent?: { isComposing?: boolean } }) =>
  !!(e.nativeEvent ?? e).isComposing || e.keyCode === 229;
function onKey(e: KeyboardEvent) {
  // a word being composed is the typist's, and an alert dialog keeps its keys
  if (composing(e) || (e.target as Element | null)?.closest?.('[role="alertdialog"]')) return;
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
