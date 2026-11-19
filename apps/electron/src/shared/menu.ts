import type { BrowserWindow, MenuItemConstructorOptions } from 'electron';
import { accelerator, KEYBINDINGS, type Command } from './shortcuts';

// Electron's default Mac menu with Oby's name spelled out: its roles would say app.name, which stays
// `oby-app` (package.json's `name`) because the app's folder and its Keychain item are named after it.
const NAME = 'Oby';

// A shortcut's item (shared/shortcuts.ts): its key, and a click that sends its command to the window
// (renderer shortcuts.ts runs it). The page answers the key itself and takes it, so the item runs
// only when it's clicked, or when the page left its key alone.
const shortcut = (command: Command): MenuItemConstructorOptions => ({
  label: KEYBINDINGS.find((b) => b.command === command)!.label,
  accelerator: accelerator(command),
  click: (_item, win) => (win as BrowserWindow | undefined)?.webContents?.send('shortcut', command),
});

export const menu: MenuItemConstructorOptions[] = [
  {
    label: NAME,
    submenu: [
      { role: 'about', label: `About ${NAME}` },
      { type: 'separator' },
      shortcut('settings.open'),
      { type: 'separator' },
      { role: 'services' },
      { type: 'separator' },
      { role: 'hide', label: `Hide ${NAME}` },
      { role: 'hideOthers' },
      { role: 'unhide' },
      { type: 'separator' },
      { role: 'quit', label: `Quit ${NAME}` },
    ],
  },
  // Electron's File menu (Close Window), the window's own items first
  { label: 'File', submenu: [shortcut('chat.new'), { type: 'separator' }, { role: 'close' }] },
  // Electron's Edit menu, with Find as macOS apps have it: Search over every conversation, and Find
  // in the one shown
  {
    label: 'Edit',
    submenu: [
      { role: 'undo' }, { role: 'redo' }, { type: 'separator' },
      { role: 'cut' }, { role: 'copy' }, { role: 'paste' }, { role: 'pasteAndMatchStyle' }, { role: 'delete' }, { role: 'selectAll' },
      { type: 'separator' },
      { label: 'Find', submenu: [shortcut('search.open'), shortcut('chat.find')] },
      { type: 'separator' },
      { label: 'Speech', submenu: [{ role: 'startSpeaking' }, { role: 'stopSpeaking' }] },
    ],
  },
  { role: 'viewMenu' },
  { role: 'windowMenu' },
];
