// The finishing actions' words (docs/contracts/folders.md, WORKSPACE-4 and 5), T3 Code's
// (`GitActionsControl.logic.ts`): the button in the conversation and main's dialog say the same.
// Placeholders until Sangam words them.
import type { FinishAction } from './obid-wire';

export const FINISH_WORDS: Record<FinishAction, string> = {
  commit: 'Commit',
  push: 'Push',
  pullRequest: 'Create PR',
  publishRepository: 'Publish repository',
  initializeGit: 'Initialize Git',
};

// Where a new conversation works: one checkbox, as Claude Code offers a worktree (the owner's choice,
// 2026-11-23); unchecked, this folder
export const WORKTREE_WORDS = 'New worktree';
