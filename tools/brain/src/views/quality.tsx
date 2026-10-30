import { CheckCircle2, CircleAlert } from 'lucide-react';
import { Empty, PageHeader, Section } from '@/components/brain/page';
import { SourceLink } from '@/components/brain/source';
import { useBrain, type CheckKind } from '@/lib/brain';
import { setQuery, type Route as HashRoute } from '@/lib/router';
import { cn } from '@/lib/utils';

/** AGENTS.md's rules, each tied to the code that keeps it and the tests that prove it. */
export function RulesView() {
  const { data } = useBrain();
  return (
    <>
      <PageHeader eyebrow="AGENTS.md" title="Rules Oby never breaks" description="Each rule beside the code that keeps it and the tests that fail without it. A link the code no longer has, or a rule with no test, shows in Checks." />
      <div className="grid gap-4 xl:grid-cols-2">
        {data.rules.map(r => {
          const tests = r.links.filter(l => l.kind === 'test');
          return (
            <Section key={r.id} title={r.title} description={r.text} action={tests.length ? <CheckCircle2 className="size-4 text-post" /> : <CircleAlert className="size-4 text-public" />}>
              <div className="space-y-3 text-sm">
                {(['code', 'test'] as const).map(kind => {
                  const ls = r.links.filter(l => l.kind === kind);
                  return (
                    <div key={kind}>
                      <div className="mb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">{kind === 'code' ? 'Kept by' : 'Proved by'}</div>
                      {ls.length === 0 ? <Empty>{kind === 'test' ? 'No test yet.' : 'Nothing linked.'}</Empty> : (
                        <ul className="space-y-1">
                          {ls.map((l, i) => (
                            <li key={i} className="flex flex-wrap items-center gap-2">
                              <span className={cn('font-mono text-[12.5px]', !l.source && 'text-delete line-through')}>{l.symbol || l.file.split('/').pop()}</span>
                              {l.source ? <SourceLink file={l.source.file} line={l.source.line} /> : <span className="text-xs text-delete">not found</span>}
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  );
                })}
              </div>
            </Section>
          );
        })}
      </div>
    </>
  );
}

export const CHECK_LABEL: Record<CheckKind, string> = {
  'route-undocumented': 'Route not in runner-api.md',
  'doc-without-route': 'Documented, but obid has no such route',
  'phone-without-route': 'Phone allowlist entry with no route',
  'call-without-route': 'The app calls a route obid doesn\'t have',
  'route-unused': 'No screen calls it',
  'local-unclassified': 'Kept locally, not classified',
  'local-owed': 'Kept locally until obid takes it',
  layering: 'Areas and layering',
  'internal-host': 'Internal host in the open-source code',
  'rule-unlinked': 'A rule without its code or test',
  wire: 'Wire keys only the app declares',
  extract: 'The extractor couldn\'t read it',
};

const ORDER: CheckKind[] = ['internal-host', 'call-without-route', 'doc-without-route', 'phone-without-route', 'rule-unlinked', 'local-unclassified', 'layering', 'route-undocumented', 'wire', 'extract', 'local-owed'];

/** What the code and its docs disagree about. */
export function ChecksView({ route }: { route: HashRoute }) {
  const { data } = useBrain();
  const only = route.query.get('kind') as CheckKind | null;
  const kinds = ORDER.filter(k => data.checks.some(c => c.kind === k));
  return (
    <>
      <PageHeader eyebrow="Checks" title="What disagrees" description="Where the code, its docs, its config and Sangam's screens disagree. Brain's tests fail on the ones that must stay at zero." />
      <div className="mb-4 flex flex-wrap gap-2">
        <button type="button" onClick={() => setQuery(route, { kind: null })} className={cn('rounded-full border px-2.5 py-0.5 text-xs', !only ? 'border-foreground/30 bg-foreground/10' : 'text-muted-foreground')}>All <span className="font-mono opacity-60">{data.checks.length}</span></button>
        {kinds.map(k => (
          <button key={k} type="button" onClick={() => setQuery(route, { kind: k })} className={cn('rounded-full border px-2.5 py-0.5 text-xs', only === k ? 'border-foreground/30 bg-foreground/10' : 'text-muted-foreground')}>
            {CHECK_LABEL[k]} <span className="font-mono opacity-60">{data.checks.filter(c => c.kind === k).length}</span>
          </button>
        ))}
      </div>
      <div className="space-y-4">
        {kinds.filter(k => !only || k === only).map(k => {
          const cs = data.checks.filter(c => c.kind === k);
          return (
            <Section key={k} title={CHECK_LABEL[k]} count={cs.length} contentClassName="px-0">
              <div className="divide-y">
                {cs.map((c, i) => (
                  <div key={i} className="flex flex-wrap items-baseline gap-x-4 gap-y-1 px-6 py-1.5 text-sm">
                    <span className="min-w-0 flex-1">{c.message}</span>
                    {c.source && <SourceLink file={c.source.file} line={c.source.line} />}
                  </div>
                ))}
              </div>
            </Section>
          );
        })}
        {kinds.length === 0 && <Empty>Nothing disagrees.</Empty>}
      </div>
    </>
  );
}
