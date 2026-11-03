// docs/contracts/test-rig.md, RIG-1 and RIG-2. Run: node --test e2e/lib/rig-guard.test.mjs
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, realpathSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { folderRefusal, portRefusal, rigRefusal } from "./rig-guard.mjs";

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
      const out = spawnSync(process.execPath, [rig, command], { encoding: "utf8", env: { PATH: process.env.PATH, HOME: home, ...env } });
      assert.equal(out.status, 2, `${name}: ${out.stdout}${out.stderr}`);
      assert.match(out.stderr, /^Refused: /, name);
      assert.deepEqual(readdirSync(join(support, "Oby")).concat(readdirSync(home)), before, name);
    }
  });
}
