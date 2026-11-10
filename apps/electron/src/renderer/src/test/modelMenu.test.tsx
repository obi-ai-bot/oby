// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

// Settings › Agents' Model menu with a long list, as Cursor's (docs/contracts/agents.md, AGENTS-10):
// inside the window wherever its button is, scrolling, its model in view, Effort still beside it.
const MODELS = Array.from({ length: 40 }, (_, i) => [`m-${i}`, `Model ${i}`] as const);
const EFFORTS = [['low', 'Low'], ['high', 'High']] as const;
const WINDOW = 420;

let root: Root, host: HTMLDivElement;
const onModel = vi.fn(), onEffort = vi.fn();
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('innerHeight', WINDOW);
  vi.stubGlobal('innerWidth', 800);
  onModel.mockClear(); onEffort.mockClear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
const button = (text: string, within: ParentNode = document) => Array.from(within.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.startsWith(text))!;
const click = async (b: HTMLButtonElement) => { await act(async () => b.click()); };

// The picker with its button from top to bottom in the window, opened; the menu
async function open(top: number, bottom: number) {
  const { ModelPicker } = await import('../SettingsView');
  await act(async () => root.render(<ModelPicker label="Cursor model and effort" models={MODELS} model="m-30" onModel={onModel} efforts={EFFORTS} effort="low" onEffort={onEffort} />));
  const picker = host.querySelector('button')!;
  picker.getBoundingClientRect = () => new DOMRect(500, top, 240, bottom - top);
  await click(picker);
  return document.querySelector<HTMLElement>('.row-menu')!;
}
// Where the menu's box is, from its style (happy-dom lays nothing out)
const box = (menu: HTMLElement) => {
  const max = Number.parseFloat(menu.style.maxHeight);
  const top = menu.style.top ? Number.parseFloat(menu.style.top) : WINDOW - Number.parseFloat(menu.style.bottom) - max;
  return { top, bottom: top + max };
};

it('AGENTS-10: near the window\'s bottom, the long Model menu opens above and fits, the Model label first', async () => {
  const menu = await open(360, 390);
  expect(menu.className).toContain('is-up');
  const { top, bottom } = box(menu);
  expect(top).toBeGreaterThanOrEqual(0);
  expect(bottom).toBeLessThanOrEqual(360);
  expect(menu.firstElementChild?.textContent).toBe('Model');
  const models = menu.querySelector('.menu-scroll')!;
  expect(models.querySelectorAll('button')).toHaveLength(40);
});

it('AGENTS-10: near the window\'s top, it opens below, capped to the room there', async () => {
  const menu = await open(20, 50);
  expect(menu.className).not.toContain('is-up');
  const { top, bottom } = box(menu);
  expect(top).toBeGreaterThanOrEqual(50);
  expect(bottom).toBeLessThanOrEqual(WINDOW);
});

it('AGENTS-10: a short list still opens below its button, as before', async () => {
  const { menuPlace } = await import('../Menu');
  expect(menuPlace(new DOMRect(0, 300, 100, 30), 60, WINDOW)).toEqual({ up: false, maxHeight: 420 - 330 - 12, scroll: false });
  expect(menuPlace(new DOMRect(0, 300, 100, 30), 200, WINDOW)).toMatchObject({ up: true, scroll: false });
});

it('AGENTS-10: the chosen model is scrolled into view in the list, which scrolls instead of the menu', async () => {
  const seen: Element[] = [];
  vi.spyOn(HTMLElement.prototype, 'scrollIntoView').mockImplementation(function (this: HTMLElement) { seen.push(this); });
  const menu = await open(360, 390);
  expect(seen).toEqual([button('Model 30', menu)]);
  expect(button('Model 30', menu).getAttribute('aria-checked')).toBe('true');
  // the whole menu doesn't scroll: that would clip the Effort submenu beside it
  expect(menu.className).not.toContain('is-scroll');
  expect(button('Model 30', menu).closest('.menu-scroll')).not.toBeNull();
  expect(button('Effort', menu).closest('.menu-scroll')).toBeNull();
});

it('AGENTS-10: Effort still opens its submenu beside it, inside the window, and picks', async () => {
  const menu = await open(360, 390);
  const effort = button('Effort', menu);
  effort.getBoundingClientRect = () => new DOMRect(500, 330, 200, 28);
  await click(effort);
  const efforts = document.querySelector<HTMLElement>('[aria-label="Effort"]')!;
  expect(efforts.closest('.menu-scroll')).toBeNull();
  expect(330 + Number.parseFloat(efforts.style.top) + Number.parseFloat(efforts.style.maxHeight)).toBeLessThanOrEqual(WINDOW);
  await click(button('High', efforts));
  expect(onEffort).toHaveBeenCalledWith('high');
  expect(document.querySelector('.row-menu')).toBeNull();
});

it('AGENTS-10: picking a model from the scrolled list chooses it', async () => {
  const menu = await open(360, 390);
  await click(button('Model 39', menu));
  expect(onModel).toHaveBeenCalledWith('m-39');
});
