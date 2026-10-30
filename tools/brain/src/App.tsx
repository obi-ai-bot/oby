import { useEffect, type ReactNode } from 'react';
import { Loader2, TriangleAlert } from 'lucide-react';
import { SidebarInset, SidebarProvider, SidebarTrigger } from '@/components/ui/sidebar';
import { Separator } from '@/components/ui/separator';
import { TooltipProvider } from '@/components/ui/tooltip';
import { AppSidebar } from '@/components/app-sidebar';
import { SearchCommand } from '@/components/search';
import { BrainProvider, useBrainState } from '@/lib/brain';
import { PrefsProvider } from '@/lib/prefs';
import { useRoute, type Route } from '@/lib/router';
import { OverviewView } from '@/views/overview';
import { KeptView, ScreensView, StatesView, WordsView } from '@/views/design';
import { AreasView, BackgroundView, EndpointsView, ModulesView, PhoneView, StoredView, WireView } from '@/views/obid';
import { ChecksView, RulesView } from '@/views/quality';

const TITLES: Record<string, string> = {
  '': 'Overview', screens: 'Screens', kept: 'Kept locally', states: 'States to open', words: 'Words',
  endpoints: 'Endpoints', phone: 'Phone allowlist', areas: 'Areas', modules: 'Modules', background: 'Background work',
  stored: 'Stored files', wire: 'Wire types', rules: 'Rules', checks: 'Checks',
};

function View({ route }: { route: Route }) {
  switch (route.path[0] ?? '') {
    case '': return <OverviewView />;
    case 'screens': return <ScreensView route={route} />;
    case 'kept': return <KeptView />;
    case 'states': return <StatesView />;
    case 'words': return <WordsView />;
    case 'endpoints': return <EndpointsView route={route} />;
    case 'phone': return <PhoneView />;
    case 'areas': return <AreasView />;
    case 'modules': return <ModulesView route={route} />;
    case 'background': return <BackgroundView />;
    case 'stored': return <StoredView />;
    case 'wire': return <WireView />;
    case 'rules': return <RulesView />;
    case 'checks': return <ChecksView route={route} />;
    default: return <p className="text-sm text-muted-foreground">No such page.</p>;
  }
}

function Centered({ children }: { children: ReactNode }) {
  return <div className="flex min-h-svh items-center justify-center p-8 text-sm text-muted-foreground">{children}</div>;
}

function Shell() {
  const { brain, error, loading } = useBrainState();
  const route = useRoute();
  const section = route.path[0] ?? '';
  useEffect(() => { document.title = `${TITLES[section] ?? 'Brain'} · Oby Brain`; }, [section]);
  // A new page starts at the top (a filter change in the query keeps the scroll).
  const page = route.path.join('/');
  useEffect(() => { window.scrollTo({ top: 0 }); }, [page]);

  if (!brain) {
    if (error) {
      return (
        <Centered>
          <div className="max-w-2xl space-y-3">
            <p className="flex items-center gap-2 font-medium text-foreground"><TriangleAlert className="size-4 text-delete" />Couldn't read the code</p>
            <pre className="overflow-auto rounded-lg border bg-muted/40 p-3 font-mono text-xs whitespace-pre-wrap">{error}</pre>
          </div>
        </Centered>
      );
    }
    return <Centered><Loader2 className="mr-2 size-4 animate-spin" />Reading the code…</Centered>;
  }

  return (
    <SidebarProvider>
      <AppSidebar route={route} />
      <SidebarInset className="min-w-0">
        <header className="sticky top-0 z-20 flex h-12 shrink-0 items-center gap-2 rounded-t-xl border-b bg-background/85 px-3 backdrop-blur">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mr-1 data-[orientation=vertical]:h-4" />
          <span className="truncate text-sm font-medium">{TITLES[section] ?? ''}</span>
          {loading && <Loader2 className="size-3.5 animate-spin text-muted-foreground" />}
          <div className="ml-auto flex w-full max-w-sm justify-end"><SearchCommand /></div>
        </header>
        <main id="main" className="min-w-0 flex-1 p-4 md:p-6">
          <View route={route} />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}

export function App() {
  return (
    <PrefsProvider>
      <TooltipProvider delayDuration={250}>
        <BrainProvider>
          <Shell />
        </BrainProvider>
      </TooltipProvider>
    </PrefsProvider>
  );
}
