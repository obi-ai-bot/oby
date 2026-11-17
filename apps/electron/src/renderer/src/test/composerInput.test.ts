import { describe, expect, it, vi } from 'vitest';
vi.mock('../live', () => ({ conversationFiles: async () => [] }));
vi.mock('../lean', () => ({ onObid: true }));
vi.mock('../mock', () => ({ FILES: [] }));
import { COMMANDS_MOST, commandWords, dropped, fileWords, isFileDrag, noCommands, searchCommands, slashOf } from '../composerInput';

// What the message box takes besides typing (docs/contracts/composer.md, COMPOSER-13, 14 and 16)
const file = (name: string) => ({ name }) as File;

describe('the box’s input', () => {
  it('composer_13_folders_are_told_apart', () => {
    expect(isFileDrag({ types: ['Files'] })).toBe(true);
    expect(isFileDrag({ types: ['text/plain'] })).toBe(false);
    const a = file('a.png'), b = file('src');
    const items = [{ kind: 'file', getAsFile: () => a, webkitGetAsEntry: () => ({ isDirectory: false }) }, { kind: 'file', getAsFile: () => b, webkitGetAsEntry: () => ({ isDirectory: true }) }, { kind: 'string', getAsFile: () => null }];
    expect(dropped({ types: ['Files'], files: [a, b], items })).toEqual({ files: [a], folders: 1 });
    // no items to tell them by: every file
    expect(dropped({ types: ['Files'], files: [a, b] })).toEqual({ files: [a, b], folders: 0 });
  });

  it('composer_14_a_file_is_named_from_its_folder', () => {
    const f = { folder: '/Users/me/oby', path: 'src/app.ts' };
    expect(fileWords(f, 1)).toBe('src/app.ts');
    expect(fileWords(f, 2)).toBe('oby/src/app.ts');
  });

  it('composer_16_commands_rank_as_t3s_do', () => {
    expect(slashOf('/')).toBe('');
    expect(slashOf('/rev')).toBe('rev');
    expect(slashOf('/review this')).toBeNull();
    expect(slashOf('look /here')).toBeNull();
    const c = (name: string, skill = false) => ({ name, skill });
    const all = [c('security-review'), c('review-pr', true), c('preview'), c('review'), c('vercel:review', true), c('compact')];
    // nothing typed: every one, in Claude Code's order
    expect(searchCommands(all, '')).toEqual(all);
    // it, then starting with it, then after a - _ or :, then holding it; by name within each
    expect(searchCommands(all, 'review').map((x) => x.name)).toEqual(['review', 'review-pr', 'security-review', 'vercel:review', 'preview']);
    expect(searchCommands(all, '/REV').map((x) => x.name)).toEqual(['review', 'review-pr', 'security-review', 'vercel:review', 'preview']);
    expect(searchCommands(all, 'zzz')).toEqual([]);
    const many = Array.from({ length: 80 }, (_, i) => c(`cmd-${i}`));
    expect(searchCommands(many, '')).toHaveLength(COMMANDS_MOST);
    expect(commandWords(c('simplify', true))).toBe('/simplify ');
    expect(noCommands('Codex')).toBe('Codex’s own commands can’t be listed here yet. Say what you want instead.');
  });
});
