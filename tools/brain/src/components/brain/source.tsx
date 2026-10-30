import { useState } from 'react';
import { Highlight, themes } from 'prism-react-renderer';
import { Check, ChevronDown, ChevronRight, Copy, FileCode2 } from 'lucide-react';
import { useBrain, shortFile } from '@/lib/brain';
import { editorUrl, usePrefs } from '@/lib/prefs';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

/** `file:line`: opens the file in the chosen editor (or copies the path). */
export function SourceLink({ file, line, className, short = true, label: given }: { file: string; line?: number; className?: string; short?: boolean; label?: string }) {
  const { data } = useBrain();
  const { editor } = usePrefs();
  const [copied, setCopied] = useState(false);
  const url = editorUrl(editor, data.root, file, line);
  const label = given ?? `${short ? shortFile(file) : file}${line ? `:${line}` : ''}`;
  const copy = () => {
    void navigator.clipboard?.writeText(`${file}${line ? `:${line}` : ''}`);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {url ? (
          <a href={url} className={cn('inline-flex min-w-0 items-center gap-1 font-mono text-xs text-muted-foreground hover:text-foreground', className)}>
            <FileCode2 className="size-3 shrink-0" />
            <span className="truncate">{label}</span>
          </a>
        ) : (
          <button type="button" onClick={copy} className={cn('inline-flex min-w-0 items-center gap-1 font-mono text-xs text-muted-foreground hover:text-foreground', className)}>
            {copied ? <Check className="size-3 shrink-0" /> : <FileCode2 className="size-3 shrink-0" />}
            <span className="truncate">{label}</span>
          </button>
        )}
      </TooltipTrigger>
      <TooltipContent><span className="font-mono">{file}{line ? `:${line}` : ''}</span>{url ? ' · open in your editor' : ' · copy the path'}</TooltipContent>
    </Tooltip>
  );
}

/** Source code with highlighting and the file's real line numbers. */
export function CodeBlock({ code, startLine = 1, language = 'tsx', maxHeight = 520, className }: { code: string; startLine?: number; language?: string; maxHeight?: number; className?: string }) {
  const { theme } = usePrefs();
  const [copied, setCopied] = useState(false);
  return (
    <div className={cn('group relative overflow-hidden rounded-lg border bg-muted/30', className)}>
      <Button
        variant="ghost" size="icon-xs" className="absolute top-1.5 right-1.5 z-10 opacity-0 group-hover:opacity-100"
        onClick={() => { void navigator.clipboard?.writeText(code); setCopied(true); setTimeout(() => setCopied(false), 1200); }}
        aria-label="Copy the code"
      >
        {copied ? <Check /> : <Copy />}
      </Button>
      <Highlight code={code} language={language} theme={theme === 'dark' ? themes.oneDark : themes.oneLight}>
        {({ tokens, getLineProps, getTokenProps }) => (
          <pre className="overflow-auto py-2 font-mono text-[12px] leading-[1.55]" style={{ maxHeight }}>
            {tokens.map((line, i) => (
              <div key={i} {...getLineProps({ line })} className="flex px-3 hover:bg-foreground/[0.03]">
                <span className="mr-4 w-10 shrink-0 text-right text-muted-foreground/50 select-none">{startLine + i}</span>
                <span className="whitespace-pre">
                  {line.map((token, k) => {
                    const props = getTokenProps({ token });
                    return <span key={k} {...props} style={{ ...props.style, background: 'transparent' }} />;
                  })}
                </span>
              </div>
            ))}
          </pre>
        )}
      </Highlight>
    </div>
  );
}

/** A collapsed code block ("show code"). */
export function CodeToggle({ code, startLine, label = 'Code', defaultOpen = false, language }: { code: string; startLine?: number; label?: string; defaultOpen?: boolean; language?: string }) {
  const [open, setOpen] = useState(defaultOpen);
  const lines = code.split('\n').length;
  return (
    <div>
      <button type="button" onClick={() => setOpen(o => !o)} className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
        {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
        {label} <span className="opacity-60">({lines} {lines === 1 ? 'line' : 'lines'})</span>
      </button>
      {open && <CodeBlock code={code} startLine={startLine} className="mt-2" language={language} />}
    </div>
  );
}
