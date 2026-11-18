// "You" on the rig, driven through the Electron dev app as a person drives it (RIG-28): the built
// app (apps/electron/out) under Playwright, hidden, with an app folder of its own in the rig's, so
// its device key and its owner key are the app's own, made in its main. Every word goes by a real
// click or keystroke that main sees itself (before-mouse-event, before-input-event), so main signs
// it with the app's key, never the rig's. Nothing here signs or calls obid in the app's place.
import { createWriteStream, existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { createPrivateKey, createPublicKey } from "node:crypto";
import { launch, skipFirstRun, until } from "../../apps/electron/scripts/drive-lib.mjs";

// The rig's own guard holds for the app too: its obid is the rig's "you", never the owner's on 7777.
export async function openApp({ rigDir, port, env }) {
  if (!port || Number(port) === 7777) throw new Error("The app is pointed at port 7777: that's the owner's own Oby.");
  const home = join(rigDir, "you/home");
  const app = await launch({
    dir: join(rigDir, "you"), userData: join(rigDir, "you/electron-data"),
    env: { ...env, OBID_HOME: home, OBID_PORT: String(port) },
  });
  // what main says (a signature it declined, a join it couldn't make), beside the rig's logs
  const log = createWriteStream(join(rigDir, "app.log"), { flags: "a" });
  app.process().stdout?.pipe(log);
  app.process().stderr?.pipe(log);
  const page = await app.firstWindow();
  await skipFirstRun(page);
  // a Mac's window, not the narrow one first run leaves, where the side panel lies over Home's box
  // and takes its clicks
  await app.evaluate(({ BrowserWindow }) => { const w = BrowserWindow.getAllWindows()[0]; w?.setMinimumSize(0, 0); w?.setSize(1400, 900); });
  await until("a window 1400 wide", async () => (await page.evaluate(() => innerWidth)) >= 1300, 10_000, 250);
  return { app, page, pid: app.process().pid, userData: join(rigDir, "you/electron-data") };
}

// The owner key main made for the Obysphere (main/sphere.ts, `owner-key.pem` in the app folder):
// its public half as the Sphere binds it, or null when there's none yet.
export function appOwnerKey(userData) {
  const file = join(userData, "owner-key.pem");
  if (!existsSync(file)) return null;
  return createPublicKey(createPrivateKey(readFileSync(file))).export({ format: "jwk" }).x;
}

// Text on the page, read again until `re` is in it.
export const showing = (page, re, seconds = 30) =>
  until(`the app showing ${re}`, async () => re.test(await page.locator("body").innerText()), seconds * 1000, 250);

// The first of these locators that's on screen within `ms`, or null.
export async function firstVisible(locators, ms = 3000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    for (const l of locators) if (await l.first().isVisible().catch(() => false)) return l.first();
    await new Promise((done) => setTimeout(done, 200));
  }
  return null;
}

// The side panel on the Obysphere: its tile, a button named for it (the rig's is "The rig"), as the
// person clicks it, only when its section (Members) isn't open already: a click on the tile of the
// open section folds the side panel away. A folded panel is shown first. Answers whether it's there.
export async function showSidebar(page) {
  const folded = await firstVisible([page.getByRole("button", { name: "Show sidebar", exact: true })], 300);
  if (folded) await folded.click({ timeout: 5000 }).catch(() => {});
}

export async function openSphere(page, name = "The rig") {
  await showSidebar(page);
  if (await firstVisible([page.getByRole("button", { name: "Members", exact: true })], 300)) return true;
  const tile = await firstVisible([page.getByRole("button", { name, exact: true })], 2000);
  if (tile) await tile.click({ timeout: 5000 }).catch(() => {});
  return Boolean(tile);
}

// A member's row in the side panel's Obysphere ("Chat with dana’s Oby"): their thread opens on a
// real click.
export async function openMember(page, owner) {
  // their thread open already: a click on its row would close it
  const open = page.locator(".room-layer").last().getByRole("textbox", { name: new RegExp(`^Ask .*\\(${owner}\\)`) });
  if (await open.isVisible().catch(() => false)) return null;
  await goHome(page);
  await openSphere(page);
  const row = await firstVisible([page.getByRole("button", { name: new RegExp(`^Chat with ${owner}’s Oby`) })], 60_000);
  if (!row) throw new Error(`No row for ${owner} in the side panel's Obysphere`);
  await row.click();
  return row;
}

// Words asked in the open thread's box ("Ask Dana’s Oby (dana)"): typed, then its Send arrow clicked.
export async function askInThread(page, words) {
  const layer = page.locator(".room-layer").last();
  const box = layer.getByRole("textbox", { name: /^Ask / });
  await box.waitFor({ state: "visible", timeout: 30_000 });
  // a person's pace: the thread settles before they type, and they look before they send
  await page.waitForTimeout(800);
  await box.click();
  await page.keyboard.insertText(words);
  await page.waitForTimeout(300);
  await layer.getByRole("button", { name: "Send" }).click();
}

// Home: every view over it closed, as a person does, with its Back (a thread, a settings page), until
// none is left; then the side panel's Chats.
export async function goHome(page) {
  for (let i = 0; i < 6; i++) {
    const back = await firstVisible([page.getByRole("button", { name: "Back", exact: true })], 300);
    if (!back) break;
    await back.click({ timeout: 5000 }).catch(() => page.keyboard.press("Escape"));
    await page.waitForTimeout(400);
  }
  const chats = await firstVisible([page.getByRole("button", { name: "Chats", exact: true })], 2000);
  if (chats) await chats.click({ timeout: 5000 }).catch(() => {});
  await page.waitForTimeout(300);
}

export const threadText = (page) => page.locator(".room-layer").last().innerText().catch(() => "");

export { until };
