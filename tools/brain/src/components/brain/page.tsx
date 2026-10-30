import type { ReactNode } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle, CardAction } from '@/components/ui/card';
import { cn } from '@/lib/utils';

export function PageHeader({ title, description, children, eyebrow }: { title: ReactNode; description?: ReactNode; children?: ReactNode; eyebrow?: ReactNode }) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-4 pb-6">
      <div className="min-w-0 space-y-1.5">
        {eyebrow && <div className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{eyebrow}</div>}
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        {description && <div className="max-w-3xl text-sm text-muted-foreground">{description}</div>}
      </div>
      {children && <div className="flex items-center gap-2">{children}</div>}
    </div>
  );
}

/** A titled card; `count` shows next to the title. */
export function Section({ title, description, count, action, children, className, contentClassName }: {
  title: ReactNode; description?: ReactNode; count?: number; action?: ReactNode; children: ReactNode; className?: string; contentClassName?: string;
}) {
  return (
    <Card className={cn('gap-3', className)}>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-sm">
          {title}
          {count !== undefined && <span className="rounded-full bg-muted px-1.5 font-mono text-[11px] font-normal text-muted-foreground">{count}</span>}
        </CardTitle>
        {description && <CardDescription>{description}</CardDescription>}
        {action && <CardAction>{action}</CardAction>}
      </CardHeader>
      <CardContent className={contentClassName}>{children}</CardContent>
    </Card>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="text-sm text-muted-foreground italic">{children}</p>;
}

/** label: value rows. */
export function Facts({ items }: { items: Array<[ReactNode, ReactNode] | null | false> }) {
  return (
    <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-2 text-sm">
      {items.filter(Boolean).map((it, i) => {
        const [k, v] = it as [ReactNode, ReactNode];
        return (
          <div key={i} className="contents">
            <dt className="text-muted-foreground">{k}</dt>
            <dd className="min-w-0">{v}</dd>
          </div>
        );
      })}
    </dl>
  );
}
