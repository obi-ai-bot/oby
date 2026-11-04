import { describe, expect, it, vi } from 'vitest';

// A question's card says "I'll ping you": the alert that keeps it (main/notify.ts, ASK-26).
vi.mock('electron', () => ({ app: {}, ipcMain: {}, Notification: {}, powerMonitor: {}, shell: {} }));
vi.mock('../../../main/obid', () => ({ obid: vi.fn(), send: vi.fn() }));
vi.mock('../../../main/device', () => ({ answerLimit: vi.fn(), writeOwn: vi.fn() }));

const card = (state: string) => ({ exchange: 'x', owner: 'dana', name: 'Dana', question: 'How is it going?', state });

describe('the ping', () => {
  it('ask_26_an_answer_pings_and_waiting_does_not', async () => {
    const { pingOf } = await import('../../../main/notify');
    expect(pingOf({ text: '> Going well.', asks: [card('answered')] })).toEqual({ title: 'Dana’s Oby answered', body: '> Going well.' });
    expect(pingOf({ text: 'Dana said no.', asks: [card('declined')] })?.title).toBe('Dana’s Oby answered');
    expect(pingOf({ text: 'It ran out.', asks: [card('expired')] })?.title).toBe('Dana’s Oby answered');
    for (const state of ['asking', 'quiet', 'waiting', 'cancelled']) expect(pingOf({ text: 'Words.', asks: [card(state)] })).toBeNull();
    expect(pingOf({ text: 'Just Oby talking.' })).toBeNull();
  });

  it('ask_26_a_name_cannot_pass_for_someone_else', async () => {
    const { pingOf } = await import('../../../main/notify');
    expect(pingOf({ text: 'Hi', asks: [{ ...card('answered'), name: 'Dаna' }] })?.title).not.toBe('Dana’s Oby answered');
  });

  it('notify_refused_bounces_the_dock_and_the_badge_counts_what_waits', async () => {
    const { refused, waiting } = await import('../../../main/notify');
    const bounce = vi.fn();
    vi.spyOn(console, 'error').mockImplementation(() => {});
    refused('UNErrorDomain error 1', { bounce });
    expect(bounce).toHaveBeenCalledWith('critical');
    expect(waiting(new Map([['a', { needs: true }], ['b', { needs: false }], ['c', { needs: true }]]))).toBe('2');
    expect(waiting(new Map([['b', { needs: false }]]))).toBe('');
  });
});
