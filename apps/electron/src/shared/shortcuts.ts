// The window's keyboard shortcuts, one table for the page (renderer shortcuts.ts) and the Mac's menu
// bar (shared/menu.ts), as T3 Code keeps its own (packages/shared/src/keybindings.ts): a key in a
// menu is always one the window answers (docs/contracts/shortcuts.md, SHORTCUTS-1 to 6).

export type Command = 'chat.new' | 'search.open' | 'chat.find' | 'settings.open';

// mod: ⌘ on a Mac, Ctrl elsewhere (T3's `mod`); label: the menu bar's item
export const KEYBINDINGS: { key: string; command: Command; label: string }[] = [
  { key: 'mod+n', command: 'chat.new', label: 'New Conversation' },
  { key: 'mod+k', command: 'search.open', label: 'Search' },
  { key: 'mod+f', command: 'chat.find', label: 'Find in Conversation' },
  { key: 'mod+,', command: 'settings.open', label: 'Settings…' },
];

// Electron's accelerator for a command's key ("CmdOrCtrl+N")
export const accelerator = (command: Command) => {
  const key = KEYBINDINGS.find((b) => b.command === command)!.key;
  return `CmdOrCtrl+${key.split('+').pop()!.toUpperCase()}`;
};
