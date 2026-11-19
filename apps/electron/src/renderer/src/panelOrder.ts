// The side panel's conversations in the order it lists them, for ⌘1 to ⌘9 and ⌘⇧[ ⌘⇧]
// (docs/contracts/shortcuts.md, SHORTCUTS-8 and 9), as T3 Code jumps through its sidebar's threads
// (apps/web/src/components/Sidebar.logic.ts, resolveAdjacentThreadId).

type Row = { id: string; pinned?: boolean; project?: string | null };
export type Group<T extends Row> = { id: string; name: string; rows: T[] };

// pinned ones first, the rest as they came
export const pinnedFirst = <T extends Row>(rows: T[]) => [...rows.filter((c) => c.pinned), ...rows.filter((c) => !c.pinned)];

// The Projects side's groups (PROJECTS-16 to 18): each project's rows in that order, then those in
// none (or in one obid doesn't list); a project with none still shows, but not while searching, and
// No project only with rows. None listed, or projects off: no groups
export function projectGroups<T extends Row>(rows: T[], projects: { id: string; name: string }[], on: boolean, searching: boolean): Group<T>[] {
  if (!on || !projects.length) return [];
  return [
    ...projects.map((p) => ({ id: p.id, name: p.name, rows: rows.filter((c) => c.project === p.id) })),
    { id: '', name: 'No project', rows: rows.filter((c) => !projects.some((p) => p.id === c.project)) },
  ].filter((g) => g.rows.length || (g.id && !searching));
}

// The ids as the panel lists them unsearched: by project when it groups them, a folded project's
// left out (they aren't on screen), else pinned first
export function panelOrder<T extends Row>(rows: T[], groups: Group<T>[], folded: Set<string>): string[] {
  return (groups.length ? groups.filter((g) => !folded.has(g.id)).flatMap((g) => g.rows) : rows).map((c) => c.id);
}

// The one before or after the open one; none open: the first going on, the last going back; at an
// end, none (T3's)
export function adjacent(ids: string[], current: string | null, direction: 'previous' | 'next'): string | null {
  if (!ids.length) return null;
  if (current === null) return direction === 'previous' ? ids.at(-1)! : ids[0];
  const i = ids.indexOf(current);
  if (i < 0) return direction === 'previous' ? ids.at(-1)! : ids[0];
  return (direction === 'previous' ? ids[i - 1] : ids[i + 1]) ?? null;
}
