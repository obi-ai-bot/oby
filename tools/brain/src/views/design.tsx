import { useMemo, useState } from 'react';
import { ExternalLink } from 'lucide-react';
import { Empty, Facts, PageHeader, Section } from '@/components/brain/page';
import { OriginBadge, OwnerBadge } from '@/components/brain/badges';
import { IssueLink, RouteLink, ScreenLink } from '@/components/brain/links';
import { SourceLink } from '@/components/brain/source';
import { Input } from '@/components/ui/input';
import { shortFile, useBrain, type Screen } from '@/lib/brain';
import { href, Link, type Route as HashRoute } from '@/lib/router';
import { cn } from '@/lib/utils';

const KIND_LABEL: Record<Screen['kind'], string> = { screen: 'Screens', store: 'Stores and helpers', main: 'Electron main and preload' };

function Filter({ value, onChange, placeholder }: { value: string; onChange: (v: string) => void; placeholder: string }) {
  return <Input value={value} onChange={e => onChange(e.target.value)} placeholder={placeholder} className="h-8 max-w-xs text-sm" />;
}

/** Each of Sangam's screens: how much of it is live, mock, kept locally or hidden. */
export function ScreensView({ route }: { route: HashRoute }) {
  const { data } = useBrain();
  const file = route.path.slice(1).join('/');
  const [q, setQ] = useState('');
  if (file) return <ScreenDetail file={file} />;
  const groups = (['screen', 'store', 'main'] as const).map(kind => ({
    kind, items: data.screens.filter(s => s.kind === kind && (!q || s.file.toLowerCase().includes(q.toLowerCase()))),
  }));
  return (
    <>
      <PageHeader eyebrow="Sangam's design" title="Screens" description="Every file of the Electron app, and where each piece of it comes from: obid (live), the demo data (mock), the app itself (kept locally), or hidden in v1 behind ?everything.">
        <Filter value={q} onChange={setQ} placeholder="Filter by file" />
      </PageHeader>
      <div className="space-y-6">
        {groups.filter(g => g.items.length).map(g => (
          <Section key={g.kind} title={KIND_LABEL[g.kind]} count={g.items.length} contentClassName="px-0">
            <div className="divide-y">
              {g.items.map(s => {
                const routes = new Set(s.live.map(l => l.route).filter(Boolean));
                return (
                  <div key={s.file} className="flex flex-wrap items-center gap-x-4 gap-y-1 px-6 py-2.5">
                    <ScreenLink file={s.file} className="w-48" />
                    <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{s.doc ?? ''}</span>
                    <div className="flex flex-wrap items-center gap-1.5">
                      {routes.size > 0 && <span className="text-xs"><OriginBadge origin="live" /> <span className="font-mono text-muted-foreground">{routes.size}</span></span>}
                      {s.mock.length > 0 && <span className="text-xs"><OriginBadge origin="mock" /> <span className="font-mono text-muted-foreground">{s.mock.length}</span></span>}
                      {s.local.length > 0 && <span className="text-xs"><OriginBadge origin="local" /> <span className="font-mono text-muted-foreground">{s.local.length}</span></span>}
                      {s.everything.length > 0 && <span className="text-xs"><OriginBadge origin="hidden" /> <span className="font-mono text-muted-foreground">{s.everything.length}</span></span>}
                    </div>
                    <span className="w-14 text-right font-mono text-[11px] text-muted-foreground">{s.lines} l</span>
                  </div>
                );
              })}
            </div>
          </Section>
        ))}
      </div>
    </>
  );
}

