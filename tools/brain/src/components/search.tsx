import { useEffect, useState } from 'react';
import { Database, LayoutTemplate, Package, Search, Type } from 'lucide-react';
import { Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from '@/components/ui/command';
import { Button } from '@/components/ui/button';
import { shortFile, useBrain } from '@/lib/brain';
import { href, navigate } from '@/lib/router';
import { MethodBadge } from './brain/badges';

const escape = (t: string) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Every typed word must appear in the item; items where a word starts a name or a path segment rank first. */
function filter(value: string, search: string): number {
  const v = value.toLowerCase();
  const terms = search.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return 1;
  if (!terms.every(t => v.includes(t))) return 0;
  return terms.every(t => new RegExp(`(^|[\\s/#._:-])${escape(t)}`).test(v)) ? 1 : 0.4;
}

/** ⌘K: one search over routes, screens, kept-locally keys, modules and the app's words. */
export function SearchCommand() {
  const { data } = useBrain();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.key === 'k' && (e.metaKey || e.ctrlKey)) || (e.key === '/' && !(e.target instanceof HTMLInputElement) && !(e.target instanceof HTMLTextAreaElement))) {
        e.preventDefault();
        setOpen(o => !o);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const go = (to: string) => { setOpen(false); setQuery(''); navigate(to); };
  const typed = query.trim().length >= 2;

  return (
    <>
      <Button variant="outline" size="sm" className="h-8 w-full max-w-sm justify-start gap-2 text-muted-foreground" onClick={() => setOpen(true)}>
        <Search className="size-3.5" />
        <span className="flex-1 text-left text-xs">Search routes, screens, modules, words…</span>
        <kbd className="rounded border bg-muted px-1.5 font-mono text-[10px]">⌘K</kbd>
      </Button>
      <CommandDialog open={open} onOpenChange={setOpen} title="Search the Brain" description="Routes, screens, kept-locally choices, modules and words" className="sm:max-w-2xl">
        <Command filter={filter}>
          <CommandInput placeholder="Try: jobs, Composer, agent.default, permissions, Bypass…" value={query} onValueChange={setQuery} />
          <CommandList className="max-h-[60vh]">
            <CommandEmpty>Nothing found.</CommandEmpty>
            <CommandGroup heading="Screens">
              {data.screens.map(s => (
                <CommandItem key={s.file} value={`${s.name} ${s.file} screen`} onSelect={() => go(href('screens', s.file))}>
                  <LayoutTemplate /><span className="font-mono text-xs">{s.name}</span>
                  <span className="ml-auto truncate text-[11px] text-muted-foreground">{shortFile(s.file)}</span>
                </CommandItem>
              ))}
            </CommandGroup>
            <CommandGroup heading="Routes">
              {data.routes.map(r => (
                <CommandItem key={r.id} value={`${r.id} ${r.handler}`} onSelect={() => go(href('endpoints', r.id))}>
                  <MethodBadge method={r.method} />
                  <span className="flex-1 truncate font-mono text-xs">{r.path}</span>
                  <span className="truncate text-[11px] text-muted-foreground">{r.handler}</span>
                </CommandItem>
              ))}
            </CommandGroup>
            <CommandGroup heading="Kept locally">
              {data.localKeys.map(k => (
                <CommandItem key={`${k.kind}:${k.key}`} value={`${k.key} kept locally ${k.note ?? ''}`} onSelect={() => go(href('kept'))}>
                  <Database /><span className="font-mono text-xs">{k.key}</span>
                  <span className="ml-auto text-[11px] text-muted-foreground">{k.owner === 'obid' ? `owed by obid${k.issue ? ` · #${k.issue}` : ''}` : k.kind}</span>
                </CommandItem>
              ))}
            </CommandGroup>
            <CommandGroup heading="Modules">
              {data.modules.map(m => (
                <CommandItem key={m.id} value={`${m.id} module`} onSelect={() => go(href('modules', m.id))}>
                  <Package /><span className="font-mono text-xs">{m.id}</span>
                  <span className="ml-auto text-[11px] text-muted-foreground">{m.lines.toLocaleString()} lines</span>
                </CommandItem>
              ))}
            </CommandGroup>
            {typed && (
              <CommandGroup heading="Words">
                {data.screens.flatMap(s => s.words.map(w => (
                  <CommandItem key={`${s.file}:${w.line}:${w.text}`} value={`${w.text} ${s.name}`} onSelect={() => go(href('screens', s.file))}>
                    <Type /><span className="truncate text-xs">{w.text}</span>
                    <span className="ml-auto text-[11px] text-muted-foreground">{s.name}</span>
                  </CommandItem>
                )))}
              </CommandGroup>
            )}
          </CommandList>
        </Command>
      </CommandDialog>
    </>
  );
}
