// The owner's own tabs in the browser (docs/contracts/browser-app.md, BROWSERAPP-45 to 48): the
// window's, never obid's. Each is a `<webview>` in this one partition, never an agent's
// (`persist:oby-agent-*`) or a box's (`oby-box-*`); main keys it by OWN_THREAD in place of a job, so
// it is never relayed to obid, and nothing but the owner moves it.
export const OWN_PARTITION = 'persist:oby-owner';
export const OWN_THREAD = '';
