// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ComputerButton } from '../computers/ComputerButton';
import { ComputerLine, ComputerStage, QUIET_MS } from '../computers/ComputerHome';
import { DEMO_CODE, DEMO_LINK, DEVBOX, STUDIO, codeRefused, demoApi } from '../computers/demo';
import { Refusal, obidComputers, type ComputersApi } from '../computers/obid';
import { rename as renameKept, resetComputers, startComputers, type Ui } from '../computers/store';
import { fingerprintText, type Computer, type ComputerList } from '../computers/view';

// The computer picker in the app (docs/contracts/computers-app.md, COMPUTERSAPP-1 to 12): the
// button, its menu, the sheets and the window's states, on the demo's stand-in for obid.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  resetComputers();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  document.body.innerHTML = '';
});

const studio = (more: Partial<Computer> = {}): Computer => ({ ...STUDIO, state: 'connected', route: { kind: 'lan', address: '192.168.1.30' }, ...more });
const ONLY: ComputerList = { current: 'this', computers: [] };
const ON_STUDIO = (more: Partial<Computer> = {}): ComputerList => ({ current: 'studio', computers: [studio(more), DEVBOX] });

/** The picker on an api, drawn as the window has it: the button (with its menu and sheets), Home's stage and the line. */
async function show(api: ComputersApi, ui: Partial<Ui> = {}, onHome = true) {
  await act(async () => { await startComputers(api, ui); });
  await act(async () => root.render(<><ComputerButton /><ComputerStage /><ComputerLine onHome={onHome} /></>));
}
const $ = <T extends Element = HTMLElement>(sel: string) => host.querySelector<T>(sel);
const $$ = (sel: string) => [...host.querySelectorAll<HTMLElement>(sel)];
const text = (sel: string) => $(sel)?.textContent ?? null;
const button = (label: string) => $$('button').find((b) => b.textContent === label || b.getAttribute('aria-label') === label);
const click = (el: Element | null | undefined) => act(async () => { (el as HTMLElement).click(); });
const until = (check: () => void) => vi.waitFor(async () => { await act(async () => {}); check(); }, { timeout: 5_000, interval: 10 });
const type = async (input: HTMLInputElement, value: string) => {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
};
const rail = () => $<HTMLButtonElement>('.rail-computer')!;
const rows = () => $$('.computer-menu .talk-choice-name').map((n) => n.textContent);

describe('COMPUTERSAPP-1: the computer button and its menu with only This Mac', () => {
  it('computersapp_1: the button is the laptop of This Mac, named for screen readers, its tooltip the name', async () => {
    await show(demoApi(ONLY, 0));
    expect(rail().getAttribute('aria-label')).toBe('Computer: This Mac. Change');
    expect(rail().dataset.tip).toBe('This Mac');
    expect(rail().getAttribute('aria-expanded')).toBe('false');
    expect(rail().classList.contains('has-dot')).toBe(false);
  });
  it('computersapp_1: its menu has This Mac checked, what this is for, then Add a computer…', async () => {
    await show(demoApi(ONLY, 0));
    await click(rail());
    expect(rail().getAttribute('aria-expanded')).toBe('true');
    expect(rows()).toEqual(['This Mac']);
    expect($('[role="menuitemradio"]')!.getAttribute('aria-checked')).toBe('true');
    expect(text('.computer-note')).toBe('Work on another Mac or Linux box from this window.');
    expect(text('.talk-choice.is-add')).toBe('Add a computer…');
  });
  it('computersapp_1: it sits at the mini sidebar’s foot, right above Settings', async () => {
    vi.stubGlobal('location', { search: '' });
    await act(async () => { await startComputers(demoApi(ONLY, 0)); });
    const { default: Sidebar } = await import('../Sidebar');
    const none = () => {};
    await act(async () => root.render(
      <Sidebar open atHome width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none} onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none}
        onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
    ));
    const foot = [...host.querySelectorAll('.rail > button')].slice(-2).map((b) => b.className.split(' ')[1]);
    expect(foot).toEqual(['rail-computer', 'rail-settings']);
  });
});

