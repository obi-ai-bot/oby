// The owner's conversation loop, one scenario each (human-drive.mjs runs them in this order). A
// scenario is { id, title, contract, needs?, run(t) }: `t.say` types and sends, `t.reply` waits for
// Oby's answer, `t.click` clicks a button by its name, `t.see` waits until the window shows
// something, `t.shot` keeps a screenshot, `t.note` writes a line in the report, `t.state` carries
// what a later scenario needs (a scenario whose `needs` didn't pass is skipped). Waits are on what
// the window shows (obid's job files only say when a task ended, as evidence). Tasks stay tiny: one
// line in one file of the scratch repository, a file of its own for each scenario that makes a
// pull request, so the run never conflicts with itself.
const MIN = 60_000;
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const ASKS = 'section[aria-label="Oby is asking"]';
const REPLY = '.msg.is-oby .msg-bubble';
const PR_OPEN = /Pull request open · waiting for your review on GitHub|Merged into \S+|Oby is reviewing|Waiting for review[^\n]*/;
const ended = (j) => ['done', 'failed', 'interrupted', 'stopped'].includes(j?.status);
const stamp = new Date().toISOString().slice(0, 16);
// a task name no earlier run used: the side panel finds tasks by name
const taskName = `Oby drive ${stamp.slice(11)}`;

// the empty chat, Home's stage, where words start a new conversation (docs/contracts/chat-first.md)
async function home(t) {
  if (await t.page.getByRole('button', { name: 'Show sidebar' }).isVisible()) await t.click('Show sidebar');
  const oby = t.page.locator('.rail-button[aria-label="Chats"]');
  if ((await oby.getAttribute('aria-pressed')) !== 'true') await oby.click();
  await t.click('New conversation');
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
      const seen = await t.see(PR_OPEN, { ms: 5 * MIN, label: 'its pull request in its conversation' });
      await t.shot('pull-request');
      const job = t.job(id);
      t.state.job = job;
      t.note(`it says: ${seen}; evidence: ${job.agent}/${job.account}, mode ${job.mode}, vetted ${job.vetted}, ${job.published ?? job.publishError}`);
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
    title: 'A task nests under its conversation and opens its own thread',
    contract: 'live-task.md (LIVETASK-19 to 22)',
    run: async (t) => {
      await home(t);
      const since = Date.now();
      await t.say(`Start a task now in ${t.repo} on Claude Code, in Ask mode: before writing anything, its agent asks me (not you) one question: should the word be "yes" or "no"? Then it makes docs/oby-decision.md with that one word. Change nothing else.`);
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
      await kid.click();
      await t.page.locator('.agent-layer').filter({ visible: true }).waitFor({ timeout: 10_000 });
      t.note(`its thread: ${(await t.page.locator('.agent-layer').last().innerText()).slice(0, 300).replace(/\n+/g, ' · ')}`);
      await t.shot('task-thread');
      await t.page.keyboard.press('Escape');
      await sleep(600);
      await t.shot('back');
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
      // its own thread
      await kid.click();
      await t.page.locator('.agent-layer').filter({ visible: true }).waitFor({ timeout: 10_000 });
      await sleep(600);
      const inThread = await t.page.locator('.agent-layer .decision-bar').count();
      t.note(`in its thread the decision is ${inThread ? 'there' : 'MISSING'}`);
      await t.shot('in-thread');
      await t.page.keyboard.press('Escape');
      await sleep(600);
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
    title: 'Stop a task an earlier run left running, from its own thread (OBY_DRIVE_STOP names it)',
    contract: 'live-task.md (LIVETASK-22)',
    run: async (t) => {
      const title = process.env.OBY_DRIVE_STOP;
      if (!title) { t.note('nothing named in OBY_DRIVE_STOP'); return; }
      await home(t);
      // its conversation's row, unfolded, then its own row
      const kid = t.page.locator('.convo-kid', { hasText: title }).first();
      for (let i = 0; i < Math.min(15, await t.page.locator('.convo-nest').count()) && !(await kid.isVisible()); i++) {
        const twist = t.page.locator('.convo-nest').nth(i).locator('.convo-twist[aria-expanded="false"]');
        if (await twist.count()) { await twist.click(); await sleep(300); }
      }
      await kid.click();
      await t.page.locator('.agent-layer').filter({ visible: true }).waitFor({ timeout: 10_000 });
      await t.shot('its-thread');
      await t.click('Stop', '.agent-layer');
      await sleep(3000);
      await t.shot('stopped');
      t.note(`its row now: ${(await kid.innerText()).replace(/\n+/g, ' ')}`);
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
