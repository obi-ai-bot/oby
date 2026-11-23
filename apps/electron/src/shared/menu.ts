import type { BrowserWindow, MenuItemConstructorOptions } from 'electron';
import { accelerator, bindingOf, JUMPS, type Command } from './shortcuts';

// Electron's default Mac menu with Oby's name spelled out: its roles would say app.name, which stays
// `oby-app` (package.json's `name`) because the app's folder and its Keychain item are named after it.
const NAME = 'Oby';

// A shortcut's item (shared/shortcuts.ts): its key, and a click that sends its command to the window
// (renderer shortcuts.ts runs it). The page answers the key itself and takes it, so the item runs
// only when it's clicked, or when the page left its key alone.
const shortcut = (command: Command): MenuItemConstructorOptions => ({
  label: bindingOf(command).label,
  accelerator: accelerator(command),
  click: (_item, win) => (win as BrowserWindow | undefined)?.webContents?.send('shortcut', command),
});

// The Help menu's items, run in main (main/help.ts, docs/contracts/help.md)
export type HelpActions = { revealLog: () => void; copyDiagnostics: () => void };
export const HELP_LABELS = { revealLog: 'Reveal obid’s Log', copyDiagnostics: 'Copy Diagnostics' } as const;

// The menu bar (SHORTCUTS-10): dev is a run from source (npm run dev), which keeps Electron's Reload,
// Force Reload and Toggle Developer Tools; a packaged build has none of them
export function appMenu({ dev, help }: { dev: boolean; help: HelpActions }): MenuItemConstructorOptions[] {
  return [
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
    // Electron's View menu (its viewMenu role), the side panel's toggle and the browser first and its developer
    // items only in a run from source
    {
      label: 'View',
      submenu: [
        shortcut('sidebar.toggle'),
        shortcut('browser.show'),
        { type: 'separator' },
        ...(dev ? [{ role: 'reload' }, { role: 'forceReload' }, { role: 'toggleDevTools' }, { type: 'separator' }] as MenuItemConstructorOptions[] : []),
        { role: 'resetZoom' }, { role: 'zoomIn' }, { role: 'zoomOut' },
        { type: 'separator' },
        { role: 'togglefullscreen' },
      ],
    },
    // the conversations in the side panel's order, as Finder's and Safari's Go menus go through places
    { label: 'Go', submenu: [shortcut('chat.previous'), shortcut('chat.next'), { type: 'separator' }, ...JUMPS.map(shortcut)] },
    { role: 'windowMenu' },
    { role: 'help', submenu: [{ label: HELP_LABELS.revealLog, click: help.revealLog }, { label: HELP_LABELS.copyDiagnostics, click: help.copyDiagnostics }] },
  ];
}
