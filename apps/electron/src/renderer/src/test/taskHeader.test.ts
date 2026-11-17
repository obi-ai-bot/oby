// @vitest-environment happy-dom
import { describe, expect, it, vi } from 'vitest';

// A task's header names its agent, then its account: an account named after its agent isn't named
// twice ("Codex", never "Codex · Codex").
vi.stubGlobal('app', { obid: { call: vi.fn() } });
const { header } = await import('../TaskCard');

describe("a task's header", () => {
  it('names the agent once when the account is named after it', () => {
    expect(header({ agent: 'codex', account: 'Codex' })).toBe('Codex');
    expect(header({ agent: 'codex', account: 'codex' })).toBe('Codex');
    expect(header({ agent: 'claude', account: 'Claude Code' })).toBe('Claude Code');
    expect(header({ agent: 'claude', account: '' })).toBe('Claude Code');
  });

  it('names an account of its own after the agent', () => {
    expect(header({ agent: 'codex', account: 'Work' })).toBe('Codex · Work');
    expect(header({ agent: 'claude', account: 'Personal' })).toBe('Claude Code · Personal');
  });
});
