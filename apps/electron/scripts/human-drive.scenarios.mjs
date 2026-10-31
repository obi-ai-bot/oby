// The owner's conversation loop, one scenario each (human-drive.mjs runs them in this order). A
// scenario is { id, title, contract, needs?, run(t) }: `t.say` types and sends, `t.reply` waits for
// Oby's answer, `t.click` clicks a button by its name, `t.see` waits until the window shows
// something, `t.shot` keeps a screenshot, `t.note` writes a line in the report, `t.state` carries
// what a later scenario needs (a scenario whose `needs` didn't pass is skipped). Waits are on what
// the window shows (obid's job files only say when a task ended, as evidence). Tasks stay tiny: one
// line in one file of the scratch repository.
const MIN = 60_000;
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const ASKS = 'section[aria-label="Oby is asking"]';
const PR_OPEN = /Pull request open · waiting for your review on GitHub|Merged into \S+|Oby is reviewing|Waiting for review[^\n]*/;
const ended = (j) => ['done', 'failed', 'interrupted', 'stopped'].includes(j?.status);
const stamp = new Date().toISOString().slice(0, 16);
// a task name no earlier run used: the side panel finds tasks by name
const taskName = `Oby drive ${stamp.slice(11)}`;

async function home(t) {
  const back = t.page.getByRole('button', { name: 'Back to Home' });
  if (await back.isVisible()) { await t.click('Back to Home'); await back.waitFor({ state: 'detached', timeout: 10_000 }).catch(() => {}); }
  await t.see(/Fix a bug|What should I work on|need you|All caught up/, { label: 'Home' });
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
// reviews, their own tasks) are never touched
async function answerCards(t, { done, ms = 25 * MIN }) {
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
      if (ours.has(title)) {
        // a check for the scratch repository: none (its tests need the network); anything else: yes,
        // and the question's answer, "asked"
        const answer = top.getByRole('button', { name: /check before publishing/.test(text) ? /^(No check|Cancel)$/ : /^(Allow|Approve|Merge|Confirm|Yes|Start|asked)/ }).first();
        if (await answer.count()) {
          t.state.cards = (t.state.cards ?? 0) + 1;
          await t.shot(`card-${t.state.cards}`);
          const name = await answer.innerText();
          t.note(`card: ${text.replace(/\n+/g, ' · ')} → ${name}`);
          await answer.click();
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
      if (!(await next.count())) break;
      await next.click();
      await sleep(300);
    }
    await sleep(2000);
  }
  throw new Error(`not done in ${ms / MIN} minutes`);
}

// a task's page, from the side panel, as the owner opens it
async function openTask(t, title) {
  await home(t);
  if (await t.page.getByRole('button', { name: 'Show sidebar' }).isVisible()) await t.click('Show sidebar');
  await t.page.locator('nav[aria-label="Side panel"]').getByText(title, { exact: true }).first().click();
  await t.see('Tell Oby about this task…', { label: 'its page' });
}

// the Confirm Oby's answer made to merge `url` (Merge <repo>#<n> into …), under that answer: its
// exact title tracked as this run's, then Confirm, the owner's one click
async function confirmMerge(t, url) {
  const n = url.match(/\/pull\/(\d+)$/)?.[1];
  // this run's repository and pull request only, never another of the owner's
  const repo = t.repo.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const card = t.page.locator('.msg-ask').filter({ has: t.page.locator('.tc-ask-head', { hasText: new RegExp(`^Merge ${repo}#${n} into `, 'i') }) }).last();
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
      const url = t.gh('issue', 'create', '-R', t.repo, '--title', `Oby drive ${stamp}: one line in docs/oby-drive.md`,
        '--body', `Add the line \`- ${stamp}: driven by Oby (issue)\` at the end of docs/oby-drive.md (make the file if it isn't there). Change nothing else. Opened by apps/electron/scripts/human-drive.mjs.`);
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
      const seen = await t.see(PR_OPEN, { ms: 5 * MIN, label: 'its pull request on its page' });
      await t.shot('pull-request');
      const job = t.job(id);
      t.state.job = job;
      t.note(`it says: ${seen}; evidence: ${job.agent}/${job.account}, mode ${job.mode}, vetted ${job.vetted}, ${job.published ?? job.publishError}`);
    },
  },
  {
    id: 'follow-up',
    title: 'A follow-up on its page: same kind, agent, account and repository',
    contract: 'task-conversations.md (TASKCONV-3)',
    needs: ['to-pr'],
    run: async (t) => {
      const since = Date.now();
      await t.say('Add a second line "- follow-up" under it.', 'Tell Oby about this task…');
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
      const before = t.state.cards ?? 0;
      await home(t);
      // the card Oby gives the owner: a merge, a review's decision, or nothing while it merges itself
      await answerCards(t, { done: merged, ms: 10 * MIN }).catch((e) => t.note(`the cards didn't merge it: ${e.message}`));
      if (!merged()) {
        // no card merged it (a question with only "Done" can't): the owner tells Oby, as they would
        t.note(`not merged after the cards (${(t.state.cards ?? 0) - before} answered): the owner says so in words`);
        await openTask(t, t.state.task.title);
        await t.say('Merge it as it is.', 'Tell Oby about this task…');
        t.note(`Oby: ${(await t.reply({ ms: 5 * MIN })).replace(/\n+/g, ' ')}`);
        await t.shot('told-to-merge');
        // words read with untrusted text make a Confirm: under Oby's answer (and on Home), answered
        // here by its exact title, tracked like the run's other cards, before the merge is waited for
        await confirmMerge(t, url).catch((e) => t.note(`no Confirm to answer in the conversation: ${e.message}`));
        const deadline = Date.now() + 3 * MIN;
        while (!merged() && Date.now() < deadline) await sleep(5000);
        await home(t);
        await answerCards(t, { done: merged, ms: 15 * MIN });
      }
      await openTask(t, t.state.task.title);
      await t.see(/Merged[^\n]*/, { ms: 3 * MIN, label: 'merged, on its page' });
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
];