describe('COMPUTERSAPP-2: the menu with other computers', () => {
  it('computersapp_2: each computer by its icon and name; the one in use checked and how it’s reached, the others their kind', async () => {
    await show(demoApi(ON_STUDIO(), 0));
    expect(rail().getAttribute('aria-label')).toBe('Computer: Studio. Change');
    expect(rail().dataset.tip).toBe('Studio');
    await click(rail());
    expect(rows()).toEqual(['This Mac', 'Studio', 'devbox']);
    const about = $$('.computer-menu .talk-choice-about').map((n) => n.textContent);
    expect(about).toEqual(['Mac · Local network', 'Linux']);
    expect($('.computer-row .route-dot')).not.toBeNull();
    expect($$('[role="menuitemradio"]').map((r) => r.getAttribute('aria-checked'))).toEqual(['false', 'true', 'false']);
    // no note once there's another computer
    expect($('.computer-note')).toBeNull();
  });
  it('computersapp_2: a computer’s ⋯ has Rename… and Remove…', async () => {
    await show(demoApi(ON_STUDIO(), 0));
    await click(rail());
    await click(button('More for devbox'));
    expect($$('.computer-more [role="menuitem"]').map((b) => b.textContent)).toEqual(['Rename…', 'Remove…']);
  });
  it('computersapp_2: a route’s name is said as the design says it', async () => {
    for (const [kind, words] of [['tailnet', 'Mac · Tailscale'], ['vpn', 'Mac · VPN'], ['public', 'Mac · Internet'], ['ssh', 'Mac · SSH']] as const) {
      resetComputers();
      await show(demoApi(ON_STUDIO({ route: { kind } }), 0));
      if (rail().getAttribute('aria-expanded') === 'false') await click(rail());
      expect($$('.computer-menu .talk-choice-about')[0].textContent).toBe(words);
    }
  });
});

describe('COMPUTERSAPP-3: switching the window to another computer, and back', () => {
  it('computersapp_3: a row picked says where it’s going and that the other keeps its tasks, dimmed, until obid says it’s in use', async () => {
    const api = demoApi({ current: 'this', computers: [STUDIO, DEVBOX] }, 30);
    await show(api);
    await click(rail());
    await click($$('[role="menuitemradio"]')[1]);
    expect($('.computer-menu')).toBeNull();
    expect(text('.attention-label')).toBe('Switching to Studio…\nThis Mac keeps its tasks running.');
    expect(document.documentElement.dataset.computer).toBe('switching');
    await until(() => expect(rail().getAttribute('aria-label')).toBe('Computer: Studio. Change'));
    expect($('.attention-label')).toBeNull();
    expect(document.documentElement.dataset.computer).toBeUndefined();
  });
  it('computersapp_3: switching back to This Mac says so, and Studio keeps its tasks', async () => {
    await show(demoApi(ON_STUDIO(), 30));
    await click(rail());
    await click($$('[role="menuitemradio"]')[0]);
    expect(text('.attention-label')).toBe('Switching to This Mac…\nStudio keeps its tasks running.');
    await until(() => expect(rail().getAttribute('aria-label')).toBe('Computer: This Mac. Change'));
  });
});

