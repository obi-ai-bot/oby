import { useEffect, useState, type AnchorHTMLAttributes, type ReactNode } from 'react';
import { cn } from '@/lib/utils';

/** The hash route: #/endpoints/GET%20%2Fhome?q=x → { path: ['endpoints', 'GET /home'], query }. */
export interface Route { path: string[]; query: URLSearchParams; raw: string }

function parse(hash: string): Route {
  const raw = hash.replace(/^#/, '') || '/';
  const [p, q = ''] = raw.split('?');
  return { path: p.split('/').filter(Boolean).map(decodeURIComponent), query: new URLSearchParams(q), raw };
}

export function useRoute(): Route {
  const [route, setRoute] = useState(() => parse(window.location.hash));
  useEffect(() => {
    const on = () => setRoute(parse(window.location.hash));
    window.addEventListener('hashchange', on);
    return () => window.removeEventListener('hashchange', on);
  }, []);
  return route;
}

/** Scrolls the element with this id into view once it renders (a job, a migration named in the route). */
export function useScrollTo(id: string | undefined): void {
  useEffect(() => {
    if (!id) return;
    const t = setTimeout(() => document.getElementById(id)?.scrollIntoView({ block: 'start', behavior: 'smooth' }), 50);
    return () => clearTimeout(t);
  }, [id]);
}

/** Builds a hash link from segments: href('endpoints', 'GET /home') → '#/endpoints/GET%20%2Fhome'. */
export function href(...segments: string[]): string {
  return `#/${segments.map(encodeURIComponent).join('/')}`;
}

export function navigate(to: string, replace = false): void {
  const target = to.startsWith('#') ? to : `#${to}`;
  if (replace) window.history.replaceState(null, '', target);
  else window.location.hash = target;
  if (replace) window.dispatchEvent(new HashChangeEvent('hashchange'));
}

/** Updates the query of the current route without a new history entry. */
export function setQuery(route: Route, changes: Record<string, string | null>): void {
  const q = new URLSearchParams(route.query);
  for (const [k, v] of Object.entries(changes)) if (v === null || v === '') q.delete(k); else q.set(k, v);
  const path = `/${route.path.map(encodeURIComponent).join('/')}`;
  navigate(`${path}${q.toString() ? `?${q}` : ''}`, true);
}

export function Link({ to, className, children, ...rest }: { to: string; children: ReactNode } & AnchorHTMLAttributes<HTMLAnchorElement>) {
  return <a href={to} className={cn('hover:underline underline-offset-4', className)} {...rest}>{children}</a>;
}
