// The window's keyboard shortcuts, one table for the page (renderer shortcuts.ts) and the Mac's menu
// bar (shared/menu.ts), as T3 Code keeps its own (packages/shared/src/keybindings.ts): a key in a
// menu is always one the window answers (docs/contracts/shortcuts.md, SHORTCUTS-1 to 11).

// ⌘1 to ⌘9: the 1st to 9th conversation in the side panel's order (T3's thread.jump.1 to 9)
export const JUMPS = ['chat.go.1', 'chat.go.2', 'chat.go.3', 'chat.go.4', 'chat.go.5', 'chat.go.6', 'chat.go.7', 'chat.go.8', 'chat.go.9'] as const;
export type Jump = (typeof JUMPS)[number];

export type Command = 'chat.new' | 'search.open' | 'chat.find' | 'settings.open' | 'sidebar.toggle' | 'chat.previous' | 'chat.next' | Jump;

// mod: ⌘ on a Mac, Ctrl elsewhere (T3's `mod`), shift: ⇧, then the one key; label: the menu bar's item
export const KEYBINDINGS: { key: string; command: Command; label: string }[] = [
  { key: 'mod+n', command: 'chat.new', label: 'New Conversation' },
  { key: 'mod+k', command: 'search.open', label: 'Search' },
  { key: 'mod+f', command: 'chat.find', label: 'Find in Conversation' },
  { key: 'mod+,', command: 'settings.open', label: 'Settings…' },
  { key: 'mod+b', command: 'sidebar.toggle', label: 'Toggle Sidebar' },
  { key: 'mod+shift+[', command: 'chat.previous', label: 'Previous Conversation' },
  { key: 'mod+shift+]', command: 'chat.next', label: 'Next Conversation' },
  ...JUMPS.map((command, i) => ({ key: `mod+${i + 1}`, command, label: `Conversation ${i + 1}` })),
];

export const bindingOf = (command: Command) => KEYBINDINGS.find((b) => b.command === command)!;

// A binding's parts: whether it takes ⇧, and its one key ("[", "n", "1")
export function keyParts(key: string) {
  const parts = key.split('+');
  return { shift: parts.includes('shift'), key: parts.at(-1)! };
}

// Electron's accelerator for a command's key ("CmdOrCtrl+N", "CmdOrCtrl+Shift+[")
export const accelerator = (command: Command) => {
  const { shift, key } = keyParts(bindingOf(command).key);
  return `CmdOrCtrl+${shift ? 'Shift+' : ''}${key.toUpperCase()}`;
};