describe('COMPUTERSAPP-4: adding a computer over SSH', () => {
  it('computersapp_4: the sheet has the switch, the host field and the words; Add computer waits for a host', async () => {
    await show(demoApi(ONLY, 0));
    await click(rail());
    await click($('.talk-choice.is-add'));
    expect(text('.computer-sheet h2')).toBe('Add a computer');
    expect(text('.computer-sheet .modal-lede')).toBe('Its conversations, tasks and folders open in this window. Oby has to be running there.');
    expect($$('[role="tab"]').map((t) => `${t.textContent}:${t.getAttribute('aria-selected')}`)).toEqual(['Over SSH:true', 'With a code:false']);
    expect(text('label[for="ssh-host"]')).toBe('SSH host');
    expect($<HTMLInputElement>('#ssh-host')!.placeholder).toBe('devbox, or you@192.168.1.20');
    expect(text('.computer-hint')).toBe('Oby uses your own SSH: your keys, agent and known hosts. Nothing is installed there.');
    expect(button('Add computer')!.hasAttribute('disabled')).toBe(true);
    await type($<HTMLInputElement>('#ssh-host')!, 'studio');
    expect(button('Add computer')!.hasAttribute('disabled')).toBe(false);
  });
  it('computersapp_4: its steps read as they happen and tick once done; added, the window switches to it', async () => {
    let step: (() => void) | null = null;
    const base = demoApi(ONLY, 0);
    const api: ComputersApi = {
      ...base,
      addOverSsh: async (host, progress) => {
        progress({ step: 'ssh', state: 'done' }); progress({ step: 'pair', state: 'doing' });
        await new Promise<void>((r) => { step = r; });
        return base.addOverSsh(host, progress);
      },
    };
    await show(api, { sheet: { kind: 'add', how: 'ssh', host: 'studio' } });
    await act(async () => { $<HTMLFormElement>('form')!.requestSubmit(); });
    expect(text('.computer-sheet h2')).toBe('Adding studio');
    expect($$('.modal-list li').map((l) => `${l.className}:${l.textContent}`)).toEqual([
      'is-done:Reached studio over SSH', 'is-now:Pairing this Mac with Oby on studio…', 'is-later:Finding the fastest way to reach it',
    ]);
    expect(button('Try again')).toBeUndefined();
    await act(async () => { step!(); });
    await until(() => expect(rail().getAttribute('aria-label')).toBe('Computer: Studio. Change'));
    expect($('.computer-sheet')).toBeNull();
  });
});

describe('COMPUTERSAPP-5: SSH says no', () => {
  it('computersapp_5: obid’s words show as it says them, what to type as code, with Cancel and Try again', async () => {
    const said = 'studio’s host key isn’t one you’ve accepted. Check it, then run `ssh studio` once in a terminal to accept it.';
    let tries = 0;
    const api: ComputersApi = { ...demoApi(ONLY, 0), addOverSsh: async () => { tries++; throw new Refusal(said); } };
    await show(api, { sheet: { kind: 'add', how: 'ssh', host: 'studio' } });
    await act(async () => { $<HTMLFormElement>('form')!.requestSubmit(); });
    await until(() => expect($('.is-trouble')).not.toBeNull());
    expect(text('.is-trouble')).toBe('studio’s host key isn’t one you’ve accepted. Check it, then run ssh studio once in a terminal to accept it.');
    expect(text('.is-trouble code')).toBe('ssh studio');
    expect($$('.modal-actions button').map((b) => b.textContent)).toEqual(['Cancel', 'Try again']);
    await click(button('Try again'));
    await until(() => expect(tries).toBe(2));
    await click(button('Cancel'));
    expect($('.computer-sheet')).toBeNull();
  });
});

describe('COMPUTERSAPP-6: adding a computer with a code', () => {
  it('computersapp_6: obid reads the link: its name and fingerprint to check, then the code', async () => {
    await show(demoApi(ONLY, 0), { sheet: { kind: 'add', how: 'code' } });
    expect(text('label[for="pair-link"]')).toBe('Link');
    expect(text('.computer-hint')).toBe('On the other computer, choose Show this computer’s code below. On Linux, run obid invite-computer.');
    expect($('#pair-code')).toBeNull();
    await type($<HTMLInputElement>('#pair-link')!, DEMO_LINK);
    await until(() => expect($('.computer-found')).not.toBeNull());
    expect(text('.computer-found b')).toBe('devbox');
    expect(text('.computer-found .fingerprint')).toBe('7F3A 91C2 5D0E 44B8 · 0C6E A1F7 93D2 6B10');
    expect($$('.computer-found span').at(-1)!.textContent).toBe('Check that devbox shows the same fingerprint.');
    expect(text('label[for="pair-code"]')).toBe('Code');
    expect(text('.computer-switch-link')).toBe('Show this computer’s code');
  });
  it('computersapp_6: a link obid refuses says so in obid’s words', async () => {
    await show(demoApi(ONLY, 0), { sheet: { kind: 'add', how: 'code' } });
    await type($<HTMLInputElement>('#pair-link')!, 'https://example.com');
    await until(() => expect(text('[role="alert"]')).toBe('That isn’t a link from Oby.'));
    expect($('.computer-found')).toBeNull();
  });
  it('computersapp_6: a wrong code is obid’s refusal and the sheet stays; the right one adds it and the window switches to it', async () => {
    await show(demoApi(ONLY, 0), { sheet: { kind: 'add', how: 'code', link: DEMO_LINK } });
    await until(() => expect($('#pair-code')).not.toBeNull());
    await type($<HTMLInputElement>('#pair-code')!, '111 111');
    await act(async () => { $<HTMLFormElement>('form')!.requestSubmit(); });
    await until(() => expect(text('[role="alert"]')).toBe(codeRefused('devbox')));
    expect($('.computer-sheet')).not.toBeNull();
    await type($<HTMLInputElement>('#pair-code')!, `${DEMO_CODE.slice(0, 3)} ${DEMO_CODE.slice(3)}`);
    await act(async () => { $<HTMLFormElement>('form')!.requestSubmit(); });
    await until(() => expect(rail().getAttribute('aria-label')).toBe('Computer: devbox. Change'));
    expect($('.computer-sheet')).toBeNull();
  });
});

