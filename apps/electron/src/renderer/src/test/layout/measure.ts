// What a person would see as broken, measured in the page (mock.ts lays it out): controls drawn on each
// other, a Copy next to another Copy, words cut off. Kept as page code in a string, so the test's own
// build (esbuild's helpers) never leaks into what runs in the browser.
import type { Page } from 'playwright-core';

export interface Box { what: string; x: number; y: number; w: number; h: number }
export interface Overlap { a: Box; b: Box }
export interface Near { a: Box; b: Box; gap: number }
export interface Clipped { what: string; text: string; over: string }
export interface Report { overlaps: Overlap[]; copiesNear: Near[]; copyOver: Overlap[]; clipped: Clipped[]; wider: number }

// controls a pointer can hit, and the cards a control may not sit on
export const CONTROLS = 'button, a[href], input:not([type=hidden]), textarea, select, [role=button], [role=menuitem], [role=menuitemradio], [role=menuitemcheckbox], [role=option], [role=tab], [role=switch], [role=checkbox], [role=radio], [contenteditable=true]';
export const CARDS = '.msg-task, .ask-card, .lt-card';
export const COPY = 'button[aria-label="Copy"], button[aria-label="Copied"]';
// closer than this, two Copies read as one control drawn twice
export const COPY_ROOM = 24;

