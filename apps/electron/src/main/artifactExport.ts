import { app, dialog, type BrowserWindow } from 'electron';
import { writeFile } from 'node:fs/promises';
import { send } from './obid';
import { ipc, type Trusted } from './ipc';
import { oneOf } from '../shared/decode';
import { artifactId, exportArtifact, fromOne } from './previewRules';

// Download, from the artifact pane's Export (docs/contracts/artifacts-app.md, ARTIFACTSAPP-11;
// Sangam's board 1c): a version as obid exports it (`GET /v1/artifacts/{id}/export/{version}?as=`,
// ARTIFACTS-9 and 10), a project archive (.zip) or one standalone HTML file, saved where the owner
// picks in main's own dialog, byte for byte as obid gave it. The page sends only the artifact, the
// version and which; main reads obid with its token and writes only the path the dialog answered.

/** The `artifact-export` channel. */
export function exporting(trusted: Trusted, window: () => BrowserWindow | null) {
  ipc(trusted).handle('artifact-export', [artifactId, fromOne, oneOf('zip', 'html')], (_e, id, version, as) => exportArtifact(id, version, as, {
    fetch: (path) => send('GET', path, undefined),
    ask: async (defaultPath) => {
      const win = window();
      const r = win && !win.isDestroyed()
        ? await dialog.showSaveDialog(win, { defaultPath, buttonLabel: 'Save', properties: ['createDirectory', 'showOverwriteConfirmation'] })
        : await dialog.showSaveDialog({ defaultPath });
      return r.canceled || !r.filePath ? null : r.filePath;
    },
    write: (path, bytes) => writeFile(path, bytes),
    downloads: app.getPath('downloads'),
  }), null);
}