function ScreenDetail({ file }: { file: string }) {
  const { screen, data } = useBrain();
  const s = screen.get(file);
  if (!s) return <Empty>No such file in the app.</Empty>;
  const keys = data.localKeys.filter(k => k.sites.some(x => x.file === file));
  const byRoute = new Map<string, typeof s.live>();
  for (const l of s.live) { const k = l.route ?? `api.${l.api} (no route)`; if (!byRoute.has(k)) byRoute.set(k, []); byRoute.get(k)!.push(l); }
  return (
    <>
      <PageHeader eyebrow={KIND_LABEL[s.kind]} title={s.name} description={s.doc ?? undefined}>
        <SourceLink file={s.file} line={1} />
      </PageHeader>
      <div className="grid gap-4 xl:grid-cols-2">
        <Section title={<span className="flex items-center gap-2">From obid <OriginBadge origin="live" /></span>} count={byRoute.size} description="The routes its data and actions reach, directly through api or through the stores it imports.">
          {byRoute.size === 0 ? <Empty>Nothing from obid.</Empty> : (
            <ul className="space-y-2">
              {[...byRoute].map(([k, ls]) => (
                <li key={k} className="space-y-0.5">
                  {ls[0].route ? <RouteLink id={ls[0].route} /> : <span className="font-mono text-xs text-delete">{k}</span>}
                  <div className="pl-16 text-xs text-muted-foreground">
                    {ls.map((l, i) => <span key={i} className="mr-3">api.{l.api}{l.via ? <> via <span className="font-mono">{l.via}</span></> : ''} · <SourceLink file={s.file} line={l.line} label={`line ${l.line}`} /></span>)}
                  </div>
                </li>
              ))}
            </ul>
          )}
        </Section>
        <Section title={<span className="flex items-center gap-2">Kept locally <OriginBadge origin="local" /></span>} count={keys.length} description="Choices the app saves itself. An owed one waits for obid to take it over.">
          {keys.length === 0 ? <Empty>None.</Empty> : (
            <ul className="space-y-2">
              {keys.map(k => (
                <li key={`${k.kind}:${k.key}`} className="flex flex-wrap items-center gap-2 text-sm">
                  <span className="font-mono text-[12.5px]">{k.key}</span>
                  <span className="text-xs text-muted-foreground">{k.kind}</span>
                  <OwnerBadge k={k} />
                  {k.issue && <IssueLink n={k.issue} />}
                  {k.note && <span className="text-xs text-muted-foreground">{k.note}</span>}
                </li>
              ))}
            </ul>
          )}
        </Section>
        <Section title={<span className="flex items-center gap-2">Demo data <OriginBadge origin="mock" /></span>} count={s.mock.length} description="Values imported from mock.ts.">
          {s.mock.length === 0 ? <Empty>None.</Empty> : (
            <div className="flex flex-wrap gap-2">{s.mock.map(m => <span key={m.name} className="rounded-md border px-2 py-0.5 font-mono text-xs">{m.name}</span>)}</div>
          )}
        </Section>
        <Section title="Renders" count={s.renders.length} description="Components from other files it shows. What they reach from obid is on their own pages.">
          {s.renders.length === 0 ? <Empty>None.</Empty> : <div className="flex flex-wrap gap-x-4 gap-y-1">{s.renders.map(r => <ScreenLink key={r} file={r} />)}</div>}
        </Section>
        <Section title={<span className="flex items-center gap-2">Gates <OriginBadge origin="hidden" /></span>} description="Lines that read ?everything (hidden in v1) or onObid (live or mock).">
          <Facts items={[
            ['?everything', s.everything.length ? s.everything.map(l => <SourceLink key={l} file={s.file} line={l} label={`${l}`} className="mr-2" />) : <span className="text-muted-foreground">none</span>],
            ['onObid', s.onObid.length ? s.onObid.map(l => <SourceLink key={l} file={s.file} line={l} label={`${l}`} className="mr-2" />) : <span className="text-muted-foreground">none</span>],
          ]} />
        </Section>
      </div>
      <Section title="Words on it" count={s.words.length} className="mt-4" description="The text a person reads, in the order it's written.">
        {s.words.length === 0 ? <Empty>No words.</Empty> : (
          <ul className="columns-1 gap-6 text-sm md:columns-2">
            {s.words.map(w => <li key={w.text} className="mb-1.5 break-inside-avoid"><span>{w.text}</span> <SourceLink file={s.file} line={w.line} label={`${w.line}`} /></li>)}
          </ul>
        )}
      </Section>
    </>
  );
}

