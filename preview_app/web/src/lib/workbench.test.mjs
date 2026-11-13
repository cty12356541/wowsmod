import { registerHooks } from 'node:module';
import test from 'node:test';
import assert from 'node:assert/strict';
registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier)) return nextResolve(specifier + '.ts', context);
  return nextResolve(specifier, context);
} });
globalThis.window = { WM_API_BASE: '', WM_NO_SSE: true, setInterval, clearInterval };
globalThis.location = { search: '?project=B' };
globalThis.localStorage = { getItem: (key) => key === 'wm.quality' ? 'low' : null, setItem() {} };
globalThis.document = { title: '', documentElement: {} };
const capabilities = { library: true, preview: false, apply: true, undo: false, restore: false, simulation: false, reasons: { preview: 'Global assets have no ship model', undo: 'Undo unavailable', restore: 'Restore unavailable', simulation: 'No simulation data' } };
const project = { project: { name: 'Same mod', dir: 'C:/mods/B' }, ship: { code: '', name: {} }, game: { running: false }, install: { state: 'not_installed', rev: null, files: 0 }, capabilities, head: 'current', etag: 1 };
const plan = { previous: [{ name: 'Same mod', project_dir: 'C:/mods/A' }], selected: { name: 'Same mod', project_dir: 'C:/mods/B' }, target_client: 'D:/Game', build: '123', files: 4, warnings: [] };
const calls = []; let realResult = { ok: true, ...plan }; let failApply = false;
globalThis.fetch = async (url, options = {}) => {
  const body = options.body ? JSON.parse(options.body) : null; calls.push({ url, body });
  if (url.endsWith('/geometry/test.glb')) return new Response(new Uint8Array([1, 2]));
  if (url.endsWith('/apply') && failApply) return new Response(JSON.stringify({ ok: false, error: { code: 'game_running', message: 'Close the running game' } }), { status: 409, headers: { 'Content-Type': 'application/json' } });
  const value = url.endsWith('/project') ? project : url.endsWith('/steps') ? { steps: [], head: 'current' } : url.includes('/model?') ? { rev: 'current', materials: {}, geometry: { url: '/api/v1/projects/B/geometry/test.glb' } } : url.includes('/parts?') ? { parts: [] } : url.includes('/textures?') ? { textures: [] } : url.endsWith('/apply') ? body.dry_run ? { ok: true, ...plan, dry_run: true } : realResult : url.includes('/jobs/') ? { state: 'done', message: 'Written', result: plan } : { ok: true, etag: 1 };
  return new Response(JSON.stringify(value), { headers: { 'Content-Type': 'application/json' } });
};
const { actions, boot, disposeWorkbench, useStore } = await import('../store.ts');
const applyCalls = () => calls.filter((c) => c.url.endsWith('/apply'));
test('global project loads metadata and catalog actions without creating a fake viewer', async () => {
  let created = false; await boot(() => { created = true; throw Error('must not create'); });
  assert.equal(created, false); assert.equal(useStore.getState().phase, 'ready');
  assert.equal(useStore.getState().project.project.dir, 'C:/mods/B');
});
test('dry run opens a review dialog and selecting project never applies automatically', async () => {
  assert.equal(applyCalls().length, 0);
  await actions.prepareApply();
  assert.equal(useStore.getState().dialog.kind, 'apply');
  assert.equal(useStore.getState().applyPlan.target_client, 'D:/Game');
  assert.deepEqual(applyCalls().map((c) => c.body), [{ dry_run: true }]);
});
test('explicit confirmation supports immediate apply result and keeps validation wording truthful', async () => {
  await actions.confirmApply();
  assert.deepEqual(applyCalls().at(-1).body, { dry_run: false });
  assert.equal(useStore.getState().dialog, null);
  assert.match(useStore.getState().applyMessage, /实机验收|in-game validation/);
});
test('job_id apply result polls the scoped job until completion', async () => {
  realResult = { ok: true, job_id: 'job-B' }; await actions.prepareApply(); await actions.confirmApply();
  assert.equal(useStore.getState().job.id, 'job-B');
  await new Promise((resolve) => setTimeout(resolve, 370));
  assert(calls.some((c) => c.url === '/api/v1/projects/B/jobs/job-B'));
  assert.equal(useStore.getState().job.state, 'done');
});
test('running-game errors remain visible and never submit a real apply', async () => {
  failApply = true; const before = applyCalls().length; await actions.prepareApply();
  assert.equal(useStore.getState().applyPlan, null);
  assert.equal(useStore.getState().applyMessage, 'Close the running game');
  await actions.confirmApply(); assert.equal(applyCalls().length, before + 1); failApply = false;
});
test('unsupported undo/restore do not dispatch requests', async () => {
  const before = calls.length; await actions.revert('s1'); await actions.restore('s1');
  assert.equal(calls.length, before);
});
test('current snapshot without history still loads its actual model', async () => {
  disposeWorkbench(); capabilities.preview = true;
  const fakeViewer = { loadGeometry() {}, setFx() {}, applyManifest: async () => ({ failed: [] }), gc() {} };
  await boot(() => fakeViewer);
  assert.equal(useStore.getState().phase, 'ready');
  assert.equal(useStore.getState().steps.length, 0);
  assert(calls.some((c) => c.url.includes('/model?rev=current')));
});
test.after(() => disposeWorkbench());
