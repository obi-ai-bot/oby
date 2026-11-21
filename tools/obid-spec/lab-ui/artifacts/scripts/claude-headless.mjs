#!/usr/bin/env node
// The Claude Code headless spike (plan §3.8; LAB-ARTIFACTS-20), behind `lab.artifacts.claude_headless`
// (src/lab/artifacts.rs), which never fails a run: it only reports.
//
//   node claude-headless.mjs <out-folder>
//
// It runs the owner's own Claude Code (`claude` on PATH, on its own login, as Oby's own lane does:
// no ANTHROPIC_API_KEY, none of a parent Claude Code's variables) in empty folders of its own:
//
//   1. `system/init` of `claude -p` in stream-json, with and without `--allowedTools Artifact`:
//      whether Claude's Artifact tool is offered headless (Claude can't publish or read its
//      artifacts from Oby until it is).
//   2. A design made headless from Oby's own guide (../guide.md) alone: Read, Write and Edit only,
//      edits accepted, no command run. The time, turns, tokens and cost, its files and tool calls.
//      `lab.artifactview.check` then draws it with Oby's runtime.
//
// It writes <out-folder>/result.json and the design under <out-folder>/work/design/.
import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync, copyFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const out = process.argv[2];
if (!out) {
  console.error('usage: node claude-headless.mjs <out-folder>');
  process.exit(2);
}
mkdirSync(out, { recursive: true });
const here = dirname(fileURLToPath(import.meta.url));

// The owner's Claude Code as Oby starts it: its own login, nothing of this process's own Claude.
const env = Object.fromEntries(
  Object.entries(process.env).filter(([k]) => !/^(ANTHROPIC_API_KEY|ANTHROPIC_AUTH_TOKEN|CLAUDECODE|CLAUDE_CODE_.*)$/.test(k)),
);

function run(args, cwd, timeoutMs) {
  return new Promise((resolve) => {
    const started = Date.now();
    const p = spawn('claude', args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    p.stdout.on('data', (d) => (stdout += d));
    p.stderr.on('data', (d) => (stderr += d));
    const timer = setTimeout(() => p.kill('SIGTERM'), timeoutMs);
    p.on('error', (e) => resolve({ code: -1, stdout, stderr: String(e), seconds: 0 }));
    p.on('close', (code) => {
      clearTimeout(timer);
      resolve({ code, stdout, stderr, seconds: (Date.now() - started) / 1000 });
    });
  });
}

function lines(stdout) {
  return stdout
    .split('\n')
    .filter((l) => l.trim().startsWith('{'))
    .map((l) => {
      try {
        return JSON.parse(l);
      } catch {
        return null;
      }
    })
    .filter(Boolean);
}

function init(stdout) {
  const i = lines(stdout).find((m) => m.type === 'system' && m.subtype === 'init');
  if (!i) return null;
  return {
    claudeCode: i.claude_code_version ?? null,
    model: i.model ?? null,
    tools: i.tools ?? [],
    artifact: (i.tools ?? []).some((t) => /^Artifact/i.test(t)),
  };
}

function result(stdout) {
  const msgs = lines(stdout);
  const r = msgs.find((m) => m.type === 'result') ?? {};
  const uses = [];
  const failed = [];
  for (const m of msgs) {
    for (const c of m.message?.content ?? []) {
      if (c.type === 'tool_use') uses.push(c.name);
      if (c.type === 'tool_result' && c.is_error) failed.push(String(typeof c.content === 'string' ? c.content : JSON.stringify(c.content)).slice(0, 300));
    }
  }
  const u = r.usage ?? {};
  return {
    subtype: r.subtype ?? null,
    isError: r.is_error ?? null,
    turns: r.num_turns ?? null,
    seconds: r.duration_ms != null ? r.duration_ms / 1000 : null,
    apiSeconds: r.duration_api_ms != null ? r.duration_api_ms / 1000 : null,
    costUsd: r.total_cost_usd ?? null,
    tokens: {
      input: u.input_tokens ?? null,
      output: u.output_tokens ?? null,
      cacheRead: u.cache_read_input_tokens ?? null,
      cacheCreation: u.cache_creation_input_tokens ?? null,
    },
    toolUses: uses,
    toolErrors: failed,
    said: String(r.result ?? '').slice(0, 600),
  };
}

function files(dir) {
  const all = [];
  const walk = (d) => {
    for (const n of readdirSync(d)) {
      const p = join(d, n);
      if (statSync(p).isDirectory()) walk(p);
      else all.push({ path: relative(dir, p), bytes: statSync(p).size });
    }
  };
  try {
    walk(dir);
  } catch {}
  return all;
}

const report = { at: new Date().toISOString() };

const version = await run(['--version'], out, 30_000);
report.claude = { ran: version.code === 0, version: version.stdout.trim() || null };
if (version.code !== 0) {
  report.skipped = `Claude Code didn't run here: ${version.stderr.trim().slice(0, 200)}`;
  writeFileSync(join(out, 'result.json'), JSON.stringify(report, null, 2));
  process.exit(0);
}

// 1. Is Claude's Artifact tool offered headless?
for (const [key, extra] of [['init', []], ['initWithArtifact', ['--allowedTools', 'Artifact']]]) {
  const dir = join(out, key);
  mkdirSync(dir, { recursive: true });
  const r = await run(['-p', 'Reply with the word ok.', '--output-format', 'stream-json', '--verbose', '--max-turns', '1', ...extra], dir, 180_000);
  report[key] = { ...init(r.stdout), ...result(r.stdout), exit: r.code, stderr: r.stderr.trim().slice(0, 300) };
}

// 2. A design from Oby's guide alone.
const work = join(out, 'work');
mkdirSync(work, { recursive: true });
copyFileSync(join(here, '..', 'guide.md'), join(work, 'GUIDE.md'));
const ask = [
  'Read GUIDE.md: it is the whole format. Make an Oby design in a folder named design/ here:',
  'a sign-in flow for a phone (390×844) with three boards: the sign-in screen (its fields from a list,',
  'a Sign in button that shows an error after a tap), a "forgot password" screen the first links to,',
  'and a small shared card board both screens import with dc-import. Put the three in canvas.json on one page.',
  'Write only files under design/. Run no command. When done, list the files you wrote.',
].join(' ');
const made = await run(
  ['-p', ask, '--output-format', 'stream-json', '--verbose', '--max-turns', '30', '--allowedTools', 'Read,Write,Edit,Glob', '--permission-mode', 'acceptEdits'],
  work,
  900_000,
);
report.make = { ...result(made.stdout), exit: made.code, stderr: made.stderr.trim().slice(0, 300), files: files(join(work, 'design')) };
report.design = join(work, 'design');
writeFileSync(join(out, 'result.json'), JSON.stringify(report, null, 2));
console.log(JSON.stringify({ artifactHeadless: report.init?.artifact, artifactAllowed: report.initWithArtifact?.artifact, made: report.make.files.length }));
