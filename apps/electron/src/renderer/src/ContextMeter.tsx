// How full a task's agent's context window is (docs/contracts/usage.md, USAGE-15 to 17), from the
// agent's own numbers as obid passes them on: the tokens its next call carries, of the window's size
// when the tool names it. The T3 Code way (ThreadDetailsContext): a hairline bar, yellow past 80%,
// red past 90%; with no size, the tokens alone and no bar. Words proposed, Sangam's to rewrite.

// 41K, 1.2M: a count of tokens, short
export const tokenWords = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1).replace(/\.0$/, '')}M` : n >= 1e3 ? `${Math.round(n / 1e3)}K` : String(n));

// the share of the window in use, 0 to 100, or null with no size to measure it by
export const contextShare = (used: number, size: number | null) => (size && size > 0 ? Math.round(Math.max(0, Math.min(100, (used / size) * 100))) : null);

export default function ContextMeter({ used, size }: { used: number; size: number | null }) {
  const share = contextShare(used, size);
  const tone = share == null ? '' : share >= 90 ? ' is-over' : share >= 80 ? ' is-high' : '';
  const value = size ? `${tokenWords(used)} / ${tokenWords(size)}` : `${tokenWords(used)} tokens`;
  return (
    <div className="tc-context" title={size ? `${used.toLocaleString('en-US')} of ${size.toLocaleString('en-US')} tokens` : undefined}>
      <span className="tc-window">
        Context
        {share != null && (
          <span className={`tc-meter${tone}`} role="meter" aria-valuenow={share} aria-valuemin={0} aria-valuemax={100} aria-label={`Context: ${share}% used`}>
            <span className="is-job" style={{ width: `${share}%` }} />
          </span>
        )}
        {value}{share != null && <b>{` · ${share}%`}</b>}
      </span>
    </div>
  );
}
