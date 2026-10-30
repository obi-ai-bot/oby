import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { config } from '../brain.config.js';
import { generateBrain } from './index.js';

// `npm run generate`: the same JSON the page reads, written to tools/brain/.brain/brain.json (out of git), and the checks.
const data = generateBrain(config);
const dir = join(config.root, 'tools/brain/.brain');
mkdirSync(dir, { recursive: true });
writeFileSync(join(dir, 'brain.json'), JSON.stringify(data, null, 2));
console.log(`brain: ${Object.entries(data.stats).map(([k, v]) => `${v} ${k}`).join(', ')} (${data.durationMs} ms)`);
for (const c of data.checks.filter(c => c.kind !== 'local-owed')) console.log(`  ${c.kind}: ${c.message}${c.source ? ` (${c.source.file}:${c.source.line})` : ''}`);
