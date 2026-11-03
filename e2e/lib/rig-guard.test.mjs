// docs/contracts/test-rig.md, RIG-1, RIG-2 and RIG-13. Run: node --test e2e/lib/rig-guard.test.mjs
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir, userInfo } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { folderRefusal, portRefusal, realObyFolders, rigRefusal } from "./rig-guard.mjs";

const rig = join(resolve(dirname(fileURLToPath(import.meta.url)), ".."), "rig.mjs");

function aMac(t) {
  const home = realpathSync(mkdtempSync(join(tmpdir(), "oby-rig-guard-")));
  t.after(() => rmSync(home, { recursive: true, force: true }));
  const support = join(home, "Library/Application Support");
  mkdirSync(join(support, "Oby/Runner"), { recursive: true });
  return { home, support, real: [join(support, "Oby")] };
}

test("rig_1_refuses_the_owners_real_oby_by_any_spelling", (t) => {
  const { home, support, real } = aMac(t);
  symlinkSync(join(support, "Oby"), join(home, "link"));
  for (const path of [join(support, "Oby"), join(support, "Oby/Runner"), join(support, "OBY/runner"), join(support, "oby/not-made-yet"), join(home, "link"), join(home, "link/Runner"), `${support}/Oby/../Oby`]) {
    assert.match(folderRefusal(path, real), /is the owner's real Oby/, path);
  }
  // A folder that holds it would have it deleted by a reset.
  for (const path of [support, home]) assert.match(folderRefusal(path, real), /holds the owner's real Oby/, path);
});

test("rig_1_goes_by_either_home", (t) => {
  // HOME can say anything; the password database's home is the one obid goes by. Only read here.
  const { home } = aMac(t);
  const was = process.env.HOME;
  t.after(() => { process.env.HOME = was; });
  process.env.HOME = home;
  const person = join(userInfo().homedir, "Library/Application Support/Oby");
  const folders = realObyFolders();
  assert.ok(folders.includes(person), "the password database's home");
  assert.ok(folders.includes(join(home, "Library/Application Support/Oby")), "HOME's");
  assert.match(folderRefusal(join(person, "Runner")), /real Oby/);
});

test("rig_1_goes_by_what_the_disk_says_not_how_it_is_spelled", { skip: process.platform !== "darwin" }, (t) => {
  const { home, support, real } = aMac(t);
  // A firmlink and a spelling macOS takes for the same name: the same folder by another path.
  for (const path of [`/System/Volumes/Data${join(support, "Oby")}`, `/System/Volumes/Data${join(support, "Oby/Runner")}`, join(home, "Library/Application ſupport/Oby")]) {
    assert.ok(existsSync(path), `${path} isn't the same folder on this disk: the test proves nothing`);
    assert.match(folderRefusal(path, real), /is the owner's real Oby/, path);
  }
  // A link to where it isn't made yet, and under it.
  symlinkSync(join(support, "Oby/not-yet"), join(home, "dangling"));
  for (const path of [join(home, "dangling"), join(home, "dangling/sub")]) assert.match(folderRefusal(path, real), /is the owner's real Oby/, path);
});

test("rig_13_refuses_a_folder_that_isnt_the_rigs", (t) => {
  const { home, real } = aMac(t);
  const broad = join(home, "Projects");
  mkdirSync(join(broad, "someones-work"), { recursive: true });
  assert.match(rigRefusal(broad, {}, real), /isn't the rig's/);
  writeFileSync(join(broad, "rig.json"), "{}");
  assert.equal(rigRefusal(broad, {}, real), null);
  const empty = join(home, "empty");
  mkdirSync(empty);
  writeFileSync(join(empty, ".DS_Store"), "");
  assert.equal(rigRefusal(empty, {}, real), null);
});

test("rig_1_takes_a_folder_of_its_own", (t) => {
  const { support, home, real } = aMac(t);
  for (const path of [join(support, "oby-rig"), join(support, "Oby-release"), join(home, "elsewhere/deep")]) assert.equal(folderRefusal(path, real), null, path);
});

test("rig_1_refuses_obid_home_pointed_at_it", (t) => {
  const { support, real } = aMac(t);
  assert.match(rigRefusal(join(support, "oby-rig"), { OBID_HOME: join(support, "Oby/Runner") }, real), /real Oby/);
  assert.equal(rigRefusal(join(support, "oby-rig"), { OBID_HOME: join(support, "oby-rig/you/home") }, real), null);
});

test("rig_2_refuses_the_real_obids_port", () => {
  assert.match(portRefusal(7777), /real obid/);
  assert.match(portRefusal("7777"), /real obid/);
  assert.equal(portRefusal(8794), null);
  assert.match(rigRefusal("/tmp/not-oby", { OBID_PORT: "7777" }, []), /real obid/);
});

// The command itself: HOME stands in for the person's, so a guard that broke would write there, and
// the test sees it.
for (const command of ["start", "stop", "reset", "status", "app"]) {
  test(`rig_1_${command}_refuses_before_it_reads_or_writes_anything`, (t) => {
    const { home, support } = aMac(t);
    for (const [name, env] of [
      ["its folder", { OBY_RIG_DIR: join(support, "Oby/Runner") }],
      ["a folder that holds it", { OBY_RIG_DIR: support }],
      ["OBID_HOME", { OBY_RIG_DIR: join(home, "rig"), OBID_HOME: join(support, "Oby/Runner") }],
      ["OBID_PORT", { OBY_RIG_DIR: join(home, "rig"), OBID_PORT: "7777" }],
    ]) {
      const before = readdirSync(join(support, "Oby")).concat(readdirSync(home));
      const out = spawnSync(process.execPath, [rig, command], { encoding: "utf8", env: { PATH: process.env.PATH, HOME: home, OBID_BIN: join(home, "no-obid"), ...env } }); // a broken guard's start stops there
      assert.equal(out.status, 2, `${name}: ${out.stdout}${out.stderr}`);
      assert.match(out.stderr, /^Refused: /, name);
      assert.deepEqual(readdirSync(join(support, "Oby")).concat(readdirSync(home)), before, name);
    }
  });
}
