import { useState, type DragEvent } from 'react';
import { FileUp, X } from 'lucide-react';
import { IMPORT } from '../../../shared/words';
import { IMPORTABLE, IMPORT_MOST } from '../../../shared/artifactImport';
import { importFrom } from './store';
import './artifacts.css';

// A file dropped into a conversation becomes an artifact (docs/contracts/artifacts-app.md,
// ARTIFACTSAPP-12; obid's import, ARTIFACTS-11): a project archive (.zip), a page or a document.
// A drop on the thread offers it; the owner's own click on Import sends its bytes, which main signs by
// that button's label (WORDS-21). The message box keeps its own drops, as attachments. Imported, it
// opens beside the conversation, and the owner's first words with it open make it the conversation's
// (ARTIFACTS-12).

const files = (e: DragEvent) => [...(e.dataTransfer?.items ?? [])].some((i) => i.kind === 'file');

/** The thread's drop handlers, and the bar that asks, for a conversation obid keeps. */
export function useImportDrop(on: boolean) {
  const [offered, setOffered] = useState<{ file: File; error?: string; busy?: boolean } | null>(null);
  const [over, setOver] = useState(false);
  const handlers = on ? {
    onDragOver: (e: DragEvent) => { if (files(e)) { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; setOver(true); } },
    onDragLeave: (e: DragEvent) => { if (e.currentTarget === e.target) setOver(false); },
    onDrop: (e: DragEvent) => {
      if (!files(e)) return;
      e.preventDefault();
      setOver(false);
      const file = [...e.dataTransfer.files].find((f) => IMPORTABLE.test(f.name));
      if (file) setOffered({ file, ...(file.size > IMPORT_MOST ? { error: `${file.name} is more than 257 MB, the most an artifact may be.` } : {}) });
    },
  } : {};
  const send = async () => {
    if (!offered || offered.error) return;
    setOffered({ ...offered, busy: true });
    const why = await importFrom(new Uint8Array(await offered.file.arrayBuffer()));
    setOffered(why ? { file: offered.file, error: why } : null);
  };
  const bar = offered && (
    <div className="art-import" role="group" aria-label={`Import ${offered.file.name} as an artifact`} data-artifact-import>
      <FileUp size={15} strokeWidth={1.6} aria-hidden="true" />
      <span className="art-import-text">{offered.error ?? <>Import <b>{offered.file.name}</b> as an artifact?</>}</span>
      {!offered.error && <button className="tc-option is-sm" disabled={offered.busy} onClick={() => void send()}>{IMPORT}</button>}
      <button className="icon-button is-28" aria-label="Don’t import it" onClick={() => setOffered(null)}><X size={13} strokeWidth={2} /></button>
    </div>
  );
  return { handlers, bar, over };
}
