// Prism reads this as it loads: it highlights only what it's asked to (never the whole page on
// its own), and never answers a worker's messages. Imported before Prism (highlight/prism.ts).
const g = globalThis as { Prism?: { manual?: boolean; disableWorkerMessageHandler?: boolean } };
g.Prism = { ...g.Prism, manual: true, disableWorkerMessageHandler: true };
export {};