const PAGE = String.raw`(() => {
  const CONTROLS = ${JSON.stringify(CONTROLS)}, CARDS = ${JSON.stringify(CARDS)}, COPY = ${JSON.stringify(COPY)}, ROOM = ${COPY_ROOM};
  const name = (e) => {
    const label = e.getAttribute('aria-label') || (e.innerText || e.value || '').trim().replace(/\s+/g, ' ').slice(0, 40);
    const cls = typeof e.className === 'string' && e.className ? '.' + e.className.trim().split(/\s+/).slice(0, 2).join('.') : '';
    return e.tagName.toLowerCase() + cls + (label ? ' "' + label + '"' : '');
  };
  // where it is drawn: its box cut by every ancestor that clips (a scroll area, a fold), else null
  const drawn = (e, evenHidden) => {
    let r = e.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) return null;
    let x0 = r.left, y0 = r.top, x1 = r.right, y1 = r.bottom;
    for (let a = e; a; a = a.parentElement) {
      const s = getComputedStyle(a);
      if (s.display === 'none' || s.visibility === 'hidden' || s.visibility === 'collapse') return null;
      if (!evenHidden && Number(s.opacity) < 0.05) return null;
      if (a !== e && /(hidden|clip|auto|scroll)/.test(s.overflowX + s.overflowY)) {
        const c = a.getBoundingClientRect();
        x0 = Math.max(x0, c.left); y0 = Math.max(y0, c.top); x1 = Math.min(x1, c.right); y1 = Math.min(y1, c.bottom);
      }
    }
    x0 = Math.max(x0, 0); y0 = Math.max(y0, 0); x1 = Math.min(x1, innerWidth); y1 = Math.min(y1, innerHeight);
    return x1 - x0 >= 1 && y1 - y0 >= 1 ? { x0, y0, x1, y1 } : null;
  };
  const box = (e, d) => ({ what: name(e), x: Math.round(d.x0), y: Math.round(d.y0), w: Math.round(d.x1 - d.x0), h: Math.round(d.y1 - d.y0) });
  // overlapping by more than a border's width either way
  const over = (p, q) => Math.min(p.x1, q.x1) - Math.max(p.x0, q.x0) > 2 && Math.min(p.y1, q.y1) - Math.max(p.y0, q.y0) > 2;
  const related = (a, b) => a === b || a.contains(b) || b.contains(a);
  // the topmost thing at a point is it (or inside it): what a person sees there
  const onTop = (e, d) => {
    const x = (d.x0 + d.x1) / 2, y = (d.y0 + d.y1) / 2, t = document.elementFromPoint(x, y);
    return !!t && (e === t || e.contains(t) || t.contains(e));
  };
  window.__layout = (rootSel) => {
    const roots = rootSel ? [...document.querySelectorAll(rootSel)] : [document.body];
    const inRoots = (sel) => roots.flatMap((r) => [...r.querySelectorAll(sel)]);
    const controls = inRoots(CONTROLS).map((e) => ({ e, d: drawn(e, false) })).filter((c) => c.d);
    const overlaps = [];
    for (let i = 0; i < controls.length; i++) for (let j = i + 1; j < controls.length; j++) {
      const a = controls[i], b = controls[j];
      if (related(a.e, b.e) || !over(a.d, b.d)) continue;
      // one under the other (a menu over the page, a dialog over its screen): only the top one shows
      if (!(onTop(a.e, a.d) && onTop(b.e, b.d))) continue;
      overlaps.push({ a: box(a.e, a.d), b: box(b.e, b.d) });
    }
    // Copies: the ones showing, never closer than ROOM to each other; any of them, shown or not
    // (a hidden button still takes the click), never on another control or a card
    const copies = inRoots(COPY);
    const shown = copies.map((e) => ({ e, d: drawn(e, false) })).filter((c) => c.d);
    const copiesNear = [];
    for (let i = 0; i < shown.length; i++) for (let j = i + 1; j < shown.length; j++) {
      const p = shown[i].d, q = shown[j].d;
      const gap = Math.hypot(Math.max(0, p.x0 - q.x1, q.x0 - p.x1), Math.max(0, p.y0 - q.y1, q.y0 - p.y1));
      if (gap < ROOM) copiesNear.push({ a: box(shown[i].e, p), b: box(shown[j].e, q), gap: Math.round(gap) });
    }
    const copyOver = [];
    const targets = inRoots(CONTROLS + ', ' + CARDS).map((e) => ({ e, d: drawn(e, true) })).filter((c) => c.d);
    for (const c of copies) {
      const d = drawn(c, true);
      if (!d) continue;
      for (const t of targets) if (!related(c, t.e) && !t.e.matches(COPY) && over(d, t.d)) copyOver.push({ a: box(c, d), b: box(t.e, t.d) });
    }
    // words cut off: an element holding text whose box clips it, unless it says so (an ellipsis, a
    // line clamp, a fade, a scroll area)
    const clipped = [];
    for (const r of roots) for (const e of r.querySelectorAll('*')) {
      if (![...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim())) continue;
      const s = getComputedStyle(e);
      if (!/(hidden|clip)/.test(s.overflowX + s.overflowY)) continue;
      if (s.textOverflow === 'ellipsis' || s.webkitLineClamp !== 'none' || (s.maskImage && s.maskImage !== 'none')) continue;
      if (!drawn(e, false)) continue;
      const wide = e.scrollWidth - e.clientWidth > 1 && /(hidden|clip)/.test(s.overflowX);
      const tall = e.scrollHeight - e.clientHeight > 1 && /(hidden|clip)/.test(s.overflowY);
      if (wide || tall) clipped.push({ what: name(e), text: (e.innerText || '').trim().slice(0, 60), over: (wide ? 'width ' + (e.scrollWidth - e.clientWidth) + 'px ' : '') + (tall ? 'height ' + (e.scrollHeight - e.clientHeight) + 'px' : '') });
    }
    return { overlaps, copiesNear, copyOver, clipped, wider: Math.max(0, document.documentElement.scrollWidth - innerWidth) };
  };
})()`;

// Everything inside `root` (a selector; the whole page by default), as drawn now
export async function measure(page: Page, root?: string): Promise<Report> {
  await page.evaluate(PAGE);
  return page.evaluate((r) => (window as unknown as { __layout: (r?: string) => Report }).__layout(r), root);
}

export const say = (r: Pick<Report, 'overlaps' | 'copiesNear' | 'copyOver'>) => [
  ...r.overlaps.map((o) => `overlap: ${o.a.what} @${o.a.x},${o.a.y} on ${o.b.what} @${o.b.x},${o.b.y}`),
  ...r.copiesNear.map((o) => `two Copies ${o.gap}px apart: @${o.a.x},${o.a.y} and @${o.b.x},${o.b.y}`),
  ...r.copyOver.map((o) => `Copy @${o.a.x},${o.a.y} on ${o.b.what} @${o.b.x},${o.b.y} ${o.b.w}x${o.b.h}`),
];
