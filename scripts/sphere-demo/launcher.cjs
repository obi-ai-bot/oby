// The demo Oby's app (scripts/sphere-demo.sh): this checkout's Electron app, built, run beside the
// owner's own Oby with an app folder of its own, on the demo obid (OBID_PORT, OBID_HOME, which only
// a development build reads). Nothing here is the product: it marks the window as the demo one and
// takes the script's few requests through files in the demo's folder, never an argument, so an
// invite link's secret is in no process list or log.
//
//   OBY_DEMO_DIR   the demo's folder (0700): app/ is its app folder, control and join-link its inbox
//   OBY_DEMO_TITLE what the window and its badge say, "Oby · Ben (demo)"
//   OBY_DEMO_MAIN  the built app's main (apps/electron/out/main/index.js)
//   OBY_DEMO_YES   1: the one "Join …?" the script's own link opens is answered Join (the owner said
//                  so by running the script with --yes); every other dialog is the owner's to answer
const { app, dialog, BrowserWindow } = require('electron');
const fs = require('node:fs');
const { join } = require('node:path');
const { pathToFileURL } = require('node:url');

const dir = process.env.OBY_DEMO_DIR;
const title = process.env.OBY_DEMO_TITLE || 'Oby (demo)';
const main = process.env.OBY_DEMO_MAIN;
if (!dir || !main) throw new Error('launcher.cjs is started by scripts/sphere-demo.sh');

app.setName(title);
app.setPath('userData', join(dir, 'app'));

// What the owner sees, to tell the two Obys apart: the window's title, a badge on its page and the
// Dock's. Dialogs are noted (their words, never a link) so the script can say why a join failed.
const note = (o) => fs.appendFileSync(join(dir, 'dialogs.log'), `${JSON.stringify({ at: new Date().toISOString(), ...o })}\n`, { mode: 0o600 });
const BADGE = `body::after { content: ${JSON.stringify(title)}; position: fixed; top: 8px; left: 50%; transform: translateX(-50%);
  z-index: 2147483647; pointer-events: none; padding: 3px 10px; border-radius: 999px; font: 600 11px/16px -apple-system, system-ui, sans-serif;
  color: #fff; background: #d9480f; box-shadow: 0 1px 4px rgba(0,0,0,.25); }`;
app.on('browser-window-created', (_e, win) => {
  // a rehearsal's (OBY_DEMO_HIDDEN=1): the window off the owner's screen, driven over DevTools
  if (process.env.OBY_DEMO_HIDDEN === '1') { win.setOpacity(0); win.setFocusable(false); win.setIgnoreMouseEvents(true); }
  win.setTitle(title);
  win.on('page-title-updated', (e) => { e.preventDefault(); win.setTitle(title); });
  win.webContents.on('did-finish-load', () => {
    void win.webContents.insertCSS(BADGE);
    // the demo Oby opens on Home, its first run marked done as the e2e scripts mark it
    // (apps/electron/scripts/drive-lib.mjs, skipFirstRun), then the page loaded again. It opens on
    // Sangam's Home (`oby.home`, docs/contracts/chat-first.md), whose list shows the task another
    // Oby handed it as it runs: chat first shows a task with no conversation nowhere until it ends
    // (that contract's known ceiling). OBY_DEMO_CHAT=1 keeps chat first.
    const home = process.env.OBY_DEMO_CHAT === '1' ? '' : "localStorage.setItem('oby.home', '1');";
    void win.webContents.executeJavaScript(`(() => {
      if (localStorage.getItem('oby.onboarded') === '1') return false;
      for (const k of ['oby.onboarded', 'oby.setUp', 'oby.welcomed']) localStorage.setItem(k, '1');
      ${home}
      return true;
    })()`).then((marked) => {
      if (!marked) return;
      // first run held the window narrow (main's setup-window); a page opened on Home keeps the
      // size it finds, so the demo window gets a working one back
      win.setResizable(true); win.setMaximizable(true); win.setFullScreenable(true);
      win.setSize(1280, 820);
      win.center();
      win.webContents.reload();
    }, () => {});
    // the page is up, so main has run sphering() and takes a link: the script may send one
    fs.writeFileSync(join(dir, 'ready'), `${process.pid}\n`);
  });
});
app.whenReady().then(() => (process.env.OBY_DEMO_HIDDEN === '1' ? app.dock?.hide() : app.dock?.setBadge('demo')));

let joinArmed = false;
const show = dialog.showMessageBox.bind(dialog);
dialog.showMessageBox = async (a, b) => {
  const opts = b ?? a;
  note({ message: opts.message, detail: opts.detail, buttons: opts.buttons });
  if (joinArmed && process.env.OBY_DEMO_YES === '1' && /^Join .*\?$/.test(opts.message ?? '') && opts.buttons?.[0] === 'Join') {
    joinArmed = false;
    note({ answered: 'Join' });
    return { response: 0, checkboxChecked: false };
  }
  const answer = await show(a, b);
  note({ message: opts.message, answered: opts.buttons?.[answer.response] });
  return answer;
};

// The script's requests: a word in the file `control`, read and removed. `join` takes the link from
// `join-link` (0600, removed once read) as macOS hands Oby an oby:// link; `leave` is Settings'
// Leave (window.app.sphereLeave, which signs with the owner key this app keeps), answered in
// `control.out`; `quit` quits.
const control = join(dir, 'control');
const take = (file) => {
  try { const text = fs.readFileSync(file, 'utf8').trim(); fs.rmSync(file, { force: true }); return text; } catch { return ''; }
};
const answer = (o) => fs.writeFileSync(join(dir, 'control.out'), `${JSON.stringify(o)}\n`, { mode: 0o600 });
fs.watchFile(control, { interval: 300 }, () => {
  const word = take(control);
  if (word === 'join') {
    const link = take(join(dir, 'join-link'));
    if (!link) return answer({ join: 'no link' });
    joinArmed = true;
    note({ openUrl: 'the script’s invite link' });
    app.emit('open-url', { preventDefault() {} }, link);
    answer({ join: 'opened' });
  } else if (word === 'leave') {
    const win = BrowserWindow.getAllWindows()[0];
    if (!win) return answer({ leave: 'no window' });
    win.webContents.executeJavaScript('window.app.sphereLeave ? window.app.sphereLeave() : "this build can’t leave"')
      .then((why) => answer({ leave: why ?? 'left' }), (e) => answer({ leave: String(e?.message ?? e) }));
  } else if (word === 'quit') {
    app.quit();
  }
});

import(pathToFileURL(main).href);
