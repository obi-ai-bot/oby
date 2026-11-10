// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { CursorGroup } from '../obid';

// Settings › Agents' Cursor Model row (docs/contracts/agents.md, AGENTS-11): Cursor's models once
// each, the chosen one's efforts and Fast apart, every choice one id Cursor listed.
const GROUPS: CursorGroup[] = [
  { base: 'auto', name: 'Auto', default: 'auto', variants: [{ id: 'auto', fast: false }] },
  { base: 'grok-4.7', name: 'Grok 4.7', default: 'grok-4.7-medium', variants: [
    { id: 'grok-4.7-low', effort: 'low', fast: false }, { id: 'grok-4.7-low-fast', effort: 'low', fast: true },
    { id: 'grok-4.7-medium', effort: 'medium', fast: false }, { id: 'grok-4.7-medium-fast', effort: 'medium', fast: true },
    { id: 'grok-4.7-high', effort: 'high', fast: false }] },
  { base: 'gpt-5.3-codex', name: 'Codex 5.3', default: 'gpt-5.3-codex', variants: [
    { id: 'gpt-5.3-codex-low', effort: 'low', fast: false }, { id: 'gpt-5.3-codex', fast: false },
    { id: 'gpt-5.3-codex-fast', fast: true }, { id: 'gpt-5.3-codex-xhigh', effort: 'xhigh', fast: false }] },
];

let root: Root, host: HTMLDivElement;
const onModel = vi.fn();
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  onModel.mockClear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });
const button = (text: string, within: ParentNode = document) => Array.from(within.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.startsWith(text))!;
const click = async (b: HTMLButtonElement) => { await act(async () => b.click()); };
async function show(model: string) {
  const { CursorModel } = await import('../SettingsView');
  await act(async () => root.render(<CursorModel name="Work" groups={GROUPS} model={model} onModel={onModel} />));
}
const fast = () => host.querySelector<HTMLButtonElement>('[role="switch"]');

it('AGENTS-11: the menu lists each model once, the chosen effort after it', async () => {
  await show('grok-4.7-low-fast');
  expect(host.querySelector('.set-picker')?.textContent).toBe('Grok 4.7Low');
  await click(host.querySelector<HTMLButtonElement>('.set-picker')!);
  const models = document.querySelector('[aria-label="Models"]')!;
  expect(Array.from(models.querySelectorAll('button'), (b) => b.textContent)).toEqual(['Auto', 'Grok 4.7', 'Codex 5.3']);
  expect(fast()?.getAttribute('aria-checked')).toBe('true');
});

it('AGENTS-11: Effort offers the model\'s own efforts, its bare id as Default, and keeps Fast where it can', async () => {
  await show('grok-4.7-low-fast');
  await click(host.querySelector<HTMLButtonElement>('.set-picker')!);
  await click(button('Effort'));
  const efforts = document.querySelector('[aria-label="Effort"]')!;
  expect(Array.from(efforts.querySelectorAll('button'), (b) => b.textContent)).toEqual(['Low', 'Medium', 'High']);
  await click(button('Medium', efforts));
  expect(onModel).toHaveBeenLastCalledWith('grok-4.7-medium-fast');
  // High has no Fast: the plain one
  await show('grok-4.7-low-fast');
  await click(host.querySelector<HTMLButtonElement>('.set-picker')!);
  await click(button('Effort'));
  await click(button('High', document.querySelector('[aria-label="Effort"]')!));
  expect(onModel).toHaveBeenLastCalledWith('grok-4.7-high');

  await show('gpt-5.3-codex');
  await click(host.querySelector<HTMLButtonElement>('.set-picker')!);
  await click(button('Effort'));
  expect(Array.from(document.querySelectorAll('[aria-label="Effort"] button'), (b) => b.textContent)).toEqual(['Low', 'Default', 'Extra high']);
});

it('AGENTS-11: Fast shows only where the effort has both, and switches the id', async () => {
  await show('grok-4.7-high');
  expect(fast()).toBeNull();
  await show('auto');
  expect(fast()).toBeNull();
  expect(host.querySelector('.set-picker')?.textContent).toBe('Auto');
  await show('grok-4.7-medium');
  await click(fast()!);
  expect(onModel).toHaveBeenLastCalledWith('grok-4.7-medium-fast');
});

it('AGENTS-11: another model keeps the effort and Fast it has, else starts on its default', async () => {
  const { cursorCompose } = await import('../SettingsView');
  const [, grok, codex] = GROUPS;
  expect(cursorCompose(codex, 'low', false)).toBe('gpt-5.3-codex-low');
  expect(cursorCompose(codex, 'medium', true)).toBe('gpt-5.3-codex-fast');
  expect(cursorCompose(grok, undefined, false)).toBe('grok-4.7-medium');
  expect(cursorCompose(grok, 'xhigh', false)).toBe('grok-4.7-medium');
  await show('gpt-5.3-codex-low');
  await click(host.querySelector<HTMLButtonElement>('.set-picker')!);
  await click(button('Grok 4.7', document.querySelector('[aria-label="Models"]')!));
  expect(onModel).toHaveBeenLastCalledWith('grok-4.7-low');
});

it('AGENTS-11: a saved id Cursor no longer lists shows as itself, and a pick moves it to a listed one', async () => {
  await show('gpt-6.1-sol');
  expect(host.querySelector('.set-picker')?.textContent).toBe('gpt-6.1-sol');
  expect(fast()).toBeNull();
  await click(host.querySelector<HTMLButtonElement>('.set-picker')!);
  await click(button('Auto', document.querySelector('[aria-label="Models"]')!));
  expect(onModel).toHaveBeenLastCalledWith('auto');
});
