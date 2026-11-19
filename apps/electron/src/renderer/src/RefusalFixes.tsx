import { useState } from 'react';
import type { Fix } from '../../shared/obid-wire';
import { signInFromChat } from './SettingsView';
import { Install } from './SetupCard';
import { AGENTS, agentOf, type Agent } from './view';

// Under Oby's words that a task it was asked for couldn't start on the account named
// (docs/contracts/accounts.md, ACCOUNTS-37): what obid says the owner can do. Install, for a CLI
// that isn't on this Mac, shows the vendor's command to copy, as first run's Tools step does
// (FIRSTRUN-2); Sign in, for an account that isn't signed in, opens its sign-in as the line over
// the box does (CantRunLine). An action this app doesn't know shows nothing.
export default function RefusalFixes({ fixes }: { fixes: Fix[] }) {
  const [installing, setInstalling] = useState<Agent | null>(null);
  const shown = fixes.filter((f) => (f.action === 'install' && f.agent in AGENTS) || (f.action === 'signIn' && f.account));
  if (!shown.length) return null;
  return (
    <div className="refusal-fixes">
      {shown.map((f) => {
        const name = agentOf(f.agent).name;
        return f.action === 'install' ? (
          <button key={`install:${f.agent}`} type="button" className="cant-run-action" aria-expanded={installing === f.agent} aria-label={`Install ${name}`}
            onClick={() => setInstalling(installing === f.agent ? null : (f.agent as Agent))}>Install</button>
        ) : (
          <button key={`signIn:${f.account}`} type="button" className="cant-run-action" aria-label={`Sign in to ${name}`} onClick={() => signInFromChat(f.account)}>Sign in</button>
        );
      })}
      {installing && <Install agent={installing} />}
    </div>
  );
}
