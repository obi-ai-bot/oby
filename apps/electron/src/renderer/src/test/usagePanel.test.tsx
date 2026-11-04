import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

// The Usage panel, as Composer draws it (docs/contracts/usage.md, USAGE-6 and USAGE-10).

const accounts = [
  { id: 'c', agent: 'claude', name: 'Default', payer: 'You', used: 91, weekly: true, session: null, sessionResets: null, resets: 'Resets Thursday', signedIn: true },
  { id: 'd', agent: 'claude', name: 'Daniel', payer: 'You', used: 40, weekly: true, session: null, sessionResets: null, resets: 'Not signed in', signedIn: false },
  { id: 'x', agent: 'codex', name: 'Codex', payer: 'You', used: 66, weekly: true, session: 0, sessionResets: '1:56 PM', resets: 'Resets Wednesday', signedIn: true },
  { id: 'u', agent: 'cursor', name: 'Cursor', payer: 'You', used: 12, weekly: false, session: null, sessionResets: null, resets: 'Resets Wednesday', signedIn: true },
];

vi.mock('../SettingsView', async (real) => ({ ...(await real<object>()), useAccounts: () => [accounts] }));

async function rows() {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', { app: { obid: { call: async () => ({ status: 404, text: '' }) } } });
  const { UsagePanel } = await import('../Composer');
  const html = renderToStaticMarkup(<UsagePanel onClose={() => {}} onBlur={() => {}} />);
  return html.split('usage-row').slice(1);
}

describe('the Usage panel', () => {
  it('USAGE-10: an account with only a week shows it as its number and its bar', async () => {
    const [claude] = await rows();
    expect(claude).toContain('>91%<');
    expect(claude).toContain('aria-valuenow="91"');
    expect(claude).toContain('91% used');
    expect(claude).not.toContain('>0%<');
    expect(claude).not.toContain('5-hour');
  });

  it('USAGE-6: Cursor has its number and its bar too', async () => {
    const [, , , cursor] = await rows();
    expect(cursor).toContain('>12%<');
    expect(cursor).toContain('aria-valuenow="12"');
  });

  it('USAGE-10: a session still going keeps its reset and its bar, at 0% too', async () => {
    const [, , codex] = await rows();
    expect(codex).toContain('Resets 1:56 PM');
    expect(codex).toContain('aria-valuenow="0"');
    expect(codex).toContain('5-hour session');
  });

  it('USAGE-10: an account that isn\'t signed in shows no bar from an old reading', async () => {
    const [, daniel] = await rows();
    expect(daniel).not.toContain('role="meter"');
    expect(daniel).toContain('Not signed in');
  });
});
