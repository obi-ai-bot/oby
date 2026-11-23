// The owner's conversation loop, one scenario each (human-drive.mjs runs them in this order). A
// scenario is { id, title, contract, needs?, run(t) }: `t.say` types and sends, `t.reply` waits for
// Oby's answer, `t.click` clicks a button by its name, `t.see` waits until the window shows
// something, `t.shot` keeps a screenshot, `t.note` writes a line in the report, `t.state` carries
// what a later scenario needs (a scenario whose `needs` didn't pass is skipped). Waits are on what
// the window shows (obid's job files only say when a task ended, as evidence). Tasks stay tiny: one
// line in one file of the scratch repository, a file of its own for each scenario that makes a
// pull request, so the run never conflicts with itself.
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const MIN = 60_000;
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const ASKS = 'section[aria-label="Oby is asking"]';
const REPLY = '.msg.is-oby .msg-bubble';
const ended = (j) => ['done', 'failed', 'interrupted', 'stopped'].includes(j?.status);
const stamp = new Date().toISOString().slice(0, 16);
// a task name no earlier run used: the side panel finds tasks by name
const taskName = `Oby drive ${stamp.slice(11)}`;

// the empty chat, Home's stage, where words start a new conversation (docs/contracts/chat-first.md)
async function home(t) {
  if (await t.page.getByRole('button', { name: 'Show sidebar' }).isVisible()) await t.click('Show sidebar');
  const oby = t.page.locator('.rail-button[aria-label="Chats"]');
  if ((await oby.getAttribute('aria-pressed')) !== 'true') await oby.click();
  // its accessible name ends with its shortcut, “New conversation ⌘N” (#2789)
  await t.click(/^New conversation/);
  await t.see('What do you want me to work on?', { label: 'the empty chat' });
}

// the task the words just said started: its job (obid's evidence, which says which one: only ever
// one in this run's repository, never the owner's other work) and its card, as the conversation
// shows it. Its kind must be one of `kinds`.
async function newTask(t, since, kinds = ['build', 'fix']) {
  const deadline = Date.now() + 5 * MIN;
  let job;
  while (!(job = t.newestJob(since))) {
    if (Date.now() > deadline) throw new Error(`no task started in ${t.repo} in 5 minutes`);
    await sleep(500);
  }
  if (job.repo.toLowerCase() !== t.repo || !kinds.includes(job.kind)) throw new Error(`the task Oby started is a ${job.kind} in ${job.repo}, not a ${kinds.join(' or ')} in ${t.repo}`);
  // this run's, for the cleanup at its end
  (t.state.jobs ??= []).push(job.id);
  await t.see(job.title, { ms: 2 * MIN, label: `its card, “${job.title}”` });
  await t.shot('task-card');
  return { title: job.title, job };
}

// Home's cards for the tasks this run started, answered in one click each until `done`: a card counts
// only when its title is exactly one of theirs, so the owner's other cards (their real GitHub
// reviews, their own tasks) are never touched; `mine` lets in a card whose title names this run's
// pull request (a Confirm Oby makes for it)
async function answerCards(t, { done, ms = 25 * MIN, mine = () => false }) {
  const ours = new Set(t.state.titles ?? []);
  const asks = t.page.locator(ASKS);
  const seen = (t.state.unanswerable ??= new Set());
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (await done()) return;
    // once round the stack: answer the first card about this work the driver can answer
    for (let i = 0; (await asks.count()) && i < 12; i++) {
      const top = asks.locator('article.ask-card.is-up');
      const text = (await top.count()) ? await top.innerText() : '';
      const title = (await top.count()) ? (await top.locator('h2.tc-title').innerText()).trim() : '';
      if (ours.has(title) || mine(title)) {
        // a check for the scratch repository: none (its tests need the network); anything else: yes,
        // and the question's answer, "asked"
        const answer = top.getByRole('button', { name: /check before publishing/.test(text) ? /^(No check|Cancel)$/ : /^(Allow|Approve|Merge|Confirm|Yes|Start|asked)/ }).first();
        if (await answer.count()) {
          // a card settled under the pointer (its pull request merged meanwhile) is progress, not a failure
          if (await done()) return;
          t.state.cards = (t.state.cards ?? 0) + 1;
          await t.shot(`card-${t.state.cards}`);
          const name = await answer.innerText();
          t.note(`card: ${text.replace(/\n+/g, ' · ')} → ${name}`);
          if (!(await answer.click({ timeout: 5000 }).then(() => true, () => false))) { t.note(`“${title}” left Home before its ${name} was clicked`); break; }
          t.step(`answered “${name}” on Home`);
          await sleep(2000);
          break;
        }
        // one the owner can only read or put off (a question with "Done" alone): noted once, passed
        const key = text.replace(/\d+\/\d+/, '');
        if (!seen.has(key)) { seen.add(key); t.note(`a card with no answer but Done or Later: ${text.replace(/\n+/g, ' · ')}`); await t.shot('card-unanswerable'); }
      }
      // the card on top: another may still be leaving, with its own arrows
      const next = top.getByRole('button', { name: 'Next' });
      if (!(await next.count()) || !(await next.click({ timeout: 5000 }).then(() => true, () => false))) break;
      await sleep(300);
    }
    await sleep(2000);
  }
  throw new Error(`not done in ${ms / MIN} minutes`);
}

// a task as the owner opens it: its card in the conversation that started it, what it did under it
// (LIVETASK-28). Its conversation is the latest few: they go down the list until its card shows
async function openTask(t, title) {
  await home(t);
  const card = t.page.locator('.msg-task', { hasText: title }).first(), rows = t.page.locator('.convo-row');
  for (let i = 0; i < Math.min(20, await rows.count()) && !(await card.isVisible()); i++) {
    await rows.nth(i).click();
    await sleep(500);
  }
  await t.see('Reply to Oby…', { label: 'its conversation' });
}

// a task's nested row's menu, View details: its conversation opens at its card (LIVETASK-28), the
// conversation's row lit. (A click on the row itself opens the task's own thread, LIVETASK-22: ownThread.)
// Whether the card is in view, above the decision drawer, is noted, not failed on (#2786)
async function openAtTask(t, kid, title) {
  await kid.click({ button: 'right' });
  await t.page.getByRole('menuitem', { name: 'View details', exact: true }).click();
  const card = t.page.locator('.msg-task', { has: t.page.locator('.msg-task-title', { hasText: title }) }).last();
  await card.waitFor({ state: 'attached', timeout: 10_000 });
  await t.page.locator('.convo-row.is-active').first().waitFor({ timeout: 10_000 });
  await sleep(800);
  const inView = await card.evaluate((el) => {
    const r = el.getBoundingClientRect();
    const bar = [...document.querySelectorAll('.decision-bar')].filter((e) => e.checkVisibility()).pop();
    return r.top >= 0 && r.bottom <= (bar ? bar.getBoundingClientRect().top : innerHeight);
  });
  t.note(`its conversation opened; its card is ${inView ? 'in view' : 'NOT in view (LIVETASK-28, #2786)'}`);
  return card;
}

// a task's nested row clicked: its own thread opens over the conversation that started it, its box
// telling its agent, and the row lit while it shows (LIVETASK-22)
async function ownThread(t, kid, task) {
  await kid.click();
  t.step(`clicked its row, “${task.title}”`);
  const top = t.page.locator('.chat-layer:not(.is-leaving):not(.is-covered)').last();
  const box = top.locator(`[aria-label="Tell the agent about ${task.title}"]`);
  const lit = t.page.locator('.convo-kid.is-active', { hasText: task.title });
  const opened = await until(t, async () => (await top.locator(`[data-task="${task.job.id}"]`).count()) && (await box.count()) && (await lit.count()), 10_000).then(() => true, () => false);
  await t.shot('own-thread');
  if (!opened) {
    throw new Error(`its row didn't open its own thread (LIVETASK-22): its box telling its agent ${(await box.count()) ? 'shows' : 'is missing'}, its row ${(await lit.count()) ? 'lit' : 'not lit'}, the conversation's row ${(await t.page.locator('.convo-row.is-active').count()) ? 'lit' : 'not lit'}`);
  }
  t.note('its row opened its own thread, its box telling its agent, the row lit');
}

