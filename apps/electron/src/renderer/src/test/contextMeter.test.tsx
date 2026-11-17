import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import ContextMeter, { contextShare, tokenWords } from '../ContextMeter';
import { pageOf } from '../jobPage';

// The context meter (docs/contracts/usage.md, USAGE-15 to 17): a task's page shows how full its
// agent's context window is, from the numbers obid passes on from the agent.

describe('USAGE-17: the context meter on a task', () => {
  it('usage_17_tokens_of_the_window_with_a_bar_yellow_past_80_red_past_90', () => {
    const at = (used: number) => renderToStaticMarkup(<ContextMeter used={used} size={200_000} />);
    expect(at(41_000)).toContain('41K / 200K');
    expect(at(41_000)).toContain('aria-valuenow="21"');
    expect(at(41_000)).not.toMatch(/is-high|is-over/);
    expect(at(170_000)).toContain('tc-meter is-high');
    expect(at(185_000)).toContain('tc-meter is-over');
  });

  it('usage_17_with_no_size_the_tokens_alone_and_no_bar', () => {
    const html = renderToStaticMarkup(<ContextMeter used={1_234_567} size={null} />);
    expect(html).toContain('1.2M tokens');
    expect(html).not.toContain('role="meter"');
    expect(contextShare(10, null)).toBeNull();
    expect(contextShare(300, 200)).toBe(100);
    expect(tokenWords(999)).toBe('999');
    expect(tokenWords(2_000_000)).toBe('2M');
  });

  it('usage_17_a_task_page_has_its_context_only_when_obid_said_it', () => {
    const job = { id: 'j1', status: 'running', created: '2026-11-17T10:00:00Z', step: '' };
    expect(pageOf({ ...job, contextWindow: { used: 50_000, size: 200_000 } } as never).context).toEqual({ used: 50_000, size: 200_000 });
    expect(pageOf({ ...job, contextWindow: { used: 50_000 } } as never).context).toEqual({ used: 50_000, size: null });
    expect(pageOf(job as never).context).toBeUndefined();
  });
});
