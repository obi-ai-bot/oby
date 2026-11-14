import { app } from 'electron';
import { constants, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { writeOwn } from './device';
import { ipc, type Trusted } from './ipc';
import type { Decoder } from '../shared/decode';

// Oby's own settings in the app's own folder, one file main owns: Settings › Notifications' alerts
// (notify.ts), About's Update automatically (updates.ts), and under `local` the page's Settings
// rows obid has no setting for yet (docs/contracts/live-state.md, LIVE-25 and 26)
export const SETTINGS = () => join(app.getPath('userData'), 'settings.json');
export const settings = (): Record<string, unknown> => { try { return JSON.parse(readFileSync(SETTINGS(), 'utf8')); } catch { return {}; } };
// settings.json with patch's top-level keys set, every other key kept
export function writeSettings(patch: Record<string, unknown>) {
  writeOwn(SETTINGS(), JSON.stringify({ ...settings(), ...patch }), constants.O_TRUNC);
}

// A row's key: never one of the object's own (__proto__), never empty, at most 64 characters
const rowKey: Decoder<string> = (k) => (typeof k === 'string' && /^[A-Za-z][A-Za-z0-9._-]{0,63}$/.test(k) ? { value: k } : undefined);
// A row's value: plain JSON, at most 64 KiB of it, kept exactly as it reads back
const rowValue: Decoder<unknown> = (v) => {
  try {
    const json = JSON.stringify(v);
    return typeof json === 'string' && json.length <= 65_536 ? { value: JSON.parse(json) } : undefined;
  } catch { return undefined; }
};

// the local rows as kept: {} unless `local` is a plain object, and only the keys a row may have
function local(): Record<string, unknown> {
  const kept = (settings() as Record<string, unknown> | null)?.local;
  if (!kept || typeof kept !== 'object' || Array.isArray(kept)) return {};
  return Object.fromEntries(Object.entries(kept).filter(([k]) => rowKey(k)));
}

// The page's rows (SettingsView.tsx, useSetting): all of them, and one of them kept. They reach
// only `local`, never a key of main's own.
export function localSettings(trusted: Trusted) {
  const channels = ipc(trusted);
  channels.handle('local-settings', [], () => local(), null);
  channels.handle('local-setting', [rowKey, rowValue], (_e, key, value) => {
    try {
      writeSettings({ local: { ...local(), [key]: value } });
      return true;
    } catch (e) {
      console.error(`Oby: couldn’t keep a setting: ${e}`);
      return false;
    }
  }, null);
}
