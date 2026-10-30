import { LayoutTemplate, Package } from 'lucide-react';
import { href, Link } from '@/lib/router';
import { shortFile, useBrain } from '@/lib/brain';
import { MethodBadge } from './badges';
import { cn } from '@/lib/utils';

export function RouteLink({ id, className }: { id: string; className?: string }) {
  const { route } = useBrain();
  const r = route.get(id);
  const [method, ...rest] = id.split(' ');
  return (
    <Link to={href('endpoints', id)} className={cn('inline-flex min-w-0 items-center gap-2 font-mono text-[12.5px]', className)}>
      <MethodBadge method={r?.method ?? method} />
      <span className="truncate">{r?.path ?? rest.join(' ')}</span>
    </Link>
  );
}

export function ScreenLink({ file, className }: { file: string; className?: string }) {
  const { screen } = useBrain();
  const s = screen.get(file);
  return (
    <Link to={href('screens', file)} className={cn('inline-flex min-w-0 items-center gap-1.5 font-mono text-[12.5px]', className)}>
      <LayoutTemplate className="size-3.5 shrink-0 text-muted-foreground" />
      <span className="truncate">{s?.name ?? shortFile(file)}</span>
    </Link>
  );
}

export function ModuleLink({ id, className }: { id: string; className?: string }) {
  return (
    <Link to={href('modules', id)} className={cn('inline-flex items-center gap-1.5 font-mono text-[12.5px]', className)}>
      <Package className="size-3.5 shrink-0 text-muted-foreground" />{id}
    </Link>
  );
}

export function IssueLink({ n }: { n: number }) {
  return <a href={`https://github.com/obi-ai-bot/oby/issues/${n}`} target="_blank" rel="noreferrer" className="font-mono text-xs text-primary hover:underline">#{n}</a>;
}
