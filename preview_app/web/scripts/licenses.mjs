// Walks the production dependency tree and fails on any non-permissive license.
import fs from 'node:fs'; import path from 'node:path';
const OK = /^(MIT|ISC|BSD-2-Clause|BSD-3-Clause|0BSD|Apache-2\.0|BlueOak-1\.0\.0|CC0-1\.0|Unlicense)$/;
const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const seen = new Map(); const bad = [];
function visit(name, from) {
  let dir = from, pj;
  while (true) { const p = path.join(dir, 'node_modules', name, 'package.json');
    if (fs.existsSync(p)) { pj = p; break; } const up = path.dirname(dir); if (up === dir) return; dir = up; }
  const pkg = JSON.parse(fs.readFileSync(pj, 'utf8')); const key = pkg.name + '@' + pkg.version;
  if (seen.has(key)) return;
  let lic = pkg.license || (pkg.licenses || []).map(l => l.type || l).join(' OR ') || 'UNKNOWN';
  if (typeof lic === 'object') lic = lic.type;
  seen.set(key, lic);
  const parts = String(lic).replace(/[()]/g, '').split(/\s+(?:OR|AND)\s+/);
  const okay = /\sAND\s/.test(lic) ? parts.every(x => OK.test(x)) : parts.some(x => OK.test(x));
  if (!okay) bad.push(key + ' ' + lic);
  for (const d of Object.keys(pkg.dependencies || {})) visit(d, path.dirname(pj));
}
const top = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
for (const d of Object.keys(top.dependencies)) visit(d, root);
const counts = {}; for (const l of seen.values()) counts[l] = (counts[l] || 0) + 1;
console.log(`${seen.size} runtime packages:`, counts);
if (bad.length) { console.error('NOT ALLOWED:\n' + bad.join('\n')); process.exit(1); }
console.log('licenses OK');
