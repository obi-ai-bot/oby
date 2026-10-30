import type { ReactNode } from 'react';
import { PageHeader, Section } from '@/components/brain/page';
import { OriginBadge } from '@/components/brain/badges';
import { IssueLink, ModuleLink, ScreenLink } from '@/components/brain/links';
import { useBrain } from '@/lib/brain';
import { href, Link } from '@/lib/router';
import { CHECK_LABEL } from './quality';

function Stat({ to, value, label, sub }: { to: string; value: ReactNode; label: string; sub?: ReactNode }) {
  return (
    <a href={to} className="rounded-xl border bg-card p-4 transition-colors hover:bg-accent/40">
      <div className="font-mono text-2xl font-semibold tabular-nums">{value}</div>
      <div className="text-sm">{label}</div>
      {sub && <div className="mt-0.5 text-xs text-muted-foreground">{sub}</div>}
    </a>
  );
}

export function OverviewView() {
  const { data, screensOf } = useBrain();
  const s = data.stats;
  const screens = data.screens.filter(x => x.kind === 'screen');
  const reached = data.routes.filter(r => screensOf.has(r.id)).length;
  const owed = data.localKeys.filter(k => k.owner === 'obid');
  const checks = data.checks.filter(c => c.kind !== 'local-owed');
  const byKind = new Map<string, number>();
  for (const c of checks) byKind.set(c.kind, (byKind.get(c.kind) ?? 0) + 1);
  return (
    <>
      <PageHeader eyebrow="Oby" title="Brain" description="A map of Oby read from its code: Sangam's screens and what each is really connected to, obid's routes, areas and background work, and the checks that keep them honest. Local only, regenerated on every save." />
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Stat to={href('screens')} value={screens.length} label="Screens" sub={`${data.api.length} obid calls in api`} />
        <Stat to={href('endpoints')} value={s.routes} label="obid routes" sub={`${reached} reached by a screen`} />
        <Stat to={href('kept')} value={owed.length} label="Owed by obid" sub="choices the app keeps itself" />
        <Stat to={href('checks')} value={checks.length} label="Checks" sub="where code and docs disagree" />
        <Stat to={href('areas')} value={data.areas.length} label="Areas" sub={`${(s.obidLines / 1000).toFixed(1)}k lines of obid`} />
        <Stat to={href('modules')} value={s.modules} label="Modules" sub={`${s.cycles} pairs import each other`} />
        <Stat to={href('background')} value={s.spawns} label="Background tasks" />
        <Stat to={href('rules')} value={data.rules.length} label="Rules" sub={`${s.tests} tests in obid`} />
      </div>
      <div className="mt-6 grid gap-4 xl:grid-cols-2">
        <Section title="For Sangam" description="What his screens show, and what still waits on obid.">
          <ul className="space-y-2 text-sm">
            {owed.map(k => (
              <li key={k.key} className="flex flex-wrap items-center gap-2">
                <OriginBadge origin="local" />
                <span className="font-mono text-[12.5px]">{k.key}</span>
                {k.issue && <IssueLink n={k.issue} />}
                <span className="text-xs text-muted-foreground">{k.note}</span>
              </li>
            ))}
          </ul>
          <p className="mt-3 text-xs text-muted-foreground">Open any state on the mock from <Link to={href('states')}>States to open</Link>; every sentence of the app is in <Link to={href('words')}>Words</Link>.</p>
        </Section>
        <Section title="Checks" count={checks.length} description="Open one kind to see each case and its line.">
          <ul className="space-y-1.5 text-sm">
            {[...byKind].sort((a, b) => b[1] - a[1]).map(([k, n]) => (
              <li key={k} className="flex items-center justify-between gap-3">
                <Link to={`${href('checks')}?kind=${k}`}>{CHECK_LABEL[k as keyof typeof CHECK_LABEL]}</Link>
                <span className="font-mono text-xs text-muted-foreground">{n}</span>
              </li>
            ))}
          </ul>
        </Section>
        <Section title="Biggest screens">
          <ul className="space-y-1.5">{[...screens].sort((a, b) => b.lines - a.lines).slice(0, 8).map(x => <li key={x.file} className="flex items-center justify-between"><ScreenLink file={x.file} /><span className="font-mono text-xs text-muted-foreground">{x.lines}</span></li>)}</ul>
        </Section>
        <Section title="Biggest modules of obid">
          <ul className="space-y-1.5">{data.modules.slice(0, 8).map(m => <li key={m.id} className="flex items-center justify-between"><ModuleLink id={m.id} /><span className="font-mono text-xs text-muted-foreground">{m.lines.toLocaleString()}</span></li>)}</ul>
        </Section>
      </div>
    </>
  );
}
