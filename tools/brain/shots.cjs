// make brain-shots: a screenshot of every state the Brain lists (.brain/brain.json's states), each
// in a fresh session of its own on the mock app, saved as .brain/shots/<id>.png with index.json.
// Run by the app's own Electron (apps/electron), so the screens are drawn as the app draws them.
const { app, BrowserWindow } = require('electron');
const { mkdirSync, readFileSync, writeFileSync } = require('node:fs');
const { join } = require('node:path');

const dir = join(__dirname, '.brain'), out = join(dir, 'shots');
const { states } = JSON.parse(readFileSync(join(dir, 'brain.json'), 'utf8'));
// a scenario plays at Instant and gets there in about a second; the rest is the app settling
const WAIT_MS = Number(process.env.SHOT_WAIT_MS) || 3500;
const AT_ONCE = Number(process.env.SHOT_AT_ONCE) || 4;
const SIZE = { width: 1280, height: 800 };

async function shoot(s) {
  // (a partition with no persist: prefix lives in memory: nothing kept from another state)
  const win = new BrowserWindow({ show: false, ...SIZE, webPreferences: { partition: `shots-${s.id}`, backgroundThrottling: false } });
  try {
    await win.loadURL(s.url);
    await new Promise((r) => setTimeout(r, WAIT_MS));
    writeFileSync(join(out, `${s.id}.png`), (await win.webContents.capturePage()).toPNG());
    console.log(`shot ${s.id}`);
    return true;
  } catch (e) {
    console.warn(`couldn't shoot ${s.id}: ${e.message}`);
    return false;
  } finally {
    win.destroy();
  }
}

app.dock?.hide();
// (a batch's windows all closed isn't the end: Electron would quit then, by default)
app.on('window-all-closed', () => {});
app.whenReady().then(async () => {
  mkdirSync(out, { recursive: true });
  const shots = {};
  for (let i = 0; i < states.length; i += AT_ONCE) {
    const batch = states.slice(i, i + AT_ONCE);
    const ok = await Promise.all(batch.map(shoot));
    batch.forEach((s, j) => { if (ok[j]) shots[s.id] = { label: s.label, url: s.url }; });
  }
  writeFileSync(join(out, 'index.json'), JSON.stringify({ at: new Date().toISOString(), size: SIZE, shots }, null, 2));
  console.log(`${Object.keys(shots).length} of ${states.length} states shot into ${out}`);
  app.quit();
});
