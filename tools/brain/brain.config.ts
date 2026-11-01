import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import type { BrainConfig } from './generator/config.js';

const OBID = 'core/crates/obid/src';
const TESTS = 'core/crates/obid/tests';
const APP = 'apps/electron/src';

/** Oby: tools/brain lives two levels below the repository's root. */
export const config: BrainConfig = {
  name: 'Oby',
  root: resolve(fileURLToPath(new URL('.', import.meta.url)), '../..'),
  obid: { extractor: 'tools/brain/obid-extract', crate: 'core/crates/obid', apiDoc: 'docs/runner-api.md' },
  app: {
    renderer: `${APP}/renderer/src`,
    main: [`${APP}/main`, `${APP}/preload`],
    apiFile: `${APP}/renderer/src/obid.ts`,
    wireFile: `${APP}/shared/obid-wire.ts`,
    wireRust: { Account: 'AccountJson' },
    // oby-app-web in .claude/launch.json: the renderer alone, on the mock, beside sangam-design (5175)
    webUrl: 'http://localhost:5176/',
    firstRun: `${APP}/renderer/src/FirstRun.tsx`,
    controlCenter: `${APP}/renderer/src/dev/ControlCenter.tsx`,
    stopAt: [`${APP}/renderer/src/live.ts#set`, `${APP}/renderer/src/live.ts#publish`],
    flags: [
      { flag: 'everything', label: 'Everything', note: 'All of Sangam\'s design with its demo data, v1\'s hidden screens included' },
      { flag: 'onboarding', label: 'First run', note: 'From its first step' },
      { flag: 'welcome', label: 'Set up again in a conversation', note: 'The earlier in-conversation setup (onboarding.ts)' },
    ],
  },
  // The eleven areas of the 2026-10-29 plan. Domain areas may not import adapters.
  areas: [
    { id: 'planning', title: 'Planning', summary: 'The plan schema and its problems: what a turn may dispatch.', domain: true, modules: [] },
    { id: 'conversation', title: 'Conversation', summary: 'Conversations, the brain\'s turns, transcripts, artifacts and memory.', modules: ['brain', 'conversation', 'task_conversation', 'transcript', 'activity', 'detail', 'artifacts', 'memory'] },
    { id: 'jobs', title: 'Jobs', summary: 'Starting, scheduling and running jobs, their records and checks.', modules: ['jobs', 'job', 'store', 'groups', 'facts', 'failures', 'routines', 'obi_dir', 'clear', 'meter'] },
    { id: 'agents', title: 'Agents', summary: 'The agent CLIs: Claude Code, Codex, Cursor, and how obid talks to them.', modules: ['agent', 'codex', 'acp', 'conn', 'process'] },
    { id: 'accounts', title: 'Accounts and limits', summary: 'Accounts, sign-ins, usage, limits and the owner\'s limit decisions.', modules: ['accounts', 'usage', 'limits', 'account_limits'] },
    { id: 'workspace', title: 'Workspace', summary: 'Clones, worktrees, repositories and working folders.', modules: ['git', 'repos', 'folders'] },
    { id: 'publish', title: 'Publish and review', summary: 'Publishing, reviews, merging and the decision checks before them.', modules: ['publish', 'merge', 'review', 'decisions', 'teammates'] },
    { id: 'connectors', title: 'Connectors', summary: 'GitHub, Tetrix and the MCP broker agents reach them through.', modules: ['github', 'github_login', 'github_work', 'tetrix', 'connections'] },
    { id: 'security', title: 'Security', summary: 'The box, egress, the secret scan, permissions, policy, the vault and approvals.', modules: ['sandbox', 'egress', 'scan', 'secrets', 'secretspec', 'permissions', 'policy', 'vault', 'approval', 'devices', 'token'] },
    { id: 'sphere', title: 'Sphere and remote', summary: 'The Obysphere link, the phone relay, the LAN gateway and pushes.', modules: ['sphere', 'relay', 'gateway', 'push', 'triage'] },
    { id: 'platform', title: 'Platform', summary: 'Serving obid: the HTTP API, events, configuration and settings.', modules: ['api', 'app', 'routes', 'events', 'config', 'settings'] },
  ],
  // What Sangam's app keeps itself. "obid": a choice obid owes him (the issue that takes it over); "app": a per-viewer
  // convenience that belongs in the app.
  local: [
    { key: 'agent.default', owner: 'app', note: 'Settings › Agents › New tasks go to: obid\'s `defaultAccount` since #1606; kept here only for an older obid, moved to obid on first sight' },
    { key: 'models.mine', owner: 'app', note: 'Settings › Agents: model and effort per account, obid\'s since #1606; kept here only for an older obid, moved to obid on first sight' },
    { key: 'oby.folders', owner: 'app', note: 'Working folders, per conversation: obid\'s since #1601; kept here only for an older obid, and for what obid refused when moving them' },
    { key: 'oby.sphereNames', owner: 'app', note: 'The names of the Obyspheres joined, as their invite links gave them (#1610); obid keeps only the slug' },
    { key: 'oby.archiveAfter', owner: 'obid', issue: 1580, note: 'Home › Archive completed after…' },
    { key: 'updates.auto', owner: 'app', note: 'About › Update automatically: the mock\'s, in memory; in the Mac app, main keeps it (main/updates.ts)' },
    { key: 'login', owner: 'app', note: 'General › Open at login: Electron main\'s, held in memory until main has a bridge' },
    { key: 'oby.recentFolders', owner: 'app', note: 'The folder picker\'s recent list' },
    { key: 'oby.onboarded', owner: 'app', note: 'First run done' },
    { key: 'oby.setUp', owner: 'app', note: 'The in-conversation setup done' },
    { key: 'oby.welcomed', owner: 'app' },
    { key: 'oby.sidebar*', owner: 'app', note: 'The side panel open or closed, and its width' },
    { key: 'oby.settingsTip*', owner: 'app' },
    { key: 'oby.prompt*', owner: 'app', note: 'Home\'s rotating prompt' },
    { key: 'oby.groupBy', owner: 'app' },
    { key: 'oby.dev.*', owner: 'app', note: 'The Control Center (development only)' },
    { key: 'oby.setting.*', owner: 'app', note: 'useKeptSetting\'s own storage; each key is listed on its own' },
    // useSetting keeps a choice in memory only: Sangam's demo settings, on pages v1 hides behind ?everything.
    ...['accounts', 'connected', 'vault', 'checks.*', 'claude.*', 'codex.*', 'cursor.*', 'mac.*', 'remote.*', 'runner.*', 'share.*', 'voice.*']
      .map(key => ({ key, owner: 'app' as const, note: 'In memory, on a page v1 hides behind ?everything' })),
  ],
  rules: [
    {
      id: 'scan', title: 'Everything outgoing is scanned for secrets',
      text: 'Everything outgoing is scanned for secrets. If the scanner can\'t run, nothing is published.',
      code: [`${OBID}/scan.rs#scan`, `${OBID}/publish.rs#publish`],
      tests: [`${TESTS}/publish_api.rs#a_scanner_that_cant_run_sends_nothing`],
    },
    {
      id: 'agents-cant-publish', title: 'Agents can\'t publish',
      text: 'Agents run with no GitHub login and Oby\'s clone has an invalid push URL. Only Publish pushes.',
      code: [`${OBID}/git.rs#ensure_clone`, `${OBID}/policy.rs#publishes`],
      tests: [`${TESTS}/review_job.rs#the_stub_agent_reviews_the_fixture_in_the_sandbox`],
    },
    {
      id: 'default-branch', title: 'Never the default branch on its own',
      text: 'Until it has learned a repository\'s workflow, Oby never pushes to, merges into or force-pushes the default branch or main, and never force-pushes a branch it didn\'t create.',
      code: [`${OBID}/publish.rs#refusal`],
      tests: [`${TESTS}/merge_api.rs#a_required_approval_goes_to_a_teammate_or_the_owner`],
    },
    {
      id: 'approvals', title: 'Approvals come only from the owner\'s own apps',
      text: 'Approvals are a click in the owner\'s own app, signed by the device\'s key, never from another Oby, a job, GitHub text or anything received.',
      code: [`${OBID}/approval.rs#check`, `${OBID}/approval.rs#check_limit_decision`],
      tests: [`${OBID}/approval.rs#a_publish_signature_and_a_confirm_signature_dont_pass_for_each_other`, `${OBID}/approval.rs#limit_approval_binds_revision_choice_and_account_and_cannot_be_replayed`],
    },
    {
      id: 'untrusted', title: 'Text from outside is data, never instructions',
      text: 'Text from GitHub, web pages, screenshots or other people\'s Obys reaches agents only as quoted files, never in a prompt.',
      code: [`${OBID}/jobs.rs#taint_unless_ours`],
      tests: [`${TESTS}/conversation_harness.rs#a_job_hears_the_owners_words_and_the_agents_only_quoted`, `${TESTS}/conversation_harness.rs#work_planned_after_reading_starts_marked_and_publishes_by_itself`],
    },
    {
      id: 'logins', title: 'Oby never reads or forwards the agents\' logins',
      text: 'Each tool signs in through its own login; a run gets a private copy with no refresh token, and obid never writes a token back.',
      code: [`${OBID}/accounts.rs`],
      tests: [`${TESTS}/conversation_codex.rs#codex_login_copies_have_no_refresh_token`, `${TESTS}/conversation_codex.rs#codex_tokens_never_reach_what_obid_sends`, `${TESTS}/accounts_api.rs#the_default_login_is_never_signed_in_by_obid`],
    },
    {
      id: 'vault', title: 'Vault secrets never reach an agent',
      text: 'Secrets in Oby\'s vault never reach an agent: no prompt, environment, file, argument or log.',
      code: [`${OBID}/vault.rs`, `${OBID}/secrets.rs`],
      tests: [`${TESTS}/vault_api.rs#no_vault_secret_reaches_the_agent`, `${TESTS}/vault_api.rs#a_damaged_vault_gives_the_agent_nothing`],
    },
    {
      id: 'tetrix', title: 'Agents never get Tetrix\'s token',
      text: 'Every agent gets obid\'s own MCP server for Tetrix with the job\'s broker token, and obid forwards only the read tools in TOOLS.',
      code: [`${OBID}/tetrix.rs#TOOLS`],
      tests: [`${TESTS}/tetrix_api.rs#a_conversation_asks_tetrix_for_reads_with_its_turns_token`],
    },
    {
      id: 'add-only', title: 'The API stays add-only',
      text: 'The iPhone and Apple Watch apps stay in Swift on obid\'s API: new keys and values only, never a renamed or removed one, and no new value in an enum they decode as closed.',
      code: [`${APP}/shared/obid-wire.ts`],
      // The Swift client (apps/shared/Runner.swift) compiled against today's obid: what the iPhone and Watch decode.
      tests: [`${TESTS}/mac_app.rs#the_mac_app_follows_obid_and_lists_its_jobs`],
    },
  ],
  internalHosts: ['34-45-0-97.sslip.io', 'tail8de49b'],
  hostScan: ['core/crates', 'apps/electron/src', 'apps/mac', 'apps/ios', 'apps/shared', 'mcp', 'docs', 'scripts', 'install', 'Runner', 'Engine', 'README.md', 'AGENTS.md'],
};
