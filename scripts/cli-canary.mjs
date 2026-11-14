#!/usr/bin/env node
// The CLI canary's report and its two-in-a-row rule (docs/contracts/ci.md, CI-14 to 16; docs/ci.md,
// "The CLI canary"), for .github/workflows/cli-canary.yml. Node alone, no npm package.
//
//   node scripts/cli-canary.mjs report --results <dir> --run <id> --out <file>
//                                      [--record <cli>/<case>=passed|failed]...
//     Writes the run's report, {"run", "clis": {<cli>: {"version", "cases": {<case>: {"result",
//     "why"}}}}}, from the results folder: each case's `<cli>/<case>.json` (what
//     tests/cli_canary.rs writes) and each CLI's `<cli>.version` (its `--version` output; none:
//     the CLI isn't installed). A case with no result is `skipped`: `not installed`, the text of
//     CLI_CANARY_SKIP, or `no result`.
//
//   node scripts/cli-canary.mjs drift --current <file> --previous <file> [--repo <owner/name>]
//     For each CLI, version and case `failed` in both reports, makes sure one open issue labelled
//     `cli-drift` names it: opens one when none is open, else comments on it with the pairs it
//     doesn't name yet. A missing previous report opens nothing. Calls `gh` (or $GH).
//
// The reports are data a CLI's run could have written: only the names in tests/cli_canary.rs's
// `CASES`, a version that matches VERSION and a run id of digits ever reach an issue, never a
// CLI's output or a case's `why`.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const VERSION = /^[0-9A-Za-z][0-9A-Za-z.+_-]{0,63}$/;
const RESULTS = ["passed", "failed", "skipped"];
const LABEL = "cli-drift";

// Every CLI and its cases, read from tests/cli_canary.rs's `CASES`, where they are written once.
function cases() {
  const source = fs.readFileSync(path.join(ROOT, "core/crates/obid/tests/cli_canary.rs"), "utf8");
  const block = source.match(/const CASES: [^=]*= &\[([\s\S]*?)\n\];/);
  if (!block) throw new Error("tests/cli_canary.rs has no CASES list");
  const list = new Map();
  for (const [, cli, names] of block[1].matchAll(/\(\s*"([a-z-]+)",\s*&\[([^\]]*)\],?\s*\)/g)) {
    list.set(cli, [...names.matchAll(/"([a-z_]+)"/g)].map((m) => m[1]));
  }
  if (list.size === 0) throw new Error("tests/cli_canary.rs's CASES names no CLI");
  return list;
}

function options(args) {
  const named = { record: [] };
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i]?.replace(/^--/, "");
    const value = args[i + 1];
    if (!key || value === undefined) throw new Error(`a value is missing after ${args[i]}`);
    if (key === "record") named.record.push(value);
    else named[key] = value;
  }
  return named;
}

// The version in a `--version` output: its first word that starts with a digit, if it is one.
function versionOf(output) {
  const word = output.split(/\s+/).find((w) => /^[0-9]/.test(w));
  return word && VERSION.test(word) ? word : "unknown";
}

function cut(text) {
  const line = String(text ?? "").split("\n")[0];
  return line.length > 200 ? `${line.slice(0, 199)}…` : line;
}

function report(args) {
  const { results, run, out, record } = options(args);
  if (!results || !run || !out) throw new Error("report needs --results, --run and --out");
  const recorded = new Map();
  for (const entry of record) {
    const m = entry.match(/^([a-z-]+)\/([a-z_]+)=(passed|failed)$/);
    if (!m) throw new Error(`--record ${entry}: not <cli>/<case>=passed|failed`);
    recorded.set(`${m[1]}/${m[2]}`, m[3]);
  }
  const skip = process.env.CLI_CANARY_SKIP;
  const clis = {};
  for (const [cli, names] of cases()) {
    const versionFile = path.join(results, `${cli}.version`);
    const installed = fs.existsSync(versionFile);
    const version = installed ? versionOf(fs.readFileSync(versionFile, "utf8")) : "not installed";
    const entry = { version, cases: {} };
    for (const name of names) {
      const given = recorded.get(`${cli}/${name}`);
      const file = path.join(results, cli, `${name}.json`);
      let result;
      if (given) {
        result = { result: given, why: given === "failed" ? "the test failed" : "" };
      } else if (fs.existsSync(file)) {
        try {
          const read = JSON.parse(fs.readFileSync(file, "utf8"));
          result = RESULTS.includes(read.result)
            ? { result: read.result, why: cut(read.why) }
            : { result: "failed", why: "its result can't be read" };
        } catch {
          result = { result: "failed", why: "its result can't be read" };
        }
      } else if (!installed) {
        result = { result: "skipped", why: "not installed" };
      } else {
        result = { result: "skipped", why: skip ? cut(skip) : "no result" };
      }
      entry.cases[name] = result;
    }
    clis[cli] = entry;
  }
  const whole = { run: /^[0-9]+$/.test(run) ? Number(run) : run, clis };
  fs.writeFileSync(out, JSON.stringify(whole, null, 2) + "\n");
  for (const [cli, entry] of Object.entries(clis)) {
    for (const [name, r] of Object.entries(entry.cases)) {
      console.log(`${cli} ${entry.version} ${name}: ${r.result}${r.why ? ` (${r.why})` : ""}`);
    }
  }
}