describe('COMPUTERSAPP-7: this computer’s code', () => {
  it('computersapp_7: its code, fingerprint, the addresses it can be reached at with their switches, and how long it lasts', async () => {
    const until1432 = new Date(2026, 10, 20, 14, 32).getTime();
    const api: ComputersApi = { ...demoApi(ONLY, 0), invite: async () => ({ name: 'Studio', code: '482913', link: DEMO_LINK, fingerprint: '7f3a91c25d0e44b80c6ea1f793d26b10', expiresAt: until1432, reach: [{ kind: 'lan', address: '192.168.1.30', on: true }, { kind: 'tailnet', address: '100.101.3.4', on: true }] }) };
    const setReach = vi.spyOn(api, 'setReach');
    await show(api, { sheet: { kind: 'add', how: 'code' } });
    await click($('.computer-switch-link'));
    await until(() => expect($('.big-code')).not.toBeNull());
    expect(text('.computer-sheet h2')).toBe('This Mac’s code');
    expect(text('.modal-lede')).toBe('On the Mac you’re adding Studio to, paste its link and type this code.');
    expect(text('.big-code')).toBe('482 913');
    expect($('.big-code')!.getAttribute('aria-label')).toBe('Code 4 8 2 9 1 3');
    expect(text('.fingerprint')).toBe('7F3A 91C2 5D0E 44B8 · 0C6E A1F7 93D2 6B10');
    expect($$('.computer-sheet .modal-label').map((l) => l.textContent)).toEqual(['Fingerprint', 'Other computers reach this Mac at']);
    expect($$('.reach-row').map((r) => r.textContent)).toEqual(['Local network192.168.1.30', 'Tailscale100.101.3.4']);
    expect($$('.computer-hint').at(-1)!.textContent).toBe(`Good until ${new Date(until1432).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}, for one computer.`);
    await click(button('Tailscale'));
    expect(button('Tailscale')!.getAttribute('aria-checked')).toBe('false');
    expect(setReach).toHaveBeenCalledWith('tailnet', false);
    expect($$('.modal-actions button').map((b) => b.textContent)).toEqual(['Copy link', 'Done']);
    await click(button('Done'));
    expect($('.computer-sheet')).toBeNull();
  });
  it('computersapp_7: with no address to offer, it says to add this Mac over SSH instead', async () => {
    const api: ComputersApi = { ...demoApi(ONLY, 0), invite: async () => ({ name: 'Studio', code: '482913', link: DEMO_LINK, fingerprint: '00', expiresAt: Date.now(), reach: [] }) };
    await show(api, { sheet: { kind: 'code' } });
    await until(() => expect($$('.computer-hint').map((h) => h.textContent)).toContain('This Mac has no address another computer can reach. Add it over SSH instead.'));
  });
  it('computersapp_7: a fingerprint is its first 16 bytes in groups of four hex digits', () => {
    expect(fingerprintText('sha256:7f:3a:91:c2:5d:0e:44:b8:0c:6e:a1:f7:93:d2:6b:10:ff:ff')).toBe('7F3A 91C2 5D0E 44B8 · 0C6E A1F7 93D2 6B10');
  });
});

