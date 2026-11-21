import { ChevronRight, Frame } from 'lucide-react';
import type { Artifact } from '../obid';
import { agentOf } from '../view';
import { kindName, openArtifact, useOpenArtifact } from './store';
import './artifacts.css';

// An artifact in the Oby message that tells of it (Sangam's board 1a, the Oby Artifacts canvas;
// docs/contracts/artifacts-app.md, ARTIFACTSAPP-4, 15, 16): its first screen on top, and a click opens
// it beside the conversation.
export function ArtifactCard({ artifact, agent }: { artifact: Artifact; agent?: string }) {
  const open = useOpenArtifact();
  const latest = artifact.versions.at(-1)?.version ?? 1;
  const by = agent ? ` · made by ${agentOf(agent).name}` : '';
  const on = open?.artifact.id === artifact.id;
  return (
    <button className={`art-card${on ? ' is-on' : ''}`} onClick={() => openArtifact(artifact)} aria-pressed={on} data-artifact={artifact.id}>
      <span className="art-thumb" aria-hidden="true"><span className="art-thumb-a" /><span className="art-thumb-b"><span /><span /></span></span>
      <span className="art-card-row">
        <Frame size={16} strokeWidth={1.6} aria-hidden="true" />
        <span className="art-card-text">
          <span className="art-card-title">{artifact.title}</span>
          <span className="art-card-meta">Oby artifact · {kindName(artifact.kind)} · v{latest}{by}</span>
        </span>
        <ChevronRight size={14} strokeWidth={1.6} aria-hidden="true" />
      </span>
    </button>
  );
}
