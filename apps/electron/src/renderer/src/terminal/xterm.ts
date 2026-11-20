import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

// One terminal on screen, as the panel uses it (TerminalView.tsx): xterm.js, which T3 Code drew its
// terminals with, and its fit add-on, which measures how many columns and rows its box holds.
export type Screen = {
  open(box: HTMLElement): void;
  // the columns and rows the box holds now; null while it has no size (hidden, or not laid out)
  measure(): { cols: number; rows: number } | null;
  size(): { cols: number; rows: number };
  resize(cols: number, rows: number): void;
  write(data: string): void;
  onData(f: (data: string) => void): void;
  focus(): void;
  dispose(): void;
};

// at most this many lines kept above the screen
export const SCROLLBACK = 5000;

const css = (name: string, fallback: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;

export function makeScreen(): Screen {
  const term = new Terminal({
    scrollback: SCROLLBACK,
    cursorBlink: true,
    fontFamily: css('--mono', 'ui-monospace, SFMono-Regular, Menlo, monospace'),
    fontSize: 12,
    allowProposedApi: false,
    // ⌥ types the Mac's own characters (é, ü, ©), as Terminal does by default
    macOptionIsMeta: false,
    theme: { background: css('--bg', '#0b0b0b'), foreground: css('--ink', '#ededed'), cursor: css('--ink', '#ededed') },
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  let shown: HTMLElement | null = null;
  return {
    open: (box) => { shown = box; term.open(box); },
    measure: () => {
      // a box that isn't drawn (hidden, or out of the page) has no size, whatever its CSS width says
      if (!shown?.isConnected || !shown.getClientRects().length) return null;
      const d = fit.proposeDimensions();
      return d && Number.isFinite(d.cols) && Number.isFinite(d.rows) && d.cols >= 2 && d.rows >= 1 ? { cols: d.cols, rows: d.rows } : null;
    },
    size: () => ({ cols: term.cols, rows: term.rows }),
    resize: (cols, rows) => term.resize(cols, rows),
    write: (data) => term.write(data),
    onData: (f) => { term.onData(f); },
    focus: () => term.focus(),
    dispose: () => term.dispose(),
  };
}