describe('COMPUTERSAPP-8: reconnecting on a fallback route', () => {
  it('computersapp_8: the connection line says so only once it has taken more than 2 s', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
    await show(demoApi(ON_STUDIO({ state: 'reconnecting', from: 'lan', route: { kind: 'ssh' } }), 0));
    expect($('.connection')).toBeNull();
    await act(async () => { vi.advanceTimersByTime(QUIET_MS - 100); });
    expect($('.connection')).toBeNull();
    await act(async () => { vi.advanceTimersByTime(200); });
    expect(text('.connection')).toBe('Studio’s local network doesn’t answer. Reconnecting over SSH…');
    expect($('.connection')!.classList.contains('is-down')).toBe(false);
    // quiet otherwise: no dot, nothing dimmed
    expect(rail().classList.contains('has-dot')).toBe(false);
    expect(document.documentElement.dataset.computer).toBeUndefined();
  });
  it('computersapp_8: a fallback that takes under 2 s shows nothing', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
    const follow: ((l: ComputerList) => void)[] = [];
    const api: ComputersApi = { ...demoApi(ONLY, 0), list: async () => ON_STUDIO({ state: 'reconnecting', from: 'lan', route: { kind: 'tailnet' } }), follow: (on) => { follow.push(on); return () => {}; } };
    await show(api);
    await act(async () => { vi.advanceTimersByTime(1500); });
    await act(async () => { follow[0](ON_STUDIO({ route: { kind: 'tailnet' } })); });
    await act(async () => { vi.advanceTimersByTime(QUIET_MS); });
    expect($('.connection')).toBeNull();
  });
});

describe('COMPUTERSAPP-9: unreachable', () => {
  const tried = ON_STUDIO({ state: 'lost', route: undefined, tried: [
    { route: 'lan', what: '192.168.1.30', why: 'no answer' }, { route: 'tailnet', what: '100.101.3.4', why: 'no answer' }, { route: 'ssh', why: 'studio didn’t answer over SSH.' },
  ] });
  it('computersapp_9: Home says so with every route tried, Try again and Back to This Mac; the button has the needs-you dot; the window dims', async () => {
    await show(demoApi(tried, 0));
    expect(text('.attention-label')).toBe('Oby can’t reach Studio right now.\nIts tasks keep running there, and Oby keeps trying.');
    expect($$('.computer-tried li').map((l) => l.textContent)).toEqual(['Local network · 192.168.1.30 · no answer', 'Tailscale · 100.101.3.4 · no answer', 'SSH · studio didn’t answer over SSH.']);
    expect($$('.chips .chip').map((c) => c.textContent)).toEqual(['Try again', 'Back to This Mac']);
    expect(rail().className).toContain('has-dot is-needs');
    expect(document.documentElement.dataset.computer).toBe('lost');
    // on Home, the stage says it: no connection line too
    expect($('.connection')).toBeNull();
    await click(rail());
    expect($$('.computer-menu .talk-choice-about')[0].textContent).toBe('Can’t reach it right now');
  });
  it('computersapp_9: outside Home the connection line says it, its edge red', async () => {
    await show(demoApi(tried, 0), {}, false);
    expect(text('.connection')).toBe('Oby can’t reach Studio right now.');
    expect($('.connection')!.classList.contains('is-down')).toBe(true);
  });
  it('computersapp_9: Try again probes every route now', async () => {
    const api = demoApi(tried, 0);
    const retry = vi.spyOn(api, 'retry');
    await show(api);
    await click(button('Try again'));
    expect(retry).toHaveBeenCalledWith('studio');
    await until(() => expect($('.computer-tried')).toBeNull());
  });
  it('computersapp_9: Back to This Mac switches the window back', async () => {
    const api = demoApi(tried, 0);
    const use = vi.spyOn(api, 'use');
    await show(api);
    await click(button('Back to This Mac'));
    expect(use).toHaveBeenCalledWith('this');
    await until(() => expect(rail().getAttribute('aria-label')).toBe('Computer: This Mac. Change'));
    expect(document.documentElement.dataset.computer).toBeUndefined();
  });
});

