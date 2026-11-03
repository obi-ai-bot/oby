// What keeps the test rig (e2e/rig.mjs, docs/contracts/test-rig.md) off the owner's real Oby: a
// folder it would use is refused when it is, is inside or holds ~/Library/Application Support/Oby,
// by any spelling (a link, letter case: macOS ignores it). A port is refused when it's obid's own.
import { realpathSync } from "node:fs";
import { homedir, userInfo } from "node:os";
import { basename, dirname, join, resolve, sep } from "node:path";

const OBIDS_OWN_PORT = 7777;

// Both homes: HOME can be changed, and obid goes by the password database's.
export const realObyFolders = () => [...new Set([homedir(), userInfo().homedir])].map((home) => join(home, "Library/Application Support/Oby"));

// The path with every link in it followed; the part not made yet stays as it's written.
function followed(path) {
  const rest = [];
  for (let at = resolve(path); ; at = dirname(at)) {
    try { return join(realpathSync(at), ...rest); }
    catch { if (dirname(at) === at) return resolve(path); rest.unshift(basename(at)); }
  }
}

const within = (path, folder) => path === folder || path.startsWith(folder.endsWith(sep) ? folder : folder + sep);

// Why the rig may not use this folder, or null.
export function folderRefusal(path, real = realObyFolders()) {
  const asked = followed(path).toLowerCase();
  for (const folder of real) {
    const oby = followed(folder).toLowerCase();
    if (within(asked, oby)) return `${path} is the owner's real Oby (${folder}). The rig never reads or writes it.`;
    if (within(oby, asked)) return `${path} holds the owner's real Oby (${folder}). The rig never reads or writes it.`;
  }
  return null;
}

// Why the rig may not listen on or follow this port, or null.
export const portRefusal = (port) => (Number(port) === OBIDS_OWN_PORT ? `Port ${port} is the owner's real obid. The rig never uses it.` : null);

// Why the rig may not start in `dir` with this environment (OBID_HOME and OBID_PORT, which point
// the dev app and a person's shell at an obid), or null.
export function rigRefusal(dir, env = process.env, real = realObyFolders()) {
  return folderRefusal(dir, real)
    ?? (env.OBID_HOME ? folderRefusal(env.OBID_HOME, real) : null)
    ?? (env.OBID_PORT ? portRefusal(env.OBID_PORT) : null);
}
