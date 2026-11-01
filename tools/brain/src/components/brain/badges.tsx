import type { AccessKind, LocalKey } from '@/lib/brain';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { Bot, KeyRound, Link2, Lock, Smartphone } from 'lucide-react';

const METHOD_CLASS: Record<string, string> = {
  GET: 'text-get bg-get/12 border-get/25',
  POST: 'text-post bg-post/12 border-post/25',
  PUT: 'text-put bg-put/12 border-put/25',
  PATCH: 'text-patch bg-patch/12 border-patch/25',
  DELETE: 'text-delete bg-delete/12 border-delete/25',
};

export function MethodBadge({ method, className }: { method: string; className?: string }) {
  return (
    <span className={cn('inline-flex h-5 w-14 shrink-0 items-center justify-center rounded-md border font-mono text-[10.5px] font-semibold tracking-wide', METHOD_CLASS[method] ?? 'text-muted-foreground bg-muted', className)}>
      {method}
    </span>
  );
}

export const ACCESS_META: Record<AccessKind, { label: string; icon: typeof Lock; className: string; explain: string }> = {
  owner: { label: 'Owner', icon: Lock, className: 'text-owner bg-owner/12 border-owner/25', explain: 'On 127.0.0.1 with obid\'s API token (the gate in api.rs): the Mac app, the CLI.' },
  agent: { label: 'Agent', icon: Bot, className: 'text-agent bg-agent/12 border-agent/25', explain: 'A job\'s or turn\'s own token, for the agent MCP and Tetrix routes only.' },
  phone: { label: 'Phone', icon: Smartphone, className: 'text-phone bg-phone/12 border-phone/25', explain: 'A paired phone may call it through the gateway (gateway::ROUTES).' },
  pairing: { label: 'Pairing', icon: Link2, className: 'text-pairing bg-pairing/12 border-pairing/25', explain: 'The gateway\'s own route: a phone redeeming its pairing credential.' },
  public: { label: 'Public', icon: KeyRound, className: 'text-public bg-public/12 border-public/25', explain: 'Anyone who reaches the port.' },
};

export function AccessBadge({ kind }: { kind: AccessKind }) {
  const meta = ACCESS_META[kind];
  const Icon = meta.icon;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className={cn('inline-flex h-5 shrink-0 items-center gap-1 rounded-full border px-2 text-[11px] font-medium', meta.className)}>
          <Icon className="size-3" />{meta.label}
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-xs">{meta.explain}</TooltipContent>
    </Tooltip>
  );
}

/** Where a screen's piece comes from: obid, the mock, or the app itself. */
export type Origin = 'live' | 'mock' | 'local' | 'hidden';
const ORIGIN: Record<Origin, { label: string; className: string; explain: string }> = {
  live: { label: 'live', className: 'text-post bg-post/12 border-post/25', explain: 'Comes from obid: the endpoint or event named next to it.' },
  mock: { label: 'mock', className: 'text-put bg-put/12 border-put/25', explain: 'Demo data from mock.ts: what the browser preview and ?everything show.' },
  local: { label: 'kept locally', className: 'text-delete bg-delete/12 border-delete/25', explain: 'The app keeps it itself (localStorage or memory); obid never sees it.' },
  hidden: { label: 'hidden in v1', className: 'text-muted-foreground bg-muted border-border', explain: 'Behind ?everything: v1 doesn\'t show it.' },
};

export function OriginBadge({ origin, className }: { origin: Origin; className?: string }) {
  const m = ORIGIN[origin];
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className={cn('inline-flex h-5 shrink-0 items-center rounded-full border px-2 text-[11px] font-medium', m.className, className)}>{m.label}</span>
      </TooltipTrigger>
      <TooltipContent className="max-w-xs">{m.explain}</TooltipContent>
    </Tooltip>
  );
}

export function OwnerBadge({ k }: { k: LocalKey }) {
  if (k.owner === 'obid') return <span className="inline-flex h-5 items-center rounded-full border border-delete/25 bg-delete/12 px-2 text-[11px] font-medium text-delete">owed by obid</span>;
  if (k.owner === 'app') return <span className="inline-flex h-5 items-center rounded-full border px-2 text-[11px] text-muted-foreground">the app's</span>;
  return <span className="inline-flex h-5 items-center rounded-full border border-public/25 bg-public/12 px-2 text-[11px] font-medium text-public">unclassified</span>;
}