describe('COMPUTERSAPP-10: Rename… in place', () => {
  it('computersapp_10: the row’s name becomes a field; Enter keeps the new name', async () => {
    const api = demoApi(ON_STUDIO(), 0);
    const rename = vi.spyOn(api, 'rename');
    await show(api);
    await click(rail());
    await click(button('More for devbox'));
    await click(button('Rename…'));
    const field = $<HTMLInputElement>('input[aria-label="Computer name"]')!;
    expect(field.value).toBe('devbox');
    field.focus();
    await type(field, 'Build box');
    await act(async () => { field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); });
    await until(() => expect(rows()).toEqual(['This Mac', 'Studio', 'Build box']));
    expect(rename).toHaveBeenCalledWith('devbox', 'Build box');
  });
  it('computersapp_10: Esc leaves the name as it was, and the menu open', async () => {
    const api = demoApi(ON_STUDIO(), 0);
    const rename = vi.spyOn(api, 'rename');
    await show(api);
    await click(rail());
    await click(button('More for devbox'));
    await click(button('Rename…'));
    const field = $<HTMLInputElement>('input[aria-label="Computer name"]')!;
    field.focus();
    await type(field, 'Nope');
    await act(async () => { dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); });
    // a browser can still blur the field as it goes (Chrome does): that late blur keeps nothing
    await act(async () => { await renameKept('devbox', 'Nope'); });
    expect(rows()).toEqual(['This Mac', 'Studio', 'devbox']);
    expect($('.computer-menu')).not.toBeNull();
    expect(rename).not.toHaveBeenCalled();
  });
});

describe('COMPUTERSAPP-11: Remove…', () => {
  it('computersapp_11: the sheet says what removing does; Remove of the computer in use goes back to This Mac first', async () => {
    const api = demoApi(ON_STUDIO(), 0);
    const order: string[] = [];
    vi.spyOn(api, 'use').mockImplementation(async (id) => { order.push(`use ${id}`); });
    const remove = vi.spyOn(api, 'remove');
    remove.mockImplementation(async (id) => { order.push(`remove ${id}`); });
    await show(api);
    await click(rail());
    await click(button('More for Studio'));
    await click(button('Remove…'));
    expect(text('.computer-sheet h2')).toBe('Remove Studio?');
    expect(text('.modal-lede')).toBe('This Mac stops reaching Studio, and Studio forgets this Mac’s key. Its tasks and folders stay on Studio.');
    expect($$('.modal-actions button').map((b) => `${b.textContent}${b.classList.contains('is-danger') ? ' (danger)' : ''}`)).toEqual(['Cancel', 'Remove (danger)']);
    await act(async () => { $<HTMLFormElement>('form')!.requestSubmit(); });
    await until(() => expect(order).toEqual(['use this', 'remove studio']));
    expect($('.computer-sheet')).toBeNull();
  });
  it('computersapp_11: removing another computer leaves the window where it is', async () => {
    const api = demoApi(ON_STUDIO(), 0);
    const use = vi.spyOn(api, 'use');
    await show(api, { sheet: { kind: 'remove', id: 'devbox' } });
    await act(async () => { $<HTMLFormElement>('form')!.requestSubmit(); });
    await until(() => expect(rail().getAttribute('aria-label')).toBe('Computer: Studio. Change'));
    expect(use).not.toHaveBeenCalled();
    await click(rail());
    await until(() => expect(rows()).toEqual(['This Mac', 'Studio']));
  });
});

describe('COMPUTERSAPP-12: on obid, offered only once obid serves the picker', () => {
  it('computersapp_12: obid serves none of the picker’s routes yet: no button, no menu, and no call', async () => {
    const call = vi.fn();
    (window as unknown as { app: unknown }).app = { obid: { call } };
    await act(async () => { await startComputers(await obidComputers()); });
    await act(async () => root.render(<><ComputerButton /><ComputerStage /><ComputerLine onHome /></>));
    expect($('.rail-computer')).toBeNull();
    expect($('.attention')).toBeNull();
    expect(call).not.toHaveBeenCalled();
    delete (window as unknown as { app?: unknown }).app;
  });
  it('computersapp_12: nothing of the demo data shows outside ?everything', async () => {
    await act(async () => root.render(<ComputerButton />));
    expect($('.rail-computer')).toBeNull();
    expect(document.documentElement.dataset.computer).toBeUndefined();
  });
});
