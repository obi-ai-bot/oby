import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { InboxItem } from '../obid';

// live.ts reads the page's window, address and storage as it loads
let itemAsk: typeof import('../live').itemAsk;
beforeAll(async () => {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', {});
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  ({ itemAsk } = await import('../live'));
});

const item = (kind: string, more: Partial<InboxItem> = {}): InboxItem => ({
  id: 'j1-permission-p1', kind, job: 'j1', title: 'Fix the login', text: '', options: [], details: null,
  created: '2026-10-30T10:00:00Z', state: 'open', later: null, headline: null, recommended: null, ...more,
});

describe('BYPASS-5: the agent’s question is Sangam’s Question card', () => {
  it('bypass_5_shows_the_question_as_its_headline_with_the_agents_options', () => {
    const ask = itemAsk(item('agentQuestion', { text: 'Which store?', options: ['SQLite', 'Postgres'], details: ['One file', ''] }));
    expect(ask).toEqual({ kind: 'question', headline: 'Which store?', text: '', options: ['SQLite', 'Postgres'] });
  });

  it('bypass_5_leaves_a_permission_a_permission', () => {
    const ask = itemAsk(item('permission', { text: 'Run npm install', options: ['Allow', 'Deny'] }), 'npm install');
    expect(ask.kind).toBe('permission');
    expect(ask.detail).toBe('npm install');
  });
});