// this run's repository and pull request only, never another of the owner's: `<repo>#<n>`
const prRef = (t, url) => `${t.repo}#${url.match(/\/pull\/(\d+)$/)?.[1]}`;
const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

// the Confirm Oby's answer made for `url` (by default to merge it: Merge <repo>#<n> into …), under
// that answer: its exact title tracked as this run's, then Confirm, the owner's one click
async function confirmMerge(t, url, head = new RegExp(`^Merge ${escape(prRef(t, url))} into `, 'i')) {
  const card = t.page.locator('.msg-ask').filter({ has: t.page.locator('.tc-ask-head', { hasText: head }) }).last();
  await card.waitFor({ timeout: MIN });
  const title = (await card.locator('.tc-ask-head').innerText()).trim();
  track(t, { title });
  t.state.cards = (t.state.cards ?? 0) + 1;
  t.note(`card: ${title} → Confirm, in the conversation`);
  // (a click in a card's first half second answers nothing: AskBlock)
  await sleep(700);
  await card.getByRole('button', { name: 'Confirm', exact: true }).click();
  t.step(`answered “Confirm” on “${title}” in the conversation`);
  await t.shot('confirmed');
}

// a conflict, as the owner settles it (MERGING-3: "Ask Oby to rebase it onto <base>"): in words on
// the task's page, then Oby's rebase (a follow-up task, or a round of the task's own: either gives
// the pull request a new head), its Confirm if one comes, and the merge. What Oby said and did goes
// in the report; no rebase is a finding.
async function rebaseThenMerge(t, url, merged) {
  const pr = () => JSON.parse(t.gh('pr', 'view', url, '--json', 'headRefOid,baseRefName,mergeable'));
  const { headRefOid: head, baseRefName: base } = pr();
  // its number whole: #15 never matches inside #151
  const ref = new RegExp(`${escape(prRef(t, url))}(?!\\d)`, 'i'), mine = (title) => ref.test(title);
  await openTask(t, t.state.task.title);
  const since = Date.now();
  await t.say(`Rebase it onto ${base}, then merge it.`, 'Reply to Oby…');
  t.note(`Oby, told to rebase: ${(await t.reply({ ms: 5 * MIN })).replace(/\n+/g, ' ')}`);
  await t.shot('told-to-rebase');
  // a Confirm for this pull request under its answer (a rebase's or a merge's), if Oby made one
  await confirmMerge(t, url, ref).catch(() => t.note('no Confirm under its answer'));
  await home(t);
  // its rebase: a task it starts for it is this run's (its cards answered, its branch cleaned up)
  const rebased = () => {
    const job = t.newestJob(since);
    if (job && !t.state.jobs.includes(job.id)) {
      t.state.jobs.push(job.id);
      track(t, job);
      t.note(`it started a task for the rebase: ${job.id}, a ${job.kind}, “${job.title}”`);
    }
    return merged() || pr().headRefOid !== head;
  };
  await answerCards(t, { done: rebased, ms: 20 * MIN, mine }).catch(() => {});
  const now = pr();
  if (now.headRefOid === head && !merged()) throw new Error(`Oby didn't rebase it onto ${base} in 20 minutes: its head is still ${head.slice(0, 7)}, ${now.mergeable}`);
  t.note(`rebased: its head ${head.slice(0, 7)} → ${now.headRefOid.slice(0, 7)}, ${now.mergeable}`);
  await t.shot('rebased');
  await answerCards(t, { done: merged, ms: 15 * MIN, mine });
}

// a task whose cards this run answers: never one named as a card the owner had on Home already
const track = (t, task) => {
  if (t.state.homeTitles?.has(task.title)) throw new Error(`the task “${task.title}” has the name of a card the owner had on Home: its cards can't be told apart, so none is answered`);
  (t.state.titles ??= []).push(task.title);
  return task;
};

// the agents' names as the window says them (browser/view.ts, AGENT_NAME)
const AGENT = { claude: 'Claude Code', codex: 'Codex', cursor: 'Cursor' };
// the conversation that's open (the top chat layer), never one leaving under it
const openChat = (t) => t.page.locator('.chat-layer:not(.is-leaving)').last();
// a public page whose heading is known and doesn't change
const PAGE = 'https://example.com';
const HEADING = /Example Domain/i;

// what the task asks in the bar over the message box of its conversation (a new conversation holds
// only this run's task), answered yes with the owner's click; false when it asks nothing
async function answerBar(t) {
  const bar = t.page.locator('.decision-bar').filter({ visible: true }).last();
  if (!(await bar.count())) return false;
  const yes = bar.getByRole('button', { name: /^(Yes|yes|Allow|Allow once|Approve|Confirm|Start|No check)/ }).first();
  if (!(await yes.count())) return false;
  await sleep(700); // (a click in a card's first half second answers nothing: AskBlock)
  t.note(`the task asked: ${(await bar.innerText()).replace(/\n+/g, ' · ')} → ${await yes.innerText()}`);
  await t.shot('asked');
  await yes.click();
  t.step('answered the bar');
  return true;
}

// the job the words just said started in this run's repository, Oby asked once to start it if it
// asks first (as the owner would say yes); null when none by `ms`
async function startedJob(t, since, { ms = 5 * MIN, yes = 'Yes, start it.' } = {}) {
  const deadline = Date.now() + ms;
  let job, nudged = false;
  while (!(job = t.newestJob(since)) && Date.now() < deadline) {
    const busy = await t.page.locator('.msg.is-streaming, .msg-step').count();
    const last = (await openChat(t).locator(REPLY).count()) ? (await openChat(t).locator(REPLY).last().innerText()).trim() : '';
    if (!nudged && !busy && /\?\s*$/.test(last)) {
      t.note(`Oby asked first: ${last.replace(/\n+/g, ' ')}`);
      await t.say(yes);
      nudged = true;
    }
    await sleep(1000);
  }
  return job ?? null;
}

// a job as obid keeps it, or null once its file is gone (deleted)
const jobOrNone = (t, id) => { try { return t.job(id); } catch { return null; } };

// main's artifact preview (a view of main's own over the pane), asked through main: evidence only
const inHost = (t, js) => t.app.evaluate(async ({ webContents }, js) => {
  const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
  return wc ? Promise.race([wc.executeJavaScript(js), new Promise((done) => setTimeout(() => done(null), 5_000))]) : null;
}, js);
const inFrame = (t, js) => t.app.evaluate(async ({ webContents }, js) => {
  const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
  const frame = wc?.mainFrame.framesInSubtree.find((f) => /^http:\/\/[a-z0-9]+-preview\.localhost:\d+$/.test(f.origin) && !f.url.includes('/__csb_relay/'));
  if (!frame) return null;
  return Promise.race([frame.executeJavaScript(js), new Promise((done) => setTimeout(() => done(null), 5_000))]);
}, js);

