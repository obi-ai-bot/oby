// The app's page over Chromium's DevTools protocol, for scripts/vm-test.sh: `node cdp.mjs <port>
// <expression>` prints what the expression gives, `node cdp.mjs <port> --screenshot <file.png>`
// saves a picture of the page.
import { writeFileSync } from 'node:fs';

const [port, what, file] = process.argv.slice(2);
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const ws = new WebSocket(targets.find((t) => t.type === 'page').webSocketDebuggerUrl);
await new Promise((open, fail) => { ws.onopen = open; ws.onerror = fail; });
const [method, params] = what === '--screenshot'
  ? ['Page.captureScreenshot', { format: 'png' }]
  : ['Runtime.evaluate', { expression: what, returnByValue: true, awaitPromise: true }];
ws.send(JSON.stringify({ id: 1, method, params }));
ws.onmessage = (m) => {
  const { id, result, error } = JSON.parse(m.data);
  if (id !== 1) return;
  if (error || result?.exceptionDetails) { console.error(JSON.stringify(error ?? result.exceptionDetails)); process.exit(1); }
  if (what === '--screenshot') writeFileSync(file, Buffer.from(result.data, 'base64'));
  else console.log(typeof result.result.value === 'string' ? result.result.value : JSON.stringify(result.result.value));
  process.exit(0);
};
