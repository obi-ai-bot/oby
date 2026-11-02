import type { MenuItemConstructorOptions } from 'electron';

// Electron's default Mac menu with Oby's name spelled out: its roles would say app.name, which stays
// `oby-app` (package.json's `name`) because the app's folder and its Keychain item are named after it.
const NAME = 'Oby';

export const menu: MenuItemConstructorOptions[] = [
  {
    label: NAME,
    submenu: [
      { role: 'about', label: `About ${NAME}` },
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
  { role: 'fileMenu' },
  { role: 'editMenu' },
  { role: 'viewMenu' },
  { role: 'windowMenu' },
];
