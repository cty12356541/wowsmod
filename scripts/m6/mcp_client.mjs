// 最小 MCP stdio 客户端 + M6 验收任务脚本(模拟一个只按 AGENTS.md / help 主题 / next 提示行事的 AI)。
// 用法: node mcp_client.mjs <wowsmod.exe> <工作目录> [--list-only]
// 统计: 读进上下文的“说明文字”(AGENTS.md、initialize.instructions、tools/list、help 主题)与“命令结果”分开计数(字符数)。
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline';

const [exe, work, flag] = process.argv.slice(2);
if (!exe || !work) { console.error('usage: node mcp_client.mjs <wowsmod.exe> <work dir> [--list-only]'); process.exit(2); }
fs.mkdirSync(work, { recursive: true });
const logf = path.join(work, 'mcp_transcript.jsonl');
fs.writeFileSync(logf, '');
const stats = { doc_chars: 0, doc_items: [], output_chars: 0, calls: 0, errors: 0, notifications: [] };
const doc = (label, s) => { stats.doc_chars += [...s].length; stats.doc_items.push({ label, chars: [...s].length }); };
const outp = (s) => { stats.output_chars += [...s].length; };

const srv = spawn(exe, ['mcp'], { stdio: ['pipe', 'pipe', 'inherit'] });
const rl = readline.createInterface({ input: srv.stdout });
let nextId = 1; const pending = new Map();
rl.on('line', (line) => {
  fs.appendFileSync(logf, JSON.stringify({ dir: 'in', msg: line.length > 4000 ? line.slice(0, 4000) + '…' : line }) + '\n');
  const m = JSON.parse(line);
  if (m.id !== undefined && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  else if (m.method) stats.notifications.push(m.method);
});
function rpc(method, params) {
  const id = nextId++; const msg = { jsonrpc: '2.0', id, method, params };
  fs.appendFileSync(logf, JSON.stringify({ dir: 'out', msg }) + '\n');
  srv.stdin.write(JSON.stringify(msg) + '\n');
  return new Promise((res) => pending.set(id, res));
}
function notify(method, params) { srv.stdin.write(JSON.stringify({ jsonrpc: '2.0', method, params }) + '\n'); }
async function call(name, args, isDoc = false) {
  stats.calls++;
  const r = await rpc('tools/call', { name, arguments: args });
  if (r.error) throw new Error(JSON.stringify(r.error));
  const res = r.result; if (res.isError) stats.errors++;
  let text = '';
  for (const c of res.content) {
    if (c.type === 'text') text += c.text;
    if (c.type === 'image') { const f = path.join(work, `render_${stats.calls}.png`); fs.writeFileSync(f, Buffer.from(c.data, 'base64')); text += `[image ${f}]`; stats.image = f; }
  }
  if (isDoc) doc(`${name} ${JSON.stringify(args)}`, text); else outp(text);
  console.log(`> ${name} ${JSON.stringify(args).slice(0, 160)}  ${res.isError ? 'ERROR' : 'ok'}  (${[...text].length} 字)`);
  return { text, res, json: (() => { try { return JSON.parse(text); } catch { return null; } })() };
}

const agents = fs.existsSync(path.join(work, 'AGENTS.md')) ? fs.readFileSync(path.join(work, 'AGENTS.md'), 'utf8') : '';
if (agents) doc('AGENTS.md', agents);
const init = await rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'm6-test', version: '1' } });
doc('initialize.instructions', init.result.instructions || '');
notify('notifications/initialized', {});
const tl0 = await rpc('tools/list', {});
doc('tools/list(初始)', JSON.stringify(tl0.result.tools));
const initialTools = tl0.result.tools.map((t) => t.name);
console.log('初始工具:', initialTools.join(', '));
if (flag === '--list-only') { srv.stdin.end(); console.log(JSON.stringify(stats)); process.exit(0); }

// ---- 任务: 把勃艮第船体改成深海蓝并让烟囱发光 ----
const P = path.join(work, 'deepsea_ai');
const st = await call('wowsmod_status', {});
await call('wowsmod_help', { topic: 'start' }, true);
await call('wowsmod_help', { topic: 'project' }, true);
await call('wowsmod_help', { topic: 'recolor' }, true);
await call('wowsmod_help', { topic: 'glow' }, true);
if (fs.existsSync(P)) fs.rmSync(P, { recursive: true, force: true });
await call('wowsmod_run', { args: ['mod', 'init', P, '--name', 'DeepSeaAI', '--ship', 'FSB510_Bourgogne', '--mode', 'sdk'] });
const imp = await call('wowsmod_run', { args: ['mod', 'import-sdk', P] });
if (imp.res.isError) throw new Error('import-sdk 失败: ' + imp.text);
const pa = await call('wowsmod_run', { args: ['port-albedo', '#1E3F66'] });
const toColor = pa.json.to_color;
// 展开 texture 组, 观察 list_changed 与新工具列表
await call('wowsmod_expand', { group: 'texture' });
const tl1 = await rpc('tools/list', {});
const added = tl1.result.tools.filter((t) => !initialTools.includes(t.name));
doc('tools/list(展开 texture 后新增部分)', JSON.stringify(added));
// AI 用自己的文件列表能力找 *_a.dds(计入命令结果)
const shipDir = path.join(P, 'assets', 'res_mods', 'PnFMods', 'DeepSeaAI', 'FSB510_Bourgogne');
const all = []; (function walk(d) { for (const e of fs.readdirSync(d, { withFileTypes: true })) { const p = path.join(d, e.name); if (e.isDirectory()) walk(p); else all.push(e.name); } })(shipDir);
outp(all.join('\n'));
const re = /^(FSB025_Bourgogne_1945_(Hull|DeckHouse)_a|F[GDR][A-Z]?\d+.*_a)\.dds$/;
const targets = [...new Set(all.filter((n) => re.test(n)))];
for (const t of targets) {
  await call('tex_recolor', { tex: t, project: P, to_color: toColor, exclude_hue: '0:65', soft: 0.04 });
  const mg = t.replace(/_a\.dds$/, '_mg.dds');
  if (!/DeckHouse_a/.test(t) && all.includes(mg)) await call('tex_channel', { tex: mg, project: P, channel: 'b', set: 0 });
}
const glb = path.join(work, 'MidBack.glb');
await call('wowsmod_run', { args: ['export', 'content/gameplay/france/ship/battleship/FSB025_Bourgogne_1945/FSB025_Bourgogne_1945_MidBack.geometry', '-o', glb] });
await call('glow', { project: P, mg: 'FSB025_Bourgogne_1945_DeckHouse_mg.dds', diffuse: 'FSB025_Bourgogne_1945_DeckHouse_a.dds', color: '#2E9BFF', glb, material: 'DeckHouse_skinned', mfm: 'FSB025_Bourgogne_1945_DeckHouse_skinned', power: 0.25 });
await call('wowsmod_render', { project: P, compare: true, view: 'quarter', width: 1200 });
await call('wowsmod_run', { args: ['install', P, '--dry-run'] });
srv.stdin.end();
stats.initial_tools = initialTools; stats.added_tools = added.map((t) => t.name); stats.targets = targets.length; stats.to_color = toColor; stats.project = P;
fs.writeFileSync(path.join(work, 'mcp_stats.json'), JSON.stringify(stats, null, 2));
console.log(JSON.stringify({ doc_chars: stats.doc_chars, output_chars: stats.output_chars, calls: stats.calls, errors: stats.errors, notifications: stats.notifications, image: stats.image, doc_items: stats.doc_items }, null, 1));
