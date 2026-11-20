// The design's boards beside the real app, for drive-app.mjs's `compare` and `sidebyside` (the
// `app.compare` and `app.sidebyside` nodes, tools/obid-spec/src/app.rs). The boards are the approved
// design's (tools/obid-spec/lab-ui/browser/design, read here, never written); a board is drawn as a
// plain page, as the lab draws it (its vite.config.ts, boardPage), and measured by the same pieces
// the lab measures its screens by (its scripts/drive-lab.mjs, PIECES): the app is held to the board
// as the lab was, so a piece the lab matched and the app doesn't is the port's to explain.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
export const DESIGN = resolve(HERE, '../../../tools/obid-spec/lab-ui/browser/design');

// A board's states, for the one board with logic (Hold: Give back and Take control on the canvas)
const STATES = {
  Hold: {
    held: { held: true, free: false, gaveBack: false, metaLine: 'Waiting for you · Codex · acme-web', cardDot: 'is-held', viewHeld: 'is-held', pwFocus: 'is-focus' },
    gaveBack: { held: false, free: true, gaveBack: true, metaLine: 'Running · Codex · acme-web', cardDot: 'is-working', viewHeld: '', pwFocus: '' },
  },
};

/** A .dc.html board as a plain page in `dir`: no canvas runtime, its images local, the dark theme. Its file URL. */
export function boardFile(name, state, dir) {
  const file = join(DESIGN, 'boards', `${basename(name)}.dc.html`);
  if (!existsSync(file)) throw new Error(`no board ${name} in ${DESIGN}/boards`);
  const src = readFileSync(file, 'utf8');
  const dc = src.split('<x-dc>')[1]?.split('</x-dc>')[0] ?? '';
  // no web fonts: the system font stands first in Oby's stack, on the Mac it is the one drawn
  const helmet = (dc.match(/<helmet>([\s\S]*?)<\/helmet>/)?.[1] ?? '').replace(/<link[^>]*fonts\.googleapis[^>]*>/g, '');
  let body = dc.replace(/<helmet>[\s\S]*?<\/helmet>/, '');
  const vals = { theme: '', true: true, false: false, ...(STATES[name]?.[state ?? Object.keys(STATES[name] ?? {})[0]] ?? {}) };
  // sc-if: kept when its value holds, dropped when not (innermost first)
  for (let i = 0; i < 6; i++) body = body.replace(/<sc-if value="\{\{(\w+)\}\}"[^>]*>((?:(?!<sc-if)[\s\S])*?)<\/sc-if>/g, (_m, k, inner) => (vals[k] ? inner : ''));
  body = body.replace(/\s(onClick)="\{\{\w+\}\}"/g, '').replace(/\{\{(\w+)\}\}/g, (_m, k) => String(vals[k] ?? ''));
  body = body.replace(/\/_blob\/([0-9a-f]{32})/g, (_m, id) => `blobs/${id}.svg`);
  const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><base href="${pathToFileURL(`${DESIGN}/`).href}"><title>${name} (design board)</title>
<link rel="stylesheet" href="oby.css">${helmet}<style>*,*::before,*::after{animation:none!important;transition:none!important}</style></head><body>${body}</body></html>`;
  mkdirSync(dir, { recursive: true });
  const out = join(dir, `board-${basename(name)}${state ? `-${state}` : ''}.html`);
  writeFileSync(out, html);
  return pathToFileURL(out).href;
}

// The pieces a board and the app both have, by the app's and the design's own class names: the
// boxes compared one by one, the words too (the lab's list, drive-lab.mjs).
export const PIECES = [
  '.bar', '.crumb', '.project-chip', '.bar-end .icon-button', '.panel-toggle',
  '.msg-bubble', '.msg-time', '.msg-task', '.msg-task-title', '.msg-task-meta', '.ss-tool-row', '.lt-answered', '.chat-live',
  '.chat-composer', '.composer-field', '.decision-card', '.tc-ask-line', '.tc-ask-text', '.secret-row', '.secret-note',
  '.br', '.br-tabs', '.br-tab', '.br-tabs-end .icon-button', '.br-nav', '.br-address', '.br-control', '.br-control-text', '.br-control .tc-option',
  '.br-view', '.agent-pointer', '.agent-label', '.you-pointer', '.br-shelf', '.br-file', '.br-tip', '.empty-words', '.where-bar', '.peek', '.ran',
  '.br-state', '.br-state-title', '.br-over',
  '.sidebar-slot', '.rail-button', '.sidebar-item', '.sidebar-title', '.connector', '.convo-row', '.convo-kid', '.sidebar-archived',
  '.set-card', '.set-row', '.set-switch', '.set-head', '.set-foot', '.group-title',
  '.site-card', '.site-input', '.site-button', '.site-row', '.plan', '.kpi',
  '.control-pill', '.modal', '.modal-title', '.modal-list li', '.steps', '.modal-actions button',
  '.msg-quote', '.tc-code',
];

// Today's app, as the app's own CSS draws it, and here, the app's own conversation and side panel
// (the boards draw a demo one): compared and reported, never held to the board.
export const TODAY = new Set([
  '.bar', '.crumb', '.project-chip', '.bar-end .icon-button', '.panel-toggle',
  '.msg-bubble', '.msg-time', '.msg-task', '.msg-task-title', '.msg-task-meta', '.ss-tool-row', '.lt-answered', '.chat-live',
  '.chat-composer', '.composer-field', '.msg-quote', '.tc-code',
  '.sidebar-slot', '.rail-button', '.sidebar-item', '.sidebar-title', '.connector', '.convo-row', '.convo-kid', '.sidebar-archived',
  '.group-title',
]);

/** Every piece's boxes and words, in a Playwright page. */
export const pieces = (p) => p.evaluate((sels) => {
  const out = {};
  for (const s of sels) {
    out[s] = [...document.querySelectorAll(s)].filter((e) => e.getClientRects().length).map((e) => {
      const b = e.getBoundingClientRect();
      return { x: b.x, y: b.y, width: b.width, height: b.height, text: e.innerText?.replace(/\s+/g, ' ').trim() ?? '' };
    });
  }
  return out;
}, PIECES);

const round = (n) => Math.round(n * 10) / 10;
const at = (b) => ({ x: round(b.x), y: round(b.y), w: round(b.width), h: round(b.height) });

/** The app's pieces against the board's: every held piece within `tolerance` px and the same words. */
export function judge(board, mine, theirs, { tolerance = 4, skip = [] } = {}) {
  const skipped = new Set(skip);
  const off = [], today = [], words = [], counts = [];
  let worst = 0, compared = 0;
  for (const sel of PIECES) {
    if (skipped.has(sel)) continue;
    const m = mine[sel], t = theirs[sel];
    if (m.length !== t.length) { if (m.length || t.length) (TODAY.has(sel) ? today : counts).push({ piece: sel, app: m.length, board: t.length }); continue; }
    t.forEach((tb, i) => {
      const mb = m[i];
      const d = Math.max(Math.abs(mb.x - tb.x), Math.abs(mb.y - tb.y), Math.abs(mb.width - tb.width), Math.abs(mb.height - tb.height));
      const entry = { piece: `${sel}[${i}]`, by: round(d), app: at(mb), board: at(tb) };
      if (TODAY.has(sel)) { if (d > tolerance) today.push(entry); return; }
      compared++;
      worst = Math.max(worst, d);
      if (d > tolerance) off.push(entry);
      if (mb.text !== tb.text) words.push({ piece: entry.piece, app: mb.text.slice(0, 160), board: tb.text.slice(0, 160) });
    });
  }
  return { board, tolerance, compared, worst: round(worst), within: off.length === 0 && counts.length === 0, off: off.slice(0, 40), offCount: off.length, sameWords: words.length === 0, words: words.slice(0, 40), counts, ignored: [...skipped], today: today.slice(0, 40), todayCount: today.length };
}
