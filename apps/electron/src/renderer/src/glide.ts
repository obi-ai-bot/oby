import { useLayoutEffect, useRef, type RefObject } from 'react';

// Things in a row that come and go (the message box's tags): one taken out shrinks and fades
// (leave, 140ms), and those after it glide to where they now belong (260ms, the warm ease), as a
// new one fades in (180ms): brief, so the change is felt, never waited on. Each thing is marked
// data-glide with its key. Nothing moves for anyone who'd rather nothing moved.
const WARM = 'cubic-bezier(0.22, 1, 0.36, 1)', EASE_IN = 'cubic-bezier(0.55, 0, 0.75, 0.3)';  // tokens.css --warm, --ease-in
const still = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

// sig: the keys in order, so it measures again whenever they change
export function useGlide(box: RefObject<HTMLElement | null>, sig: string) {
  const last = useRef(new Map<string, DOMRect>());
  const first = useRef(true);
  useLayoutEffect(() => {
    const row = box.current;
    // (those on their way out are left to their leave: cancelling it would keep them)
    const els = row ? [...row.querySelectorAll<HTMLElement>('[data-glide]:not([data-leaving])')] : [];
    // where each is now, against the row and its scroll (the row itself may move, as a box anchored
    // at its bottom grows a line; past 3 lines it scrolls): any glide still going is ended first, so it's where it really is; all read
    // before anything moves
    els.forEach((el) => el.getAnimations().forEach((a) => a.cancel()));
    const origin = row?.getBoundingClientRect();
    const now = new Map<string, DOMRect>(els.map((el) => {
      const r = el.getBoundingClientRect();
      return [el.dataset.glide!, new DOMRect(r.left - origin!.left + row!.scrollLeft, r.top - origin!.top + row!.scrollTop, r.width, r.height)];
    }));
    if (!still()) els.forEach((el) => {
      const key = el.dataset.glide!, at = now.get(key)!, was = last.current.get(key);
      if (was) {
        const dx = was.left - at.left, dy = was.top - at.top;
        if (dx || dy) el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'none' }], { duration: 260, easing: WARM });
      } else if (!first.current) {
        el.animate([{ opacity: 0, transform: 'scale(0.92)' }, { opacity: 1, transform: 'none' }], { duration: 180, easing: WARM });
      }
    });
    last.current = now;
    first.current = false;
  }, [sig]);
}

// one taken out: it shrinks and fades, then then() takes it out (and the rest glide)
export function leave(el: Element | null, then: () => void) {
  if (!(el instanceof HTMLElement) || still()) return then();
  el.style.pointerEvents = 'none';
  el.dataset.leaving = '';
  el.animate([{ opacity: 1, transform: 'none' }, { opacity: 0, transform: 'scale(0.92)' }], { duration: 140, easing: EASE_IN, fill: 'forwards' }).onfinish = then;
}