function gh(args) {
  const ran = spawnSync(process.env.GH || "gh", args, { encoding: "utf8" });
  if (ran.status !== 0) {
    throw new Error(`gh ${args.slice(0, 2).join(" ")} failed: ${ran.stderr || ran.error || ""}`.trim());
  }
  return ran.stdout;
}

function runLink(repo, run) {
  const server = process.env.GITHUB_SERVER_URL || "https://github.com";
  return /^[0-9]+$/.test(String(run)) ? `${server}/${repo}/actions/runs/${run}` : "an unknown run";
}

// The pairs that failed in both reports on the same version, as `<cli> <version> <case>`.
function twiceFailed(previous, current) {
  const pairs = [];
  for (const [cli, names] of cases()) {
    const [before, now] = [previous?.clis?.[cli], current?.clis?.[cli]];
    const version = now?.version;
    if (typeof version !== "string" || !VERSION.test(version) || version === "unknown") continue;
    if (before?.version !== version) continue;
    for (const name of names) {
      if (before?.cases?.[name]?.result === "failed" && now?.cases?.[name]?.result === "failed") {
        pairs.push(`${cli} ${version} ${name}`);
      }
    }
  }
  return pairs;
}

function drift(args) {
  const { current, previous, repo = process.env.GITHUB_REPOSITORY } = options(args);
  if (!current || !previous || !repo) throw new Error("drift needs --current, --previous and --repo");
  if (!fs.existsSync(previous)) {
    console.log("No previous report: nothing to compare.");
    return;
  }
  const [before, now] = [previous, current].map((f) => JSON.parse(fs.readFileSync(f, "utf8")));
  const pairs = twiceFailed(before, now);
  if (pairs.length === 0) {
    console.log("No case failed twice in a row on the same version.");
    return;
  }
  const runs = `Runs: ${runLink(repo, before.run)} and ${runLink(repo, now.run)}.`;
  const list = (named) => named.map((p) => `- ${p}`).join("\n");
  const open = JSON.parse(
    gh(["issue", "list", "--repo", repo, "--label", LABEL, "--state", "open",
        "--json", "number,body,comments", "--limit", "100"]),
  );
  if (open.length === 0) {
    gh(["label", "create", LABEL, "--repo", repo, "--force", "--color", "d93f0b",
        "--description", "The CLI canary found a CLI that fails twice in a row"]);
    const body = `The CLI canary failed these cases twice in a row on the same version:\n\n${list(pairs)}\n\n${runs}\n`;
    gh(["issue", "create", "--repo", repo, "--label", LABEL, "--title", `CLI drift: ${pairs[0]}`, "--body", body]);
    console.log(`Opened a ${LABEL} issue: ${pairs.join(", ")}.`);
    return;
  }
  const issue = open.sort((a, b) => a.number - b.number)[0];
  if (!Number.isInteger(issue.number)) throw new Error("gh issue list gave an issue with no number");
  const said = [issue.body ?? "", ...(issue.comments ?? []).map((c) => c?.body ?? "")]
    .join("\n")
    .split("\n");
  const fresh = pairs.filter((p) => !said.includes(`- ${p}`));
  if (fresh.length === 0) {
    console.log(`Issue #${issue.number} already names ${pairs.join(", ")}.`);
    return;
  }
  const body = `These cases failed twice in a row on the same version too:\n\n${list(fresh)}\n\n${runs}\n`;
  gh(["issue", "comment", String(issue.number), "--repo", repo, "--body", body]);
  console.log(`Commented on #${issue.number}: ${fresh.join(", ")}.`);
}

const [command, ...rest] = process.argv.slice(2);
try {
  if (command === "report") report(rest);
  else if (command === "drift") drift(rest);
  else throw new Error("usage: cli-canary.mjs report|drift …");
} catch (error) {
  console.error(`cli-canary: ${error.message}`);
  process.exit(1);
}
