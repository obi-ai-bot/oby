import {
  AlertTriangle, Boxes, Database, ExternalLink, FileJson, LayoutDashboard, LayoutTemplate, ListChecks, Moon, Network, PlayCircle, RefreshCw,
  Route as RouteIcon, ScrollText, ShieldCheck, Smartphone, Sun, Timer, Type, Waypoints,
} from 'lucide-react';
import {
  Sidebar, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupContent, SidebarGroupLabel, SidebarHeader, SidebarMenu,
  SidebarMenuBadge, SidebarMenuButton, SidebarMenuItem, SidebarRail,
} from '@/components/ui/sidebar';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useBrain, useBrainState } from '@/lib/brain';
import { EDITORS, usePrefs, type Editor } from '@/lib/prefs';
import { href, type Route } from '@/lib/router';
import { cn } from '@/lib/utils';

function ago(iso: string): string {
  const s = Math.max(0, Math.round((Date.now() - new Date(iso).getTime()) / 1000));
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  return new Date(iso).toLocaleString('en', { dateStyle: 'short', timeStyle: 'short' });
}

export function AppSidebar({ route }: { route: Route }) {
  const { data } = useBrain();
  const { loading, regenerate } = useBrainState();
  const { editor, setEditor, theme, setTheme } = usePrefs();
  const section = route.path[0] ?? '';
  const checks = data.checks.filter(c => c.kind !== 'local-owed').length;

  const item = (s: string, label: string, Icon: typeof RouteIcon, count?: number, warn = false) => (
    <SidebarMenuItem key={s}>
      <SidebarMenuButton asChild isActive={section === s} tooltip={label}>
        <a href={href(s)}><Icon /><span>{label}</span></a>
      </SidebarMenuButton>
      {count !== undefined && <SidebarMenuBadge className={cn('font-mono', warn && count > 0 && 'text-public')}>{count}</SidebarMenuBadge>}
    </SidebarMenuItem>
  );
  const group = (label: string, items: React.ReactNode) => (
    <SidebarGroup>
      <SidebarGroupLabel>{label}</SidebarGroupLabel>
      <SidebarGroupContent><SidebarMenu>{items}</SidebarMenu></SidebarGroupContent>
    </SidebarGroup>
  );

  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <a href="#/">
                <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-foreground text-background">
                  <Waypoints className="size-4" />
                </div>
                <div className="grid flex-1 text-left leading-tight">
                  <span className="truncate text-sm font-semibold">{data.name} Brain</span>
                  <span className="truncate font-mono text-[11px] text-muted-foreground">
                    {data.git ? `${data.git.branch} · ${data.git.commit}${data.git.dirty ? ' · changed' : ''}` : 'no git'}
                  </span>
                </div>
              </a>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              {item('', 'Overview', LayoutDashboard)}
              {item('checks', 'Checks', AlertTriangle, checks, true)}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
        {group('Sangam\'s design', <>
          {item('screens', 'Screens', LayoutTemplate, data.screens.filter(s => s.kind === 'screen').length)}
          {item('kept', 'Kept locally', Database, data.localKeys.filter(k => k.owner === 'obid').length)}
          {item('states', 'States to open', PlayCircle, data.states.length)}
          {item('words', 'Words', Type)}
        </>)}
        {group('obid', <>
          {item('endpoints', 'Endpoints', RouteIcon, data.routes.length)}
          {item('phone', 'Phone allowlist', Smartphone, data.phoneRoutes.length)}
          {item('areas', 'Areas', Boxes, data.areas.length)}
          {item('modules', 'Modules', Network, data.modules.length)}
          {item('background', 'Background work', Timer, data.spawns.length)}
          {item('stored', 'Stored files', ScrollText, data.storedFiles.length)}
          {item('wire', 'Wire types', FileJson, data.wire.length)}
        </>)}
        {group('AGENTS.md', <>
          {item('rules', 'Rules', ShieldCheck, data.rules.length)}
        </>)}
        <SidebarGroup className="mt-auto">
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton asChild tooltip="The roadmap board">
                  <a href="https://github.com/orgs/obi-ai-bot/projects/2" target="_blank" rel="noreferrer"><ListChecks /><span>Roadmap</span><ExternalLink className="ml-auto size-3 opacity-50" /></a>
                </SidebarMenuButton>
              </SidebarMenuItem>
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter className="gap-2 group-data-[collapsible=icon]:hidden">
        <div className="flex items-center gap-1.5">
          <Select value={editor} onValueChange={v => setEditor(v as Editor)}>
            <SelectTrigger size="sm" className="h-7 flex-1 text-xs"><SelectValue /></SelectTrigger>
            <SelectContent>
              {(Object.keys(EDITORS) as Editor[]).map(e => <SelectItem key={e} value={e} className="text-xs">Open in: {EDITORS[e]}</SelectItem>)}
            </SelectContent>
          </Select>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon-sm" onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')} aria-label="Switch theme">
                {theme === 'dark' ? <Sun /> : <Moon />}
              </Button>
            </TooltipTrigger>
            <TooltipContent>{theme === 'dark' ? 'Light' : 'Dark'} theme</TooltipContent>
          </Tooltip>
        </div>
        <div className="flex items-center justify-between gap-2 px-1 text-[11px] text-muted-foreground">
          <span title={data.generatedAt}>Read from the code {ago(data.generatedAt)} · {data.durationMs} ms</span>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon-xs" onClick={() => void regenerate()} disabled={loading} aria-label="Read the code again">
                <RefreshCw className={cn(loading && 'animate-spin')} />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Read the code again (it also happens on every save)</TooltipContent>
          </Tooltip>
        </div>
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  );
}