/** Every choice the app keeps itself, and which of them obid owes. */
export function KeptView() {
  const { data } = useBrain();
  const owed = data.localKeys.filter(k => k.owner === 'obid');
  const rest = data.localKeys.filter(k => k.owner !== 'obid');
  const row = (k: (typeof data.localKeys)[number]) => (
    <div key={`${k.kind}:${k.key}`} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-6 py-2.5">
      <span className="w-56 truncate font-mono text-[12.5px]">{k.key}</span>
      <span className="w-28 text-xs text-muted-foreground">{k.kind}</span>
      <OwnerBadge k={k} />
      {k.issue && <IssueLink n={k.issue} />}
      <span className="min-w-0 flex-1 text-xs text-muted-foreground">{k.note ?? ''}</span>
      <span className="flex flex-wrap gap-2">{k.sites.map((x, i) => <ScreenLink key={i} file={x.file} />)}</span>
    </div>
  );
  return (
    <>
      <PageHeader eyebrow="Sangam's design" title="Kept locally" description="Choices Sangam's screens save in the app because obid has no setting for them yet. When obid takes one over, its issue closes and it leaves this list." />
      <div className="space-y-6">
        <Section title="Owed by obid" count={owed.length} contentClassName="px-0"><div className="divide-y">{owed.map(row)}</div></Section>
        <Section title="The app's own, and unclassified" count={rest.length} description="Per-viewer conveniences that belong in the app. An unclassified one needs a line in brain.config.ts." contentClassName="px-0"><div className="divide-y">{rest.map(row)}</div></Section>
      </div>
    </>
  );
}

/** States of the app to open on the mock (npm run web, or oby-app-web in the launch configurations). */
export function StatesView() {
  const { data } = useBrain();
  const groups = [...new Set(data.states.map(s => s.group))];
  return (
    <>
      <PageHeader eyebrow="Sangam's design" title="States to open" description={<>Each link opens the app in that state on the mock. Run <code className="font-mono">npm run web</code> in apps/electron, or oby-app-web from the launch configurations, first.</>} />
      <div className="grid gap-4 lg:grid-cols-2">
        {groups.map(g => (
          <Section key={g} title={g} count={data.states.filter(s => s.group === g).length}>
            <ul className="space-y-1.5">
              {data.states.filter(s => s.group === g).map(s => (
                <li key={s.url} className="flex flex-wrap items-baseline gap-2 text-sm">
                  <a href={s.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 hover:underline">{s.label}<ExternalLink className="size-3 text-muted-foreground" /></a>
                  <span className="font-mono text-[11px] text-muted-foreground">{s.url.replace(/^https?:\/\/[^/]+\//, '/')}</span>
                  {s.note && <span className="text-xs text-muted-foreground">{s.note}</span>}
                </li>
              ))}
            </ul>
          </Section>
        ))}
      </div>
    </>
  );
}

/** Every sentence the app shows, searchable, by screen. */
export function WordsView() {
  const { data } = useBrain();
  const [q, setQ] = useState('');
  const all = useMemo(() => data.screens.flatMap(s => s.words.map(w => ({ ...w, file: s.file, name: s.name }))), [data]);
  const shown = all.filter(w => !q || w.text.toLowerCase().includes(q.toLowerCase()));
  return (
    <>
      <PageHeader eyebrow="Sangam's design" title="Words" description="What a person reads in the app, by screen. obid's own error texts that reach the screens verbatim are on each endpoint's page.">
        <Filter value={q} onChange={setQ} placeholder="Search the words" />
      </PageHeader>
      <Section title="Sentences" count={shown.length} contentClassName="px-0">
        <div className="divide-y">
          {shown.slice(0, 600).map((w, i) => (
            <div key={i} className="flex flex-wrap items-baseline gap-x-4 px-6 py-1.5 text-sm">
              <span className="min-w-0 flex-1">{w.text}</span>
              <Link to={href('screens', w.file)} className={cn('font-mono text-xs text-muted-foreground')}>{w.name}</Link>
              <SourceLink file={w.file} line={w.line} label={`${shortFile(w.file).split('/').pop()}:${w.line}`} />
            </div>
          ))}
        </div>
        {shown.length > 600 && <p className="px-6 pt-2 text-xs text-muted-foreground">Showing 600 of {shown.length}: search to narrow it.</p>}
      </Section>
    </>
  );
}
