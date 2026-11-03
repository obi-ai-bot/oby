// What keeps the test rig (e2e/rig.mjs, docs/contracts/test-rig.md) off the owner's real Oby and
// anyone else's folder: a folder it would use is refused when it is, is inside or holds
// ~/Library/Application Support/Oby, by any spelling (a link, a firmlink, letter case and the other
// spellings macOS takes for the same name: the disk's own identity decides, and the name when the
// folder isn't there). A folder that isn't the rig's is refused, and so is a link among its own.
import { existsSync, lstatSync, readdirSync, readlinkSync, realpathSync, statSync } from "node:fs";
import { homedir, userInfo } from "node:os";
import { basename, dirname, join, resolve, sep } from "node:path";

const OBIDS_OWN_PORT = 7777;
// What the rig makes, deletes, reads or writes through: none of them may lead elsewhere.
const EACH_PERSON = ["", "home", "home/accounts", "home/accounts/stand-in", "home/accounts/no-codex", "home/accounts/no-cursor", "home/pairing", "home/api-token", "gh", "env-home", "tmp", "config.json", "electron-data", "electron-launcher.cjs"];
const OWN_PATHS = [
  ...["you", "dana"].flatMap((who) => EACH_PERSON.map((name) => join(who, name))),
  "sphere", "sphere/state", "sphere/home", "sphere/tmp", "fixture", "fixture/remote.git", "fixture/work", "obi-sphere", "obi-sphere/server", "obi-sphere.ready",
  "sphere.log", "you-obid.log", "dana-obid.log", "app.log", "rig.json", "rig.json.new", "rig.lock",
];

// Both homes: HOME can be changed, and obid goes by the password database's.
export const realObyFolders = () => [...new Set([homedir(), userInfo().homedir])].map((home) => join(home, "Library/Application Support/Oby"));

// The path with every link in it followed, one that leads nowhere too; the part not made yet stays
// as it's written.
function followed(path, links = 40) {
  const rest = [];
  for (let at = resolve(path); ; at = dirname(at)) {
    try { return join(realpathSync(at), ...rest); }
    catch {
      let to = null;
      try { to = readlinkSync(at); } catch {}
      if (to !== null && links > 0) return join(followed(resolve(dirname(at), to), links - 1), ...rest);
      if (dirname(at) === at) return resolve(path);
      rest.unshift(basename(at));
    }
  }
}

const spelled = (path) => path.normalize("NFKC").toLowerCase();
const within = (path, folder) => path === folder || path.startsWith(folder.endsWith(sep) ? folder : folder + sep);
const identity = (path) => { try { const s = statSync(path); return `${s.dev}:${s.ino}`; } catch { return null; } };
const andUp = (path) => { const all = [path]; while (dirname(all.at(-1)) !== all.at(-1)) all.push(dirname(all.at(-1))); return all; };
// Whether `path` is `folder` or inside it, by name or by what the disk says.
const isIn = (path, folder) => within(spelled(path), spelled(folder)) || (identity(folder) !== null && andUp(path).some((p) => identity(p) === identity(folder)));

// Why the rig may not use this folder, or null.
export function folderRefusal(path, real = realObyFolders()) {
  const asked = followed(path);
  for (const folder of real) {
    const oby = followed(folder);
    if (isIn(asked, oby)) return `${path} is the owner's real Oby (${folder}). The rig never reads or writes it.`;
    if (isIn(oby, asked)) return `${path} holds the owner's real Oby (${folder}). The rig never reads or writes it.`;
  }
  return null;
}

// Why the rig may not listen on or follow this port, or null.
export const portRefusal = (port) => (Number(port) === OBIDS_OWN_PORT ? `Port ${port} is the owner's real obid. The rig never uses it.` : null);

// Why this folder isn't the rig's to use: it holds other things and no rig.json, which the rig
// writes first of all. A reset deletes folders by name.
function notTheRigs(dir) {
  if (!existsSync(dir) || existsSync(join(dir, "rig.json"))) return null;
  return readdirSync(dir).some((name) => name !== ".DS_Store") ? `${dir} has other things in it and isn't the rig's. Name an empty folder, or one of the rig's.` : null;
}

// A link among the rig's own paths, or a refused one, or null. The accounts it doesn't name, too:
// Dana's signed-in one has a name obid gave it.
export function aLink(dir, real = realObyFolders()) {
  const accounts = ["you", "dana"].flatMap((who) => {
    const folder = join(who, "home/accounts");
    try { return readdirSync(join(dir, folder)).map((name) => join(folder, name)); } catch { return []; }
  });
  for (const name of [...OWN_PATHS, ...accounts]) {
    const path = join(dir, name);
    let link = false;
    try { link = lstatSync(path).isSymbolicLink(); } catch {}
    if (link) return `${path} is a link: the rig deletes and writes here, so it stays inside its own folder.`;
    const refused = folderRefusal(path, real);
    if (refused) return refused;
  }
  return null;
}

// Why the rig may not run in `dir` with this environment (OBID_HOME and OBID_PORT, which point the
// dev app and a person's shell at an obid), or null.
export function rigRefusal(dir, env = process.env, real = realObyFolders()) {
  return folderRefusal(dir, real)
    ?? (env.OBID_HOME ? folderRefusal(env.OBID_HOME, real) : null)
    ?? (env.OBID_PORT ? portRefusal(env.OBID_PORT) : null)
    ?? notTheRigs(dir)
    ?? aLink(dir, real);
}
