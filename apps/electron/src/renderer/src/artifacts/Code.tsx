import { useEffect, useRef, useState } from 'react';
import { EditorState } from '@codemirror/state';
import { EditorView, lineNumbers } from '@codemirror/view';
import { HighlightStyle, syntaxHighlighting, type LanguageSupport } from '@codemirror/language';
import { html } from '@codemirror/lang-html';
import { css } from '@codemirror/lang-css';
import { javascript } from '@codemirror/lang-javascript';
import { markdown } from '@codemirror/lang-markdown';
import { json } from '@codemirror/lang-json';
import { tags } from '@lezer/highlight';
import { api, type Artifact } from '../obid';

// The pane's Code tab (docs/contracts/artifacts-app.md, ARTIFACTSAPP-5), as LibreChat's: a tab per
// file of the version, index.html first, its text in CodeMirror 6, read-only until editing comes
// (L3b). CodeMirror is the editor Sandpack's own SandpackCodeEditor wraps; sandpack-react itself
// isn't built in, as it names CodeSandbox's hosted services (scripts/local-only.mjs). A file that
// isn't text (a picture, a font) is named, never shown.

const TEXT = /\.(html?|css|m?jsx?|tsx?|json|md|markdown|svg|txt|csv|xml|ya?ml)$/i;
const isText = (path: string) => TEXT.test(path) || !path.includes('.');
/** The version's files as the tabs list them: index.html first, then by name. */
export const tabsOf = (files: string[]) => [...files].sort((a, b) => (a === 'index.html' ? -1 : b === 'index.html' ? 1 : a.localeCompare(b)));

function language(path: string): LanguageSupport | null {
  const ext = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
  if (ext === 'html' || ext === 'htm' || ext === 'svg' || ext === 'xml') return html();
  if (ext === 'css') return css();
  if (['js', 'mjs', 'jsx'].includes(ext)) return javascript({ jsx: true });
  if (['ts', 'tsx'].includes(ext)) return javascript({ jsx: ext === 'tsx', typescript: true });
  if (ext === 'json') return json();
  if (ext === 'md' || ext === 'markdown') return markdown();
  return null;
}

// on the app's own tokens (tokens.css), dark as the panel is
const theme = EditorView.theme({
  '&': { height: '100%', fontSize: '12.5px', backgroundColor: 'var(--bg)', color: 'var(--ink)' },
  '.cm-scroller': { fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace', lineHeight: '1.55' },
  '.cm-gutters': { backgroundColor: 'var(--bg)', color: 'var(--muted)', border: 'none' },
  '.cm-content': { caretColor: 'var(--ink)' },
  '&.cm-focused': { outline: 'none' },
}, { dark: true });
const highlight = HighlightStyle.define([
  { tag: [tags.tagName, tags.keyword], color: '#ff7ab2' },
  { tag: [tags.attributeName, tags.propertyName], color: '#78c2b3' },
  { tag: [tags.string, tags.attributeValue], color: '#ffa14f' },
  { tag: [tags.comment], color: '#7f8c98', fontStyle: 'italic' },
  { tag: [tags.number, tags.bool, tags.null], color: '#d9c97c' },
  { tag: [tags.heading], fontWeight: '600', color: '#ffffff' },
  { tag: [tags.typeName, tags.className], color: '#acf2e4' },
]);

/** CodeMirror over `text`, read-only. */
function Editor({ path, text }: { path: string; text: string }) {
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!host.current) return;
    const lang = language(path);
    const view = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: text,
        extensions: [lineNumbers(), EditorState.readOnly.of(true), EditorView.editable.of(false), theme, syntaxHighlighting(highlight), ...(lang ? [lang] : [])],
      }),
    });
    return () => view.destroy();
  }, [path, text]);
  return <div ref={host} className="art-code-editor" data-artifact-code={path} />;
}

export function Code({ artifact, version }: { artifact: Artifact; version: number }) {
  const files = tabsOf(artifact.versions.find((v) => v.version === version)?.files ?? []);
  const [picked, setPicked] = useState<string | null>(null);
  const path = picked && files.includes(picked) ? picked : files[0];
  const [read, setRead] = useState<{ key: string; text?: string; error?: string } | null>(null);
  const key = `${artifact.id}/${version}/${path}`;
  useEffect(() => {
    if (!path || !isText(path)) return;
    let gone = false;
    api.artifactText(artifact.id, version, path).then(
      (text) => { if (!gone) setRead({ key, text }); },
      (e: Error) => { if (!gone) setRead({ key, error: e.message }); },
    );
    return () => { gone = true; };
  }, [key]);
  const shown = read?.key === key ? read : null;
  return (
    <div className="art-code">
      <div className="art-files" role="tablist" aria-label="The artifact’s files">
        {files.map((f) => (
          <button key={f} role="tab" aria-selected={f === path} className={`art-file${f === path ? ' is-on' : ''}`} onClick={() => setPicked(f)}>{f}</button>
        ))}
      </div>
      <div className="art-code-body" role="tabpanel" aria-label={path}>
        {!path ? <p className="art-view-note">This version has no files.</p>
          : !isText(path) ? <p className="art-view-note">{path} isn’t text: Preview shows it.</p>
          : shown?.error ? <p className="art-view-note" role="alert">{shown.error}</p>
          : shown?.text !== undefined ? <Editor path={path} text={shown.text} />
          : null}
      </div>
    </div>
  );
}
