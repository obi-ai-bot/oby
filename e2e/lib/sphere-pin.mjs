// The one obi-sphere commit Oby's Sphere tests run on (RIG-27): e2e/sphere.ref, set to the commit
// the deployed Sphere runs, so a green test says something about the server people reach. The rig
// clones it, `npm run test:sphere` clones it or refuses a checkout at another commit, and moving to
// a newer server is a one-line change to that file, in a pull request.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const PIN_FILE = resolve(dirname(fileURLToPath(import.meta.url)), "../sphere.ref");
// obi-sphere was renamed oby-sphere; GitHub answers both, and the clone names the one it has now.
export const SPHERE_REMOTE = "https://github.com/obi-ai-bot/oby-sphere.git";

// The pinned commit, its 40 hex digits, or a throw that says what the file holds instead.
export function spherePin(file = PIN_FILE) {
  const pin = readFileSync(file, "utf8").trim();
  if (!/^[0-9a-f]{40}$/.test(pin)) throw new Error(`${file} isn't one commit id: ${JSON.stringify(pin.slice(0, 80))}`);
  return pin;
}

// What commit a checkout is at, or null when it isn't one.
export function headOf(dir) {
  try { return execFileSync("git", ["-C", dir, "rev-parse", "HEAD"], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim(); }
  catch { return null; }
}

// `clone` holds obi-sphere at the pin with its server's packages installed; done again from nothing
// when it doesn't, or was cut short: its marker, `<clone>.ready`, is written last. `env` is what git
// and pnpm get of the person's environment. Answers the pin.
export function checkoutAt(clone, { env = process.env, say = () => {}, pin = spherePin() } = {}) {
  const ready = `${clone}.ready`;
  if (existsSync(ready) && readFileSync(ready, "utf8").trim() === pin && headOf(clone) === pin) return pin;
  rmSync(ready, { force: true });
  rmSync(clone, { recursive: true, force: true });
  mkdirSync(clone, { recursive: true });
  say(`Cloning obi-sphere at ${pin.slice(0, 7)} (e2e/sphere.ref) and installing its dependencies…`);
  const git = (...args) => execFileSync("git", ["-C", clone, ...args], { stdio: "inherit", env });
  git("init", "-q");
  git("fetch", "-q", "--depth", "1", SPHERE_REMOTE, pin);
  git("-c", "advice.detachedHead=false", "checkout", "-q", "FETCH_HEAD");
  execFileSync("pnpm", ["install", "--frozen-lockfile", "--silent"], { cwd: join(clone, "server"), stdio: "inherit", env });
  writeFileSync(ready, `${pin}\n`);
  return pin;
}

// For a test handed a checkout (OBI_SPHERE_DIR): null when it's at the pin, else why not. A test
// run on another server says nothing about the deployed one, so it refuses unless told it's meant.
export function offPin(dir, pin = spherePin()) {
  const head = headOf(dir);
  if (head === pin) return null;
  return `${dir} is at ${head ? head.slice(0, 7) : "no commit"}, not ${pin.slice(0, 7)} (e2e/sphere.ref, what the deployed Sphere runs). Leave OBI_SPHERE_DIR out to have it cloned at the pin, or set OBY_SPHERE_ANY_COMMIT=1 to run on it anyway.`;
}