export default [
  {
    id: 'open',
    title: 'Oby opens on Home',
    run: async (t) => {
      await home(t);
      await t.shot('home');
    },
  },
  {
    id: 'research',
    title: 'Ask Oby about a repository and read its answer',
    contract: 'task-conversations.md',
    run: async (t) => {
      await t.say(`In ${t.repo}, what is this repository for? One sentence.`);
      let answer = await t.reply({ ms: 5 * MIN });
      t.note(`first answer: ${answer.replace(/\n+/g, ' ')}`);
      await t.shot('first-answer');
      // Oby may offer to look in a task first: yes, as the owner would, then wait for its report
      if (/start (a|the)\b.*\b(job|task)|look through|want me to|if you want/i.test(answer)) {
        await t.say('Yes, look and tell me.');
        answer = await t.reply({ ms: 5 * MIN });
        for (let i = 0; i < 5 && /hasn.t finished|once it.s done|I.ll tell you|I started/i.test(answer); i++) answer = await t.reply({ ms: 20 * MIN });
        t.note(`its report: ${answer.replace(/\n+/g, ' ')}`);
        await t.shot('answer');
      }
      if (/couldn.t|can.t say|unable|hasn.t finished|I.ll tell you/i.test(answer)) throw new Error('Oby didn’t say what the repository is for');
    },
  },
  {
    id: 'issue-task',
    title: 'Point Oby at an issue: it reads it and its task does it',
    contract: 'tasks.md',
    run: async (t) => {
      const url = t.gh('issue', 'create', '-R', t.repo, '--title', `Oby drive ${stamp}: one line in docs/oby-drive-issue.md`,
        '--body', `Add the line \`- ${stamp}: driven by Oby (issue)\` at the end of docs/oby-drive-issue.md (make the file if it isn't there). Change nothing else. Opened by apps/electron/scripts/human-drive.mjs.`);
      (t.state.issues ??= []).push(url);
      t.note(`issue: ${url}`);
      await home(t);
      const since = Date.now();
      await t.say(`Do ${url}`);
      const task = track(t, await newTask(t, since));
      if (task.job.issue !== Number(url.split('/').pop())) throw new Error(`the task Oby started isn't for ${url}`);
      await home(t);
      await answerCards(t, { done: () => ended(t.job(task.job?.id)) });
      const job = t.job(task.job.id);
      t.note(`evidence: ${job.status}, ${job.commits} commits, ${job.published ?? job.publishError ?? 'not published'}; its summary: ${(job.summary ?? '').split('\n')[0]}`);
      await t.shot('ended');
      if (!job.commits && !job.published) throw new Error('the task ended with no change: its agent never saw the issue’s text');
    },
  },
  {
    id: 'task',
    title: 'Ask for a change in words: a task starts',
    contract: 'tasks.md',
    run: async (t) => {
      await home(t);
      const since = Date.now();
      await t.say(`In ${t.repo}: add the line "- ${stamp}: driven by Oby" at the end of docs/oby-drive.md (make the file if it isn't there). Change nothing else. Call the task "${taskName}".`);
      t.state.task = track(t, await newTask(t, since));
    },
  },
  {
    id: 'steer',
    title: 'Steer the task while it works',
    contract: 'task-conversations.md (TASKCONV-3)',
    needs: ['task'],
    run: async (t) => {
      await t.say('For that task: also end the line with a period.');
      t.note(`Oby: ${(await t.reply({ ms: 3 * MIN })).replace(/\n+/g, ' ')}`);
      await t.shot('steered');
    },
  },
  {
    id: 'to-pr',
    title: 'Watch it to its pull request, answering its cards on Home',
    contract: 'publishing.md, merging.md',
    needs: ['task'],
    run: async (t) => {
      const id = t.state.task.job?.id;
      await home(t);
      await answerCards(t, { done: () => { const j = t.job(id); return j?.published || ended(j); } });
      await openTask(t, t.state.task.title);
      // its pull request: the link on its card's head (a done card shows its head alone, LIVETASK-29;
      // nothing there says whether it's open or merging, #2785)
      const job = t.job(id);
      const n = job.published?.match(/\/pull\/(\d+)$/)?.[1];
      if (!n) throw new Error(`the task published no pull request: ${job.publishError ?? job.status}`);
      const card = t.page.locator('.msg-task', { has: t.page.locator('.msg-task-title', { hasText: t.state.task.title }) }).last();
      const link = card.locator('.tc-result a', { hasText: new RegExp(`#${n}$`) });
      await until(t, () => link.isVisible(), 5 * MIN).catch(() => { throw new Error(`its card doesn't show its pull request #${n}`); });
      await t.shot('pull-request');
      t.state.job = job;
      t.note(`its card: ${(await card.innerText()).replace(/\n+/g, ' · ')}; evidence: ${job.agent}/${job.account}, mode ${job.mode}, vetted ${job.vetted}, ${job.published}`);
    },
  },
  {
    id: 'follow-up',
    title: 'A follow-up in its conversation: same kind, agent, account and repository',
    contract: 'task-conversations.md (TASKCONV-3)',
    needs: ['to-pr'],
    run: async (t) => {
      const since = Date.now();
      await t.say('Add a second line "- follow-up" under it.', 'Reply to Oby…');
      // its card in the task's conversation, which the app opens (Oby names it as it likes)
      const first = t.state.job, { job } = await newTask(t, since, [first.kind]);
      if (!job) throw new Error('no follow-up job');
      track(t, job);
      t.note(`follow-up ${job.id} of ${job.followUpOf}: ${job.kind} ${job.agent}/${job.account} ${job.repo}, vetted ${job.vetted}`);
      const differs = ['kind', 'agent', 'account', 'repo'].filter((k) => job[k] !== first[k]);
      if (differs.length) throw new Error(`the follow-up differs in ${differs.join(', ')}`);
      await home(t);
      await answerCards(t, { done: () => ended(t.job(job.id)) });
      t.state.followUp = t.job(job.id);
      t.note(`it ended ${t.state.followUp.status}, ${t.state.followUp.published ?? 'nothing published'}`);
    },
  },
  {
    id: 'merge',
    title: 'Act on the review or merge card, and watch to the merge',
    contract: 'merging.md',
    needs: ['to-pr'],
    run: async (t) => {
      const url = t.state.job.published;
      const merged = () => JSON.parse(t.gh('pr', 'view', url, '--json', 'state')).state === 'MERGED';
      // GitHub's word, or the card's (MERGING-3) for this task or its follow-up
      const conflicts = () => JSON.parse(t.gh('pr', 'view', url, '--json', 'mergeable')).mergeable === 'CONFLICTING'
        || [...(t.state.unanswerable ?? [])].some((k) => k.includes(t.state.task.title) && /conflicts with/.test(k));
      const before = t.state.cards ?? 0;
      await home(t);
      // the card Oby gives the owner: a merge, a review's decision, or nothing while it merges itself
      await answerCards(t, { done: merged, ms: 10 * MIN }).catch((e) => t.note(`the cards didn't merge it: ${e.message}`));
      if (!merged() && !conflicts()) {
        // no card merged it (a question with only "Done" can't): the owner tells Oby, as they would
        t.note(`not merged after the cards (${(t.state.cards ?? 0) - before} answered): the owner says so in words`);
        await openTask(t, t.state.task.title);
        await t.say('Merge it as it is.', 'Reply to Oby…');
        t.note(`Oby: ${(await t.reply({ ms: 5 * MIN })).replace(/\n+/g, ' ')}`);
        await t.shot('told-to-merge');
        // words read with untrusted text make a Confirm: under Oby's answer (and on Home), answered
        // here by its exact title, tracked like the run's other cards, before the merge is waited for
        await confirmMerge(t, url).catch((e) => t.note(`no Confirm to answer in the conversation: ${e.message}`));
        const deadline = Date.now() + 3 * MIN;
        while (!merged() && Date.now() < deadline) await sleep(5000);
        await home(t);
        await answerCards(t, { done: merged, ms: 15 * MIN }).catch((e) => t.note(`not merged after the owner's word: ${e.message}`));
      }
      // it conflicts with its base (or GitHub found so when asked to merge): the owner asks for a rebase
      if (!merged() && conflicts()) await rebaseThenMerge(t, url, merged);
      if (!merged()) throw new Error('not merged');
      await openTask(t, t.state.task.title);
      await t.see(/Merged[^\n]*/, { ms: 3 * MIN, label: 'merged, in its conversation' });
      await t.shot('merged');
    },
  },
  {
    id: 'cards',
    title: 'A permission and a question, each answered in one click on Home',
    contract: 'permissions.md',
    run: async (t) => {
      await home(t);
      const before = t.state.cards ?? 0, since = Date.now();
      await t.say(`Start a task now in ${t.repo}, in Ask mode: its agent asks me (not you) whether the word should be "ask" or "asked", then makes docs/oby-ask.md with that one word.`);
      const task = track(t, await newTask(t, since));
      await home(t);
      await answerCards(t, { done: () => ended(t.job(task.job?.id)) });
      t.note(`cards answered: ${(t.state.cards ?? 0) - before}`);
      if ((t.state.cards ?? 0) === before) throw new Error('no permission or question card came');
    },
  },
  {
    id: 'first-answer',
    title: 'The time to the first answer in a new conversation',
    contract: 'conversation-agent.md (CONVERSATION-AGENT-24, 25)',
    run: async (t) => {
      // twice cold (a new conversation each), then once warm (a second message in the last one)
      const times = [];
      for (const [i, cold] of [[1, true], [2, true], [3, false]]) {
        if (cold) await home(t);
        // counted in the conversation that's open (the top chat layer), never in one leaving under it
        const open = () => t.page.locator('.chat-layer:not(.is-leaving)').last();
        const SIGN = '.msg.is-oby, .msg.is-streaming, .msg-step';
        const replies = async () => (await open().count()) ? open().locator(REPLY).count() : 0;
        const before = await replies(), signs = (await open().count()) ? await open().locator(SIGN).count() : 0;
        const t0 = Date.now();
        await t.say(i === 3 ? 'And one more word?' : `Say hello in ${['three', 'four'][i - 1]} words.`);
        // the first sign Oby is on it (its streaming bubble, a step, or a reply) and the first words of it
        await until(t, async () => (await open().locator(SIGN).count()) > (cold ? 0 : signs), 60_000);
        const sign = Date.now() - t0;
        await until(t, async () => (await replies()) > (cold ? 0 : before) && (await open().locator(REPLY).last().innerText()).trim(), 120_000);
        const words = Date.now() - t0;
        const text = await t.reply({ ms: 2 * MIN });
        const all = Date.now() - t0;
        times.push({ cold, sign, words, all });
        t.note(`${cold ? 'cold' : 'warm'}: a first sign in ${(sign / 1000).toFixed(1)} s, first words in ${(words / 1000).toFixed(1)} s, the whole answer in ${(all / 1000).toFixed(1)} s (“${text.slice(0, 60).replace(/\n/g, ' ')}”)`);
        if (i === 1) await t.shot('first-answer');
      }
      t.state.firstAnswer = times;
    },
  },
  {
    id: 'agent-menu',
    title: 'The agent menu and its model search (nothing picked)',
    contract: 'conversation-agent.md (CONVERSATION-AGENT-27 to 31)',
    run: async (t) => {
      await home(t);
      const pill = t.page.locator('button.talk-pick').filter({ visible: true }).last();
      await pill.waitFor({ timeout: 20_000 });
      t.note(`the pill reads: ${await pill.getAttribute('aria-label')}`);
      const t0 = Date.now();
      await pill.click();
      await t.page.locator('.talk-menu').waitFor({ timeout: 5000 });
      t.note(`menu open in ${Date.now() - t0} ms`);
      await t.shot('menu');
      const rows = t.page.locator('.talk-menu button.talk-choice[aria-haspopup="menu"]:not([disabled])');
      const n = await rows.count();
      t.note(`accounts listed: ${n}: ${(await rows.allInnerTexts()).map((s) => s.replace(/\n+/g, ' · ')).join(' | ')}`);
      let searched = false;
      for (let i = 0; i < n && !searched; i++) {
        await rows.nth(i).click();
        const list = t.page.locator('.model-menu');
        await list.waitFor({ timeout: 5000 });
        const box = await list.boundingBox();
        const models = await list.locator('.model-row').allInnerTexts();
        t.note(`${(await rows.nth(i).locator('.talk-choice-name').innerText())}: ${models.length} models (${models.slice(0, 8).join(', ')}${models.length > 8 ? '…' : ''}), list at ${Math.round(box?.x ?? 0)},${Math.round(box?.y ?? 0)} ${Math.round(box?.width ?? 0)}×${Math.round(box?.height ?? 0)}`);
        await t.shot(`models-${i + 1}`);
        const search = list.locator('input.model-search');
        if (await search.count()) {
          const focused = await search.evaluate((el) => el === document.activeElement);
          t.note(`its search is ${focused ? '' : 'NOT '}focused as the list opens`);
          const q = models[1]?.split(/\s+/)[0]?.slice(0, 4) ?? 'son';
          await t.page.keyboard.type(q.toLowerCase());
          await sleep(300);
          const after = await list.boundingBox();
          t.note(`“${q.toLowerCase()}” leaves ${await list.locator('.model-row').count()} rows; the list ${after?.height === box?.height && after?.x === box?.x ? 'kept' : 'CHANGED'} its size and place`);
          await t.shot('search');
          await t.page.keyboard.type('zzqx');
          await t.see('No models match', { ms: 3000 });
          await t.shot('no-match');
          searched = true;
        }
      }
      if (!searched) t.note('no account has more than 6 models: no search to try');
      // Escape: clears the search, closes the list, closes the menu; nothing chosen
      for (let i = 0; i < 4 && (await t.page.locator('.talk-menu').count()); i++) { await t.page.keyboard.press('Escape'); await sleep(250); }
      if (await t.page.locator('.talk-menu').count()) throw new Error('Escape didn’t close the agent menu');
      const still = await pill.getAttribute('aria-label');
      t.note(`after Escape the pill reads: ${still}`);
    },
  },
  {
    id: 'nested',
    title: 'A task nests under its conversation; its row opens its own thread, its menu the conversation at its card',
    contract: 'live-task.md (LIVETASK-19 to 22, 28)',
    run: async (t) => {
      await home(t);
      const since = Date.now();
      await t.say(`Start a task now in ${t.repo} on Claude Code, in Ask mode: before writing anything, its agent asks me (not you) one question: should the word be "yes" or "no"? Then it makes docs/oby-decision.md with that one word. Change nothing else. Call the task "Oby decision ${stamp.slice(11)}".`);
      const task = track(t, await newTask(t, since));
      t.state.decision = task;
      // its conversation's row, with the task under it
      const kid = t.page.locator('.convo-kid', { hasText: task.title }).first();
      await until(t, () => kid.isVisible(), 60_000).catch(async () => {
        // folded: the twist under the open conversation unfolds it
        const twist = t.page.locator('.convo-row.is-active').locator('xpath=..').locator('.convo-twist');
        if (await twist.count()) await twist.click();
        await kid.waitFor({ timeout: 10_000 });
      });
      t.note(`its row: ${(await kid.innerText()).replace(/\n+/g, ' ')}`);
      const row = t.page.locator('.convo-row.is-active');
      if (await row.count()) t.note(`its conversation's row: ${(await row.innerText()).replace(/\n+/g, ' · ')}`);
      await t.shot('nested-row');
      // a click on its row opens its own thread over the conversation, the row lit (LIVETASK-22)
      await ownThread(t, kid, task);
      // its row's menu, View details: its conversation at its card (LIVETASK-28)
      const card = await openAtTask(t, kid, task.title);
      t.note(`its card: ${(await card.innerText()).replace(/\n+/g, ' · ')}`);
      await t.shot('at-its-card');
    },
  },
  {
    id: 'decision',
    title: 'A decision in the bar over the message box: put off with Later, found again, then answered',
    contract: 'live-task.md (LIVETASK-23 to 26), home.md (WAITING-1 to 4)',
    needs: ['nested'],
    run: async (t) => {
      const task = t.state.decision;
      const bar = () => t.page.locator('.decision-bar').filter({ visible: true }).last();
      const id = task.job.id;
      // in its conversation: the bar comes when the task asks
      await until(t, async () => (await bar().count()) || ended(t.job(id)), 10 * MIN);
      if (ended(t.job(id))) throw new Error(`the task ended (${t.job(id).status}) with no decision asked of the owner`);
      t.note(`the bar: ${(await bar().innerText()).replace(/\n+/g, ' · ')}`);
      await t.shot('bar');
      const kind = t.job(id).ask?.kind;
      t.note(`what it asks, as obid keeps it: ${kind}, “${t.job(id).ask?.headline}”`);
      // Later: off this bar, while the view is open
      await sleep(700);
      await bar().getByRole('button', { name: 'Later', exact: true }).click();
      await sleep(600);
      t.note(`after Later the bar is ${(await bar().count()) ? 'STILL there' : 'gone'}`);
      await t.shot('later');
      // found again: its nested row
      const kid = t.page.locator('.convo-kid', { hasText: task.title }).first();
      t.note(`its row after Later: ${(await kid.isVisible()) ? (await kid.innerText()).replace(/\n+/g, ' ') : 'not shown'}`);
      // its row's menu, View details: its conversation opened at it, where Later's decision is back (LIVETASK-24 to 28)
      await openAtTask(t, kid, task.title);
      t.note(`opened at its task the decision is ${(await bar().count()) ? 'back' : 'MISSING'}`);
      await t.shot('at-its-card');
      // Waiting for you
      const waiting = t.page.getByRole('button', { name: /Waiting for you/ });
      if (await waiting.count()) {
        await waiting.first().click();
        await t.see('Waiting for you', { ms: 5000 });
        await sleep(600);
        const listed = (await t.page.locator('.waiting-list').innerText().catch(() => '')).includes(task.title);
        t.note(`Waiting for you lists it: ${listed ? 'yes' : 'NO'}`);
        await t.shot('waiting');
        await t.page.keyboard.press('Escape');
        await sleep(600);
      } else t.note('no Waiting for you row in the side panel');
      // back to its conversation: the bar again, and answered there
      const convo = t.page.locator('.convo-nest', { has: kid }).locator('.convo-row');
      await convo.first().click();
      await until(t, () => bar().count(), 20_000).catch(() => {});
      t.note(`back in its conversation the bar is ${(await bar().count()) ? 'back' : 'MISSING'}`);
      await t.shot('back-again');
      await sleep(700);
      // (the owner may answer it first from their own app: then it's gone from here, and noted)
      const answer = bar().getByRole('button', { name: /^(yes|Yes|Allow|Allow once|Approve)/ }).first();
      await until(t, async () => (await answer.count()) || ended(t.job(id)), 20_000).catch(() => {});
      if (!(await answer.count())) {
        if (await bar().count()) throw new Error(`no answer to click in the bar: ${(await bar().innerText()).replace(/\n+/g, ' · ')}`);
        t.note('the decision was answered before the driver clicked (from another of the owner’s devices?)');
      } else {
      t.note(`answered: ${await answer.innerText()}`);
      await answer.click();
      await t.shot('answered');
      }
      // the rest of its asks (a permission to write the file) answered the same way, until it ends
      const deadline = Date.now() + 10 * MIN;
      while (!ended(t.job(id)) && Date.now() < deadline) {
        if (await bar().count()) {
          const more = bar().getByRole('button', { name: /^(yes|Yes|Allow|Allow once|Approve|Confirm|No check)/ }).first();
          if (await more.count()) { await sleep(700); t.note(`then: ${(await bar().innerText()).replace(/\n+/g, ' · ')} → ${await more.innerText()}`); await more.click(); }
        }
        await sleep(2000);
      }
      const job = t.job(id);
      t.note(`it ended ${job.status}, ${job.published ?? job.publishError ?? 'not published'}`);
      await t.shot('ended');
    },
  },
  {
    id: 'edit-project',
    title: 'Make a project, edit its repositories and name, then delete it',
    contract: 'projects.md (PROJECTS-25 to 31)',
    run: async (t) => {
      await home(t);
      const name = `Oby drive ${stamp.slice(11).replace(':', '')}`;
      const add = t.page.getByRole('button', { name: 'New project' });
      if (!(await add.count())) throw new Error('no New project in the side panel (projects off?)');
      await add.first().click();
      await t.page.locator('#project-name').fill(name);
      await t.click('Add from GitHub…');
      const repoItem = t.page.getByRole('menuitemcheckbox', { name: t.repo, exact: false });
      await until(t, () => repoItem.count(), 15_000).catch(() => {});
      if (!(await repoItem.count())) { await t.shot('no-repo'); throw new Error(`${t.repo} isn't in Add from GitHub…’s list`); }
      await repoItem.first().click();
      await t.shot('new');
      await t.click('Create project');
      const head = t.page.locator('.project-group', { hasText: name });
      await head.first().waitFor({ timeout: 15_000 });
      await t.shot('made');
      // edit: its ⋮, Edit project…
      await t.page.getByRole('button', { name: `More for ${name}` }).click();
      await t.page.getByRole('menuitem', { name: 'Edit project…' }).click();
      await t.see('Edit project', { ms: 5000 });
      const rows = await t.page.locator('.project-sheet .project-row').allInnerTexts();
      t.note(`its sheet shows: ${rows.map((r) => r.replace(/\n+/g, ' ')).join(' | ')}`);
      await t.shot('edit');
      await t.page.getByRole('button', { name: `Remove ${t.repo.split('/')[1]}` }).click();
      await t.page.locator('#project-name').fill(`${name} edited`);
      await t.shot('edited');
      await t.click('Save');
      await t.page.locator('.project-group', { hasText: `${name} edited` }).first().waitFor({ timeout: 15_000 });
      // what obid kept, as the sheet shows it when opened again
      await t.page.getByRole('button', { name: `More for ${name} edited` }).click();
      await t.page.getByRole('menuitem', { name: 'Edit project…' }).click();
      await t.see('Edit project', { ms: 5000 });
      const kept = await t.page.locator('.project-sheet .project-row').count();
      t.note(`opened again it has ${kept} folders and repositories`);
      await t.shot('reopened');
      await t.click('Delete project…');
      await t.click('Delete project');
      await until(t, async () => !(await t.page.locator('.project-group', { hasText: `${name} edited` }).count()), 15_000);
      await t.shot('deleted');
      if (kept) throw new Error(`the removed repository came back: ${kept} rows`);
    },
  },
  {
    id: 'maximized',
    title: 'The window maximized: the conversation column grows, up to T3 Code’s wide width',
    contract: 'chat-first.md (CHATFIRST-34)',
    run: async (t) => {
      // the latest conversation with words in it, in a normal window, then maximized
      await home(t);
      const row = t.page.locator('.convo-row').first();
      if (await row.count()) await row.click();
      await sleep(800);
      const measure = () => t.page.evaluate(() => {
        const col = [...document.querySelectorAll('.chat-column')].filter((e) => e.checkVisibility()).pop();
        const box = [...document.querySelectorAll('.composer')].filter((e) => e.checkVisibility()).pop();
        const side = document.querySelector('.sidebar');
        // anything a person can't see: past the window's right edge
        const past = [...document.querySelectorAll('.composer, .chat-column, .asks, .ask-card, .decision-bar')].filter((e) => e.checkVisibility())
          .map((e) => Math.round(e.getBoundingClientRect().right - innerWidth)).filter((d) => d > 1);
        return { window: innerWidth, column: Math.round(col?.getBoundingClientRect().width ?? 0), box: Math.round(box?.getBoundingClientRect().width ?? 0), boxRight: Math.round(box?.getBoundingClientRect().right ?? 0), sidebar: Math.round(side?.getBoundingClientRect().width ?? 0), pastRightEdge: past };
      });
      // a narrow window (700px, the side panel open: the app's least is 420), then the app's own
      // (994 × 670), then maximized
      await t.app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setSize(700, 670));
      await sleep(1000);
      const narrow = await measure();
      t.note(`narrow: ${JSON.stringify(narrow)}`);
      await t.shot('narrow');
      await t.app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setSize(994, 670));
      await sleep(1000);
      const normal = await measure();
      t.note(`normal: ${JSON.stringify(normal)}`);
      await t.shot('normal');
      await t.app.evaluate(({ BrowserWindow, screen }) => { const w = BrowserWindow.getAllWindows()[0]; w.maximize(); return screen.getPrimaryDisplay().workAreaSize; });
      await sleep(1500);
      const max = await measure();
      t.note(`maximized: ${JSON.stringify(max)}`);
      await t.shot('maximized');
      await t.app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].unmaximize());
      await sleep(800);
      if (max.window > normal.window && max.column <= normal.column) throw new Error(`the column didn't grow: ${normal.column} → ${max.column}px`);
    },
  },
  {
    id: 'stop-left',
    title: 'Stop a task an earlier run left running, from its card’s menu (OBY_DRIVE_STOP names it)',
    contract: 'live-task.md (LIVETASK-28, 29)',
    run: async (t) => {
      const title = process.env.OBY_DRIVE_STOP;
      if (!title) { t.note('nothing named in OBY_DRIVE_STOP'); return; }
      await home(t);
      // its conversation's row, unfolded, then its own row's menu
      const kid = t.page.locator('.convo-kid', { hasText: title }).first();
      for (let i = 0; i < Math.min(15, await t.page.locator('.convo-nest').count()) && !(await kid.isVisible()); i++) {
        const twist = t.page.locator('.convo-nest').nth(i).locator('.convo-twist[aria-expanded="false"]');
        if (await twist.count()) { await twist.click(); await sleep(300); }
      }
      // its row's menu: its conversation at its card (LIVETASK-28), then Stop from the card's menu
      await openAtTask(t, kid, title);
      await t.shot('at-its-card');
      await t.click(`More for ${title}`);
      await t.page.getByRole('menuitem', { name: 'Stop task', exact: true }).click();
      await sleep(3000);
      await t.shot('stopped');
      t.note(`its row now: ${(await kid.innerText()).replace(/\n+/g, ' ')}`);
    },
  },
  {
    id: 'browser',
    title: 'An agent opens a page in Oby’s browser: its tab beside the conversation, Take control, Give back, its answer',
    contract: 'browser-app.md (BROWSERAPP-1, 2, 9, 26, 40, 41), browser.md (BROWSER-3, 5, 25)',
    run: async (t) => {
      // Oby's own turn has no browser (brain/harness_cursor.rs: "A conversation's agent gets the
      // browser with the rest of BROWSER-17"): a task's agent does, so the words ask for a task. It
      // changes nothing in the repository; it waits two minutes so its tab is there to take (a task's
      // tabs leave the list when it ends, BROWSER-25).
      await home(t);
      const since = Date.now(), name = `Oby browser ${stamp.slice(11)}`;
      await t.say(`Start a task now in ${t.repo} on Claude Code: open ${PAGE} in Oby's browser (your oby_browser tools) and read the page's main heading. Keep that tab open for me: once you have read it, wait two minutes (sleep 120) before you finish. End by telling me the heading. Change no files and open no pull request. Call the task "${name}".`);
      if (!(await startedJob(t, since))) throw new Error(`no task started in ${t.repo} in 5 minutes`);
      const task = track(t, await newTask(t, since, ['investigate', 'research', 'build', 'fix', 'test']));
      const id = task.job.id, who = AGENT[task.job.agent] ?? task.job.agent;
      t.note(`its task: ${id}, a ${task.job.kind} on ${task.job.agent}/${task.job.account}, mode ${task.job.mode}`);
      // the globe at the title bar's end says how many tabs, and who uses one (BROWSERAPP-1)
      const globe = t.page.locator('[data-browser="globe"]');
      const label = () => globe.getAttribute('aria-label', { timeout: 1000 }).catch(() => '');
      const deadline = Date.now() + 8 * MIN;
      while (!/^Show the browser, [1-9]/.test((await label()) ?? '')) {
        const job = t.job(id);
        if (ended(job)) throw new Error(`the task ended ${job.status} and no tab showed in the window (globe: “${await label()}”): ${(job.summary ?? job.error ?? '').split('\n')[0]}`);
        if (Date.now() > deadline) throw new Error(`no tab in the window in 8 minutes (globe: “${await label()}”)`);
        await answerBar(t);
        await sleep(1000);
      }
      t.note(`the globe: “${await label()}”`);
      await t.shot('globe');
      await globe.click();
      t.step('clicked the globe');
      const panel = t.page.locator('aside[data-browser="browser"]');
      await panel.waitFor({ timeout: 10_000 });
      // its tab, by its page's title or address (the owner's other agents may have tabs too)
      const tab = panel.locator('.br-tab', { hasText: /Example Domain|example\.com/i }).first();
      await until(t, () => tab.isVisible(), 60_000).catch(async () => { throw new Error(`no tab for ${PAGE} in the panel: ${(await panel.locator('.br-tabs').innerText()).replace(/\n+/g, ' · ')}`); });
      await tab.click();
      const address = panel.locator('.br-address');
      await until(t, async () => /example\.com/.test(await address.innerText()), 30_000).catch(async () => { throw new Error(`the address doesn't say example.com: “${await address.innerText()}”`); });
      const strip = panel.locator('[data-browser="control"]');
      const said = await strip.innerText();
      t.note(`tabs: ${(await panel.locator('.br-tab').allInnerTexts()).map((s) => s.replace(/\n+/g, ' ')).join(' | ')}; address: ${await address.innerText()}; strip: ${said.replace(/\n+/g, ' · ')}; webviews in the window: ${await t.page.locator('webview').count()}`);
      await t.shot('panel');
      if (!said.includes(who)) throw new Error(`the strip doesn't name ${who}: “${said}”`);
      // Take control (signed by its own label, BROWSERAPP-9): the strip says it's yours, the page ringed
      if (!(await tab.isVisible())) throw new Error(`the tab left before Take control (the task is ${t.job(id).status})`);
      await strip.getByRole('button', { name: 'Take control' }).click();
      t.step('clicked “Take control”');
      await until(t, async () => (await strip.innerText()).includes('You have this tab.'), 20_000).catch(async () => { throw new Error(`Take control didn't hold the tab: the strip says “${(await strip.innerText()).replace(/\n+/g, ' · ')}”`); });
      // the keyboard moves once obid has answered the take (BROWSERAPP-40): waited for, not sampled once
      const onPage = await until(t, () => t.page.evaluate(() => document.activeElement?.tagName === 'WEBVIEW'), 5000).then(() => true, () => false);
      t.note(`held: “${(await strip.innerText()).replace(/\n+/g, ' · ')}”; the page ringed: ${(await panel.locator('.br-view.is-held').count()) ? 'yes' : 'NO'}; the keyboard on the page within 5 s: ${onPage ? 'yes' : `NO (focus on ${await t.page.evaluate(() => document.activeElement?.tagName ?? 'nothing')})`}`);
      await t.shot('held');
      // Give back: the strip is the agent's again, the keyboard back in the message box (BROWSERAPP-41)
      await strip.getByRole('button', { name: 'Give back' }).click();
      t.step('clicked “Give back”');
      await until(t, async () => { const now = await strip.innerText(); return !now.includes('You have this tab.') && now.includes(who); }, 20_000)
        .catch(async () => { throw new Error(`Give back didn't give the tab back: the strip says “${(await strip.innerText().catch(() => 'nothing')).replace(/\n+/g, ' · ')}”`); });
      const inBox = await until(t, () => t.page.evaluate(() => !!document.activeElement?.closest('[contenteditable="true"]')), 5000).then(() => true, () => false);
      t.note(`given back: “${(await strip.innerText()).replace(/\n+/g, ' · ')}”; the keyboard in the message box within 5 s: ${inBox ? 'yes' : `NO (focus on ${await t.page.evaluate(() => document.activeElement?.tagName ?? 'nothing')})`}`);
      await t.shot('given-back');
      // its end, and its answer in the conversation (not the panel's tab title)
      const end = Date.now() + 10 * MIN;
      while (!ended(t.job(id)) && Date.now() < end) { await answerBar(t); await sleep(2000); }
      const job = t.job(id);
      if (!ended(job)) throw new Error('the task didn’t end in 10 minutes');
      t.note(`it ended ${job.status}, ${job.commits ?? 0} commits, ${job.published ?? 'nothing published'}; its summary: ${(job.summary ?? '').replace(/\n+/g, ' ').slice(0, 300)}`);
      const answer = await until(t, async () => (await openChat(t).locator('.msg.is-oby, .msg-task').allInnerTexts()).find((m) => HEADING.test(m)), 3 * MIN).catch(() => null);
      await t.shot('answer');
      t.note(`after it ended the globe says “${await label()}”, the panel shows ${(await panel.count()) ? `“${(await panel.locator('.br-tabs').innerText().catch(() => '')).replace(/\n+/g, ' · ')}”` : 'nothing'}`);
      if (await t.page.getByRole('button', { name: 'Hide the browser' }).count()) await t.click('Hide the browser');
      if (!answer) throw new Error(`the conversation doesn't name the heading (“Example Domain”)${HEADING.test(job.summary ?? '') ? ', though the task’s summary does' : ''}`);
      t.note(`its answer: …${answer.replace(/\n+/g, ' ').match(/[^.]{0,160}Example Domain[^.]{0,80}/i)?.[0] ?? ''}…`);
    },
  },
  {
    id: 'artifact',
    title: 'Oby makes a page as an artifact: its card, the pane, the preview, Code, its version, Download',
    contract: 'artifacts-app.md (ARTIFACTSAPP-1, 4, 5, 11, 15, 18), artifacts.md',
    run: async (t) => {
      await home(t);
      const since = Date.now(), name = `Oby drive counter ${stamp.slice(11)}`;
      await t.say(`Make me a small self-contained HTML page as an Oby artifact: a counter, with a button labelled "Count" and the number of clicks beside it, everything in one index.html and nothing fetched from outside. Call it "${name}".`);
      // Oby's turn publishes it (oby_artifact), or a task does and its report brings the card
      const chat = openChat(t), card = chat.locator('button.art-card').last();
      const deadline = Date.now() + 10 * MIN;
      let nudged = false;
      while (!(await card.isVisible().catch(() => false))) {
        if (Date.now() > deadline) throw new Error(`no artifact card in the conversation in 10 minutes; Oby said: ${((await chat.locator(REPLY).allInnerTexts()).at(-1) ?? 'nothing').replace(/\n+/g, ' ').slice(0, 300)}`);
        const job = t.newestJob(since);
        if (job && !(t.state.jobs ?? []).includes(job.id)) { (t.state.jobs ??= []).push(job.id); track(t, job); t.note(`Oby started a task for it: ${job.id}, a ${job.kind}, “${job.title}”`); }
        const busy = await t.page.locator('.msg.is-streaming, .msg-step').count();
        const last = (await chat.locator(REPLY).count()) ? (await chat.locator(REPLY).last().innerText()).trim() : '';
        if (!nudged && !busy && /\?\s*$/.test(last)) { t.note(`Oby asked first: ${last.replace(/\n+/g, ' ')}`); await t.say('Yes, make it.'); nudged = true; }
        await answerBar(t);
        await sleep(1000);
      }
      t.step('saw its card');
      t.note(`its card: ${(await card.innerText()).replace(/\n+/g, ' · ')}`);
      // its picture (ARTIFACTSAPP-18): the board's blocks until main has taken it
      const picture = await until(t, () => card.locator('img.art-thumb-img').evaluate((i) => i.complete && i.naturalWidth > 0), 30_000).then(() => true, () => false);
      t.note(`its picture: ${picture ? 'shown' : 'NOT shown in 30 s (the board’s blocks)'}`);
      await t.shot('card');
      const problems = [];
      if (!/Oby artifact · Page · v\d+/.test(await card.innerText())) problems.push(`its card doesn't say “Oby artifact · Page · v<n>”: “${(await card.innerText()).replace(/\n+/g, ' · ')}”`);
      // the pane: an artifact made in the conversation on view may open by itself (ARTIFACTSAPP-6)
      const strip = t.page.locator('[data-artifact-strip]');
      t.note(`before its card's click the pane is ${(await strip.isVisible()) ? 'open already' : 'closed'}`);
      await card.click();
      t.step('clicked its card');
      await strip.waitFor({ timeout: 15_000 });
      const head = (await strip.locator('.art-strip-title').innerText()).replace(/\n+/g, ' · ');
      t.note(`the pane: ${head}${(await strip.locator('select.art-version').count()) ? `; versions: ${(await strip.locator('select.art-version option').allInnerTexts()).join(', ')}` : ''}`);
      if (!/Oby artifact · Page · v\d+/.test(head)) problems.push(`the pane's strip doesn't say “Oby artifact · Page · v<n>”: “${head}”`);
      // Preview: main's view draws it, Sandpack's status settled (evidence only)
      const status = await until(t, () => inHost(t, 'Promise.race([window.obyStatus, new Promise((r) => setTimeout(() => r(null), 100))])'), 60_000).catch(() => null);
      const text = await inFrame(t, 'document.body.innerText').catch(() => null);
      t.note(`its preview: status ${JSON.stringify(status)}; the page says “${(text ?? '(no frame)').replace(/\n+/g, ' · ').slice(0, 200)}”`);
      const pageError = await t.page.locator('.art-page-error').innerText().catch(() => '');
      if (pageError) t.note(`the pane's error strip: ${pageError.replace(/\n+/g, ' · ')}`);
      await t.shot('preview');
      if (!status?.drawn || status.error) problems.push(`the preview didn't draw: ${JSON.stringify(status)}`);
      else if (!/Count/i.test(text ?? '')) problems.push(`the preview drew no Count button: “${text}”`);
      // Code: its files as tabs, index.html first, the one picked shown (ARTIFACTSAPP-5)
      await t.page.getByRole('tab', { name: 'Code', exact: true }).click();
      t.step('clicked “Code”');
      const files = t.page.locator('.art-files [role="tab"]');
      await until(t, () => files.count(), 15_000).catch(() => {});
      const editor = t.page.locator('.art-code-editor');
      const code = await until(t, async () => (await editor.innerText()).trim(), 15_000).catch(() => '');
      t.note(`its files: ${(await files.allInnerTexts()).join(', ') || 'none'}; the code starts “${code.replace(/\n+/g, ' ').slice(0, 120)}”`);
      await t.shot('code');
      if (!(await files.count())) problems.push('Code shows no file tabs');
      else if (!/<button/i.test(code)) problems.push('Code doesn’t show the page’s button');
      await t.page.getByRole('tab', { name: 'Preview', exact: true }).click();
      t.step('clicked “Preview”');
      // Download (board 1c): main's save dialog answered for the owner with a file in this run's
      // folder (no one can click a hidden window's sheet), never their Downloads; the pane says where
      const to = join(t.out, 'artifact-export.zip');
      await t.app.evaluate(({ dialog }, to) => { globalThis.__obyDriveSave ??= dialog.showSaveDialog; dialog.showSaveDialog = async () => ({ canceled: false, filePath: to }); }, to);
      try {
        await t.click('Export');
        await t.page.getByRole('menu', { name: 'Download' }).waitFor({ timeout: 5000 });
        await t.shot('export-menu');
        await t.page.getByRole('menuitem', { name: 'Project archive (.zip)' }).click();
        t.step('clicked “Project archive (.zip)”');
        const saved = await until(t, async () => (await t.page.locator('.art-saved').innerText()).trim(), 30_000).catch(() => '');
        const zip = existsSync(to) ? readFileSync(to) : null;
        t.note(`the pane says “${saved}”; the file: ${zip ? `${zip.length} bytes, ${zip.subarray(0, 2).toString() === 'PK' ? 'a zip' : 'NOT a zip'}` : 'none'}`);
        await t.shot('downloaded');
        if (saved !== `Saved to ${to}`) problems.push(`the pane doesn't say where the download went: “${saved}”`);
        if (!zip || zip.subarray(0, 2).toString() !== 'PK') problems.push(`no zip saved at ${to}`);
      } finally {
        await t.app.evaluate(({ dialog }) => { if (globalThis.__obyDriveSave) dialog.showSaveDialog = globalThis.__obyDriveSave; }).catch(() => {});
      }
      await t.click('Close the artifact');
      if (problems.length) throw new Error(problems.join('; '));
    },
  },
  {
    id: 'delete-task',
    title: 'Delete a finished task from its card’s menu: it’s gone at once, or obid asks first, or says why not',
    contract: 'tasks.md (TASKS-3, 11)',
    run: async (t) => {
      // a task of this run's that changes nothing, ended
      await home(t);
      const since = Date.now(), name = `Oby delete ${stamp.slice(11)}`;
      await t.say(`Start a task now in ${t.repo} on Claude Code: read the README and tell me its first line. Change no files and open no pull request. Call the task "${name}".`);
      if (!(await startedJob(t, since))) throw new Error(`no task started in ${t.repo} in 5 minutes`);
      const task = track(t, await newTask(t, since, ['investigate', 'research', 'build', 'fix', 'test']));
      const id = task.job.id, title = task.title;
      const end = Date.now() + 10 * MIN;
      while (!ended(t.job(id)) && Date.now() < end) { await answerBar(t); await sleep(2000); }
      const job = t.job(id);
      if (!ended(job)) throw new Error('the task didn’t end in 10 minutes');
      t.note(`it ended ${job.status}, ${job.commits ?? 0} commits, ${job.published ?? 'nothing published'}`);
      const more = t.page.getByRole('button', { name: `More for ${title}`, exact: true }).filter({ visible: true });
      const row = t.page.locator('.convo-kid', { hasText: title });
      await until(t, () => more.count(), 30_000).catch(() => { throw new Error(`no “More for ${title}” on its card`); });
      t.note(`before Delete: its rows in the side panel ${await row.count()}`);
      await t.shot('ended');
      const before = new Set((await t.shown()).split('\n').map((l) => l.trim()).filter(Boolean));
      // Delete, from its card's menu
      await more.first().click();
      t.step(`clicked “More for ${title}”`);
      const del = t.page.getByRole('menuitem', { name: 'Delete task', exact: true });
      await del.waitFor({ timeout: 5000 });
      await t.shot('menu');
      await del.click();
      t.step('clicked “Delete task”');
      // gone (TASKS-3), at once, with no reload: no card's menu, no row in the side panel
      const gone = async () => !(await more.count()) && !(await row.count());
      // or obid's Confirm (TASKS-11): commits that aren't published, or work that isn't committed
      const confirm = t.page.locator('.msg-ask, .decision-bar, article.ask-card').filter({ visible: true }).filter({ hasText: /lost|isn.t committed|aren.t published|Delete/i })
        .getByRole('button', { name: 'Confirm', exact: true }).first();
      const outcome = await until(t, async () => ((await gone()) ? 'gone' : (await confirm.count()) ? 'confirm' : null), 10_000).catch(() => null);
      await t.shot('after-delete');
      if (outcome === 'confirm') {
        t.note(`obid asked first: ${(await t.page.locator('.msg-ask, .decision-bar, article.ask-card').filter({ has: confirm }).first().innerText()).replace(/\n+/g, ' · ')}`);
        await sleep(700); // (a click in a card's first half second answers nothing: AskBlock)
        await confirm.click();
        t.step('clicked “Confirm”');
        const after = await until(t, gone, 10_000).then(() => true, () => false);
        await t.shot('confirmed');
        if (!after) throw new Error(`after Delete then Confirm the task is still shown: ${await row.count()} side-panel rows, its card's menu ${(await more.count()) ? 'still there' : 'gone'}`);
      } else if (outcome !== 'gone') {
        // still there: what's new on screen (obid's words, a 409's), and whether its menu still offers Delete
        const added = (await t.shown()).split('\n').map((l) => l.trim()).filter((l) => l && !before.has(l));
        let offers = 'no menu';
        if (await more.count()) {
          await more.first().click();
          offers = (await del.waitFor({ timeout: 3000 }).then(() => true, () => false)) ? 'still offers Delete task' : 'no longer offers Delete task';
          await t.shot('menu-again');
          await t.page.keyboard.press('Escape');
        }
        const kept = jobOrNone(t, id) ? `obid still keeps it (${jobOrNone(t, id).status})` : 'obid deleted its job file';
        throw new Error(`10 s after Delete task it is still shown: ${await row.count()} side-panel rows; its card's menu ${offers}; ${kept}; new on screen: ${added.length ? added.slice(0, 6).map((l) => `“${l.slice(0, 120)}”`).join(', ') : 'nothing (no words from obid)'}`);
      }
      // Home and the side panel, as the owner comes back to them: no card, no row
      await home(t);
      const left = await t.page.locator('article.ask-card, .convo-kid, .convo-row').filter({ hasText: title }).count();
      await t.shot('home');
      if (left) throw new Error(`deleted, the task is still on Home or in the side panel (${left})`);
      t.note(`obid's job file is ${jobOrNone(t, id) ? 'still there' : 'gone'}`);
    },
  },
];

// waits until f gives something (a scenario's own wait, not on the window's text)
async function until(t, f, ms) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    try { const v = await f(); if (v) return v; } catch { /* not yet */ }
    await sleep(400);
  }
  throw new Error(`waited ${Math.round(ms / 1000)} s for nothing`);
}
