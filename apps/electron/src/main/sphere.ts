import { app, BrowserWindow, dialog, ipcMain } from 'electron';
import { generateKeyPairSync, type KeyObject } from 'node:crypto';
import { keptKey } from './device';
import { obid, type Trusted } from './obid';
import { NotThisKey, join, joinWarning, keepMembership, leave, parseInvite } from '../shared/sphere';

// Joining an Obysphere by a private link (docs/contracts/obysphere.md). The owner key, Ed25519, is
// made here in main and kept as the device key is (device.ts): it never leaves main, and the page
// can't ask it to sign anything. A link joins only on the owner's click in this app's own dialog;
// the Obysphere's join page's Open in Oby (oby://join/<host>#…) is how one arrives.
// ponytail: one owner key per Mac, as the Swift app's; the same person on a second Mac is
// another key, which a Sphere refuses for the same owner (SPHERE-2's words say so).

let key: KeyObject | null = null;
const ownerKey = () => (key ??= keptKey('owner-key', () => generateKeyPairSync('ed25519').privateKey));

// oby:// opens this app (electron-builder.yml's protocols); the link may come before the window
let pending: string | null = null;
let opened: ((url: string) => void) | null = null;
app.on('open-url', (e, url) => {
  e.preventDefault();
  if (opened) opened(url); else pending = url;
});

export function sphering(trusted: Trusted, show: () => BrowserWindow) {
  if (app.isPackaged) app.setAsDefaultProtocolClient('oby');
  const said = (w: BrowserWindow, message: string, detail?: string) => dialog.showMessageBox(w, { type: 'warning', message, detail, buttons: ['OK'] });

  // the card obid asks for (its seal key) and the delegation's renewal (SPHERE-6): at launch, after
  // a join, and every 5 minutes, as the Swift app did; obid has no membership event to wait on
  // A membership another key signed is told to the owner, once a launch (SPHERE-6)
  let told = false;
  const keep = () => keepMembership(obid, ownerKey).catch((e) => {
    if (e instanceof NotThisKey && !told) { told = true; void said(show(), 'Oby can’t keep your Obysphere working.', e.message); }
    else if (!(e instanceof NotThisKey)) console.warn('Obysphere membership:', e instanceof Error ? e.message : e);
  });
  void keep();
  setInterval(() => void keep(), 5 * 60_000);

  opened = async (url) => {
    const w = show();
    const invite = parseInvite(url);
    if (!invite) return void said(w, 'Oby can’t open this link.', 'It isn’t an Obysphere invite link.');
    const { response } = await dialog.showMessageBox(w, {
      type: 'question', message: `Join ${invite.name}?`,
      detail: `${invite.by ? `${invite.by} invited you. ` : ''}${joinWarning(invite)}`,
      buttons: ['Join', 'Cancel'], defaultId: 1, cancelId: 1,
    });
    if (response !== 0) return;
    try {
      await join(obid, ownerKey, invite);
      void keep();
      if (!w.isDestroyed()) w.webContents.send('sphere', { sphere: invite.sphere, name: invite.name });
    } catch (e) {
      void said(w, `Oby couldn’t join ${invite.name}.`, e instanceof Error ? e.message : String(e));
    }
  };
  if (pending) { const url = pending; pending = null; opened(url); }

  // Leave, from the Obysphere's settings (SPHERE-4): when the Obysphere couldn't be told, or obid
  // couldn't leave, the owner is told why; answers that, or null
  ipcMain.handle('sphere-leave', async (e) => {
    if (!trusted(e)) return 'Not this app’s page.';
    const w = BrowserWindow.fromWebContents(e.sender) ?? show();
    try {
      const { membership, untold, sphere } = await leave(obid, ownerKey);
      if (untold) void said(w, `You left ${sphere}, but it couldn’t be told.`, `${untold}. It still lists your Oby until its admin removes it.`);
      return untold ?? (membership.state === 'none' ? null : 'obid is still in it.');
    } catch (err) {
      const why = err instanceof Error ? err.message : String(err);
      void said(w, 'Oby couldn’t leave the Obysphere.', why);
      return why;
    }
  });
}
