import { useState } from 'react';
import { Empty, Facts, PageHeader, Section } from '@/components/brain/page';
import { AccessBadge, MethodBadge } from '@/components/brain/badges';
import { ModuleLink, RouteLink, ScreenLink } from '@/components/brain/links';
import { SourceLink } from '@/components/brain/source';
import { Input } from '@/components/ui/input';
import { useBrain, type Route } from '@/lib/brain';
import { href, Link, setQuery, type Route as HashRoute } from '@/lib/router';
import { cn } from '@/lib/utils';

const pill = (on: boolean) => cn('rounded-full border px-2.5 py-0.5 text-xs', on ? 'border-foreground/30 bg-foreground/10 text-foreground' : 'text-muted-foreground hover:text-foreground');

/** obid's HTTP routes: who may call them, what they refuse, and which screens reach them. */
export function EndpointsView({ route }: { route: HashRoute }) {
  const brain = useBrain();
  const id = route.path.slice(1).join('/');
  if (id) return <EndpointDetail id={id} />;
  const q = route.query.get('q') ?? '';
  const show = route.query.get('show') ?? 'all';
  const filters: Record<string, (r: Route) => boolean> = {
    all: () => true,
    called: r => brain.screensOf.has(r.id) || r.callers.length > 0,
    unused: r => !brain.screensOf.has(r.id) && r.callers.length === 0,
    phone: r => r.phone,
    undocumented: r => !r.documented,
  };
  const shown = brain.data.routes.filter(r => filters[show](r) && (!q || r.id.toLowerCase().includes(q.toLowerCase()) || r.handler.toLowerCase().includes(q.toLowerCase())));
  const groups = new Map<string, Route[]>();
  for (const r of shown) { const g = r.path.split('/').slice(0, 3).join('/'); if (!groups.has(g)) groups.set(g, []); groups.get(g)!.push(r); }
  return (
    <>
      <PageHeader eyebrow="obid" title="Endpoints" description="Every route obid registers (axum, read by obid-extract), the gate that lets each one in, and the app's screens that call it.">
        <Input value={q} onChange={e => setQuery(route, { q: e.target.value })} placeholder="Filter by path or handler" className="h-8 w-64 text-sm" />
      </PageHeader>
      <div className="mb-4 flex flex-wrap gap-2">
        {Object.keys(filters).map(k => (
          <button key={k} type="button" className={pill(show === k)} onClick={() => setQuery(route, { show: k === 'all' ? null : k })}>
            {{ all: 'All', called: 'Called by the app', unused: 'No screen calls it', phone: 'Phone may call', undocumented: 'Not in runner-api.md' }[k]}
            <span className="ml-1.5 font-mono opacity-60">{brain.data.routes.filter(filters[k]).length}</span>
          </button>
        ))}
      </div>
      <div className="space-y-4">
        {[...groups].map(([g, rs]) => (
          <Section key={g} title={<span className="font-mono">{g}</span>} count={rs.length} contentClassName="px-0">
            <div className="divide-y">
              {rs.map(r => {
                const screens = brain.screensOf.get(r.id) ?? [];
                return (
                  <div key={r.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-6 py-2">
                    <RouteLink id={r.id} className="min-w-72 flex-1" />
                    <div className="flex gap-1">{r.access.map(a => <AccessBadge key={a} kind={a} />)}</div>
                    {!r.documented && <span className="rounded-full border border-public/25 bg-public/12 px-2 text-[11px] text-public">undocumented</span>}
                    <span className="w-40 truncate text-right text-xs text-muted-foreground">
                      {screens.length ? `${screens.length} screen${screens.length > 1 ? 's' : ''}` : r.callers.length ? r.callers.map(c => c.via).join(', ') : 'no caller'}
                    </span>
                  </div>
                );
              })}
            </div>
          </Section>
        ))}
        {!shown.length && <Empty>No route matches.</Empty>}
      </div>
    </>
  );
}

function EndpointDetail({ id }: { id: string }) {
  const brain = useBrain();
  const r = brain.route.get(id);
  if (!r) return <Empty>obid has no route {id}.</Empty>;
  const screens = brain.screensOf.get(r.id) ?? [];
  return (
    <>
      <PageHeader eyebrow={`obid · ${r.router} router`} title={<span className="flex items-center gap-3 font-mono text-xl"><MethodBadge method={r.method} />{r.path}</span>} description={r.handlerDoc ?? undefined}>
        <div className="flex gap-1">{r.access.map(a => <AccessBadge key={a} kind={a} />)}</div>
      </PageHeader>
      <div className="grid gap-4 xl:grid-cols-2">
        <Section title="Where it lives">
          <Facts items={[
            ['Registered', <SourceLink key="r" file={r.source.file} line={r.source.line} />],
            ['Handler', <span key="h" className="flex flex-wrap items-center gap-2"><span className="font-mono text-[12.5px]">{r.handler}</span>{r.handlerSource && <SourceLink file={r.handlerSource.file} line={r.handlerSource.line} />}</span>],
            ['Reads', r.extractors.length ? <span key="e" className="flex flex-wrap gap-1.5">{r.extractors.map(x => <code key={x} className="rounded bg-muted px-1.5 font-mono text-[11.5px]">{x}</code>)}</span> : 'nothing'],
            ['runner-api.md', r.documented ? 'documented' : <span key="d" className="text-public">not documented</span>],
            ['Phone', r.phone ? 'a paired phone may call it through the gateway' : 'Mac only'],
          ]} />
        </Section>
        <Section title="Screens that reach it" count={screens.length} description="Sangam's screens whose data or actions end in this route.">
          {screens.length === 0 && r.callers.length === 0 ? <Empty>No screen calls it.</Empty> : (
            <ul className="space-y-1.5 text-sm">
              {screens.map(s => <li key={s.screen.file} className="flex flex-wrap items-center gap-2"><ScreenLink file={s.screen.file} /><span className="text-xs text-muted-foreground">api.{s.api}{s.via ? ` via ${s.via}` : ''}</span></li>)}
              {r.callers.filter(c => !c.via.startsWith('api.')).map((c, i) => <li key={i} className="flex items-center gap-2 text-xs"><span className="font-mono">{c.via}</span><SourceLink file={c.source.file} line={c.source.line} /></li>)}
            </ul>
          )}
        </Section>
      </div>
      <Section title="What it refuses" count={r.errors.length} className="mt-4" description="Every ApiError the handler writes itself (errors from functions it calls aren't listed). These words reach the app verbatim.">
        {r.errors.length === 0 ? <Empty>None written in the handler.</Empty> : (
          <ul className="space-y-1.5 text-sm">
            {r.errors.map((e, i) => (
              <li key={i} className="flex flex-wrap items-baseline gap-2">
                <span className="w-44 font-mono text-xs text-delete">{e.status}</span>
                <span className="min-w-0 flex-1">{e.message}</span>
                <SourceLink file={r.handlerSource?.file ?? r.source.file} line={e.line} label={`${e.line}`} />
              </li>
            ))}
          </ul>
        )}
      </Section>
    </>
  );
}

/** obid's eleven areas and the modules in each. */
export function AreasView() {
  const { data } = useBrain();
  const max = Math.max(...data.areas.map(a => a.lines), 1);
  return (
    <>
      <PageHeader eyebrow="obid" title="Areas" description="obid's code by area, as the 2026-10-29 plan draws it. Dependencies should point toward the business rules; a domain area imports no HTTP, process, file or network crate." />
      <div className="grid gap-4 lg:grid-cols-2">
        {data.areas.map(a => {
          const mods = data.modules.filter(m => m.area === a.id);
          const out = new Set(mods.flatMap(m => m.uses).filter(u => !a.modules.includes(u)));
          return (
            <Section key={a.id} title={<span className="flex items-center gap-2">{a.title}{a.domain && <span className="rounded-full border px-2 text-[10px] font-normal text-muted-foreground">domain</span>}</span>} description={a.summary} count={mods.length}>
              <div className="mb-3 h-1.5 overflow-hidden rounded-full bg-muted"><div className="h-full rounded-full bg-primary/70" style={{ width: `${(a.lines / max) * 100}%` }} /></div>
              <div className="mb-3 font-mono text-xs text-muted-foreground">{a.lines.toLocaleString()} lines</div>
              <div className="flex flex-wrap gap-x-4 gap-y-1">{mods.map(m => <span key={m.id} className="flex items-center gap-1.5"><ModuleLink id={m.id} /><span className="font-mono text-[11px] text-muted-foreground">{m.lines.toLocaleString()}</span></span>)}</div>
              {out.size > 0 && <p className="mt-3 text-xs text-muted-foreground">Reaches into: {[...out].sort().join(', ')}</p>}
              {a.modules.length === 0 && <p className="mt-2 text-xs text-muted-foreground">Lives inside another module today (brain::plan); it gets its own module when it's split out.</p>}
            </Section>
          );
        })}
      </div>
    </>
  );
}

/** obid's top-level modules: size, area, who they use and who uses them, and the pairs that import each other. */
export function ModulesView({ route }: { route: HashRoute }) {
  const { data, module } = useBrain();
  const id = route.path[1];
  const [q, setQ] = useState('');
  if (id) {
    const m = module.get(id);
    if (!m) return <Empty>No module {id}.</Empty>;
    const cyc = data.cycles.filter(c => c.includes(id)).map(c => c.find(x => x !== id)!);
    const files = data.spawns.filter(s => s.module === id);
    return (
      <>
        <PageHeader eyebrow={m.area ? data.areas.find(a => a.id === m.area)?.title : 'No area'} title={<span className="font-mono">{m.id}</span>} description={`${m.lines.toLocaleString()} lines`}>
          <SourceLink file={m.file} line={1} />
        </PageHeader>
        <div className="grid gap-4 xl:grid-cols-2">
          <Section title="Uses" count={m.uses.length}><div className="flex flex-wrap gap-x-4 gap-y-1">{m.uses.map(u => <ModuleLink key={u} id={u} />)}</div></Section>
          <Section title="Used by" count={m.usedBy.length}><div className="flex flex-wrap gap-x-4 gap-y-1">{m.usedBy.map(u => <ModuleLink key={u} id={u} />)}</div></Section>
          <Section title="Imports each other with" count={cyc.length} description="Modules this one uses that use it back.">{cyc.length ? <div className="flex flex-wrap gap-x-4 gap-y-1">{cyc.map(u => <ModuleLink key={u} id={u} />)}</div> : <Empty>None.</Empty>}</Section>
          <Section title="Outside crates" count={m.external.length}><div className="flex flex-wrap gap-1.5">{m.external.map(c => <code key={c} className="rounded bg-muted px-1.5 font-mono text-[11.5px]">{c}</code>)}</div></Section>
        </div>
        <Section title="Background work started here" count={files.length} className="mt-4">
          {files.length === 0 ? <Empty>None.</Empty> : <ul className="space-y-1 text-sm">{files.map((s, i) => <li key={i} className="flex flex-wrap items-center gap-2"><span className="w-32 font-mono text-xs text-muted-foreground">{s.kind}</span><span className="font-mono text-[12.5px]">{s.inFn ?? '(top level)'}</span><SourceLink file={s.source.file} line={s.source.line} /></li>)}</ul>}
        </Section>
      </>
    );
  }
  const shown = data.modules.filter(m => !q || m.id.includes(q));
  const max = Math.max(...data.modules.map(m => m.lines), 1);
  return (
    <>
      <PageHeader eyebrow="obid" title="Modules" description={`${data.modules.length} top-level modules, ${data.cycles.length} pairs that import each other.`}>
        <Input value={q} onChange={e => setQ(e.target.value)} placeholder="Filter" className="h-8 w-48 text-sm" />
      </PageHeader>
      <Section title="By size" count={shown.length} contentClassName="px-0">
        <div className="divide-y">
          {shown.map(m => (
            <div key={m.id} className="flex flex-wrap items-center gap-x-4 gap-y-1 px-6 py-2">
              <ModuleLink id={m.id} className="w-36" />
              <span className="w-32 text-xs text-muted-foreground">{data.areas.find(a => a.id === m.area)?.title ?? 'no area'}</span>
              <div className="h-1.5 min-w-24 flex-1 overflow-hidden rounded-full bg-muted"><div className="h-full rounded-full bg-primary/60" style={{ width: `${(m.lines / max) * 100}%` }} /></div>
              <span className="w-16 text-right font-mono text-[11px] text-muted-foreground">{m.lines.toLocaleString()}</span>
              <span className="w-32 text-right text-xs text-muted-foreground">uses {m.uses.length} · used by {m.usedBy.length}</span>
            </div>
          ))}
        </div>
      </Section>
    </>
  );
}

/** Every spawned task, thread and interval: what runs in the background. */
export function BackgroundView() {
  const { data } = useBrain();
  const byModule = new Map<string, typeof data.spawns>();
  for (const s of data.spawns) { if (!byModule.has(s.module)) byModule.set(s.module, []); byModule.get(s.module)!.push(s); }
  return (
    <>
      <PageHeader eyebrow="obid" title="Background work" description="Every tokio::spawn, spawn_blocking, thread and interval in obid, and the function that starts it." />
      <div className="space-y-4">
        {[...byModule].sort((a, b) => b[1].length - a[1].length).map(([m, ss]) => (
          <Section key={m} title={<ModuleLink id={m} />} count={ss.length} contentClassName="px-0">
            <div className="divide-y">
              {ss.map((s, i) => (
                <div key={i} className="flex flex-wrap items-baseline gap-x-3 px-6 py-1.5 text-sm">
                  <span className="w-28 font-mono text-xs text-muted-foreground">{s.kind}</span>
                  <span className="w-64 truncate font-mono text-[12.5px]">{s.inFn ?? '(top level)'}</span>
                  <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{s.doc ?? ''}</span>
                  <SourceLink file={s.source.file} line={s.source.line} />
                </div>
              ))}
            </div>
          </Section>
        ))}
      </div>
    </>
  );
}

/** obid has no database: the files it keeps under its home. */
export function StoredView() {
  const { data } = useBrain();
  const [q, setQ] = useState('');
  const shown = data.storedFiles.filter(f => !q || f.name.includes(q));
  return (
    <>
      <PageHeader eyebrow="obid" title="Stored files" description="obid keeps its state in JSON files under its home, not a database. Each file name written in the code, and where.">
        <Input value={q} onChange={e => setQ(e.target.value)} placeholder="Filter" className="h-8 w-48 text-sm" />
      </PageHeader>
      <Section title="Files" count={shown.length} contentClassName="px-0">
        <div className="divide-y">
          {shown.map(f => (
            <div key={f.name} className="flex flex-wrap items-baseline gap-x-4 gap-y-1 px-6 py-2">
              <span className="w-64 font-mono text-[12.5px]">{f.name}</span>
              <span className="flex min-w-0 flex-1 flex-wrap gap-x-3 gap-y-0.5">{f.sites.slice(0, 6).map((s, i) => <SourceLink key={i} file={s.file} line={s.line} label={`${s.inFn ?? s.file.split('/').pop()}`} />)}{f.sites.length > 6 && <span className="text-xs text-muted-foreground">+{f.sites.length - 6}</span>}</span>
            </div>
          ))}
        </div>
      </Section>
    </>
  );
}

/** The app's hand-written wire types beside obid's serde structs. */
export function WireView() {
  const { data } = useBrain();
  return (
    <>
      <PageHeader eyebrow="obid and the app" title="Wire types" description="Each type in apps/electron/src/shared/obid-wire.ts beside the Rust struct obid serialises. A key only the app expects is often one obid adds by hand with json!; a key only Rust has is one the app doesn't read yet." />
      <div className="grid gap-4 xl:grid-cols-2">
        {data.wire.map(w => (
          <Section key={w.name} title={<span className="font-mono">{w.name}</span>} action={<SourceLink file={w.source.file} line={w.source.line} />} description={w.rust ? <>Rust: <span className="font-mono">{w.rust.name}</span> <SourceLink file={w.rust.source.file} line={w.rust.source.line} /></> : 'No Rust struct of that name.'}>
            <div className="flex flex-wrap gap-1.5">
              {w.keys.map(k => (
                <code key={k.key} className={cn('rounded px-1.5 font-mono text-[11.5px]', w.onlyTs.includes(k.key) ? 'bg-public/12 text-public' : 'bg-muted')}>{k.key}{k.optional ? '?' : ''}</code>
              ))}
              {w.onlyRust.map(k => <code key={k} className="rounded border border-dashed px-1.5 font-mono text-[11.5px] text-muted-foreground">{k}</code>)}
            </div>
            {(w.onlyTs.length > 0 || w.onlyRust.length > 0) && (
              <p className="mt-2 text-xs text-muted-foreground">
                {w.onlyTs.length > 0 && <>Only in the app: {w.onlyTs.join(', ')}. </>}
                {w.onlyRust.length > 0 && <>Only in Rust (dashed): {w.onlyRust.join(', ')}.</>}
              </p>
            )}
          </Section>
        ))}
      </div>
    </>
  );
}

/** The phone allowlist, entry by entry. */
export function PhoneView() {
  const { data } = useBrain();
  return (
    <>
      <PageHeader eyebrow="obid" title="Phone allowlist" description="gateway::ROUTES: what a paired phone may call through the gateway. An entry that matches no route lets nothing through today, but it's stale." />
      <Section title="Entries" count={data.phoneRoutes.length} contentClassName="px-0">
        <div className="divide-y">
          {data.phoneRoutes.map((p, i) => (
            <div key={i} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-6 py-1.5">
              <MethodBadge method={p.method} />
              <span className="w-72 font-mono text-[12.5px]">{p.pattern}</span>
              <span className="flex min-w-0 flex-1 flex-wrap gap-x-3">
                {p.matches.length ? p.matches.map(m => <Link key={m} to={href('endpoints', m)} className="font-mono text-xs text-muted-foreground">{m}</Link>) : <span className="text-xs text-delete">matches no route</span>}
              </span>
              <SourceLink file="core/crates/obid/src/gateway.rs" line={p.line} label={`${p.line}`} />
            </div>
          ))}
        </div>
      </Section>
    </>
  );
}
