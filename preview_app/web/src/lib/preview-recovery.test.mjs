import { registerHooks } from 'node:module';
import test from 'node:test';
import assert from 'node:assert/strict';
registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier)) return nextResolve(specifier + '.ts', context);
  return nextResolve(specifier, context);
} });
globalThis.window = { WM_API_BASE: '', WM_NO_SSE: true, setInterval, clearInterval };
globalThis.location = { search: '?project=B' };
globalThis.localStorage = { getItem: (key) => key === 'wm.quality' ? 'low' : null };
globalThis.document = { title: '', documentElement: {} };
let preview = false, generation = 1, pendingGeometry = null, failGeometry = false, pendingMaterial = null, failMaterial = false;
let created = 0;
const loaded = [], effects = [], simulations = [], materials = [];
globalThis.fetch = async (url, options = {}) => {
  if (url.includes('/geometry/')) {
    const value = Number(url.match(/g(\d+)\.glb/)[1]);
    if (failGeometry) return new Response(JSON.stringify({ error: { code: 'no_model', message: 'Model data unavailable' } }), { status: 404, headers: { 'Content-Type': 'application/json' } });
    if (pendingGeometry === true) return new Promise((resolve, reject) => {
      pendingGeometry = { release: () => resolve(new Response(new Uint8Array([value]))), aborted: false };
      const pending = pendingGeometry;
      options.signal?.addEventListener('abort', () => { pending.aborted = true; reject(Object.assign(new Error('aborted'), { name: 'AbortError' })); });
    });
    return new Response(new Uint8Array([value]));
  }
  const value = url.endsWith('/project') ? { project: { name: 'Repairable model', dir: 'C:/mods/B' }, ship: { code: 'PFSB110', param: 'PFSB110_Bourgogne' }, fx: { preview: { generation } }, game: { running: false }, install: { state: 'not_installed' }, capabilities: { preview, simulation: true, reasons: {} } } : url.endsWith('/steps') ? { steps: [], head: 'current' } : url.includes('/model?') ? { rev: 'current', geometry: { hash: `g${generation}`, url: `/api/v1/projects/B/geometry/g${generation}.glb` }, materials: {} } : url.includes('/parts?') ? { parts: [{ id: 'hull' }] } : url.includes('/textures?') ? { textures: [] } : url.endsWith('/sim') ? { ship: { code: 'PFSB110' }, mounts: {} } : { ok: true, etag: 1 };
  return new Response(JSON.stringify(value), { headers: { 'Content-Type': 'application/json' } });
};
const { boot, disposeWorkbench, resync, useStore } = await import('../store.ts');
const factory = () => { created++; return {
  loadGeometry(bytes, options) { loaded.push({ generation: new Uint8Array(bytes)[0], options }); },
  setFx(value) { effects.push(value); }, setSimData(value) { simulations.push(value); },
  applyManifest: async (side, manifest) => {
    materials.push([side, manifest.geometry.hash]);
    if (failMaterial) throw new Error('Material data unavailable');
    if (side === 'B' && pendingMaterial === true) await new Promise((resolve) => { pendingMaterial = resolve; });
    return { failed: [] };
  },
  gc() {}, setPartVisible() {},
}; };
async function reset() {
  disposeWorkbench(); preview = false; generation = 1; pendingGeometry = null; failGeometry = false; pendingMaterial = null; failMaterial = false;
  created = 0; loaded.length = effects.length = simulations.length = materials.length = 0;
  await boot(factory); assert.equal(created, 0); assert.equal(useStore.getState().phase, 'ready');
}
async function waitForGeometry() {
  for (let i = 0; i < 100 && pendingGeometry === true; i++) await new Promise((resolve) => setTimeout(resolve, 1));
  assert.equal(typeof pendingGeometry, 'object');
  return pendingGeometry;
}
test('a repaired same-ID project creates its viewer, frames geometry and initializes effects/materials/simulation', async () => {
  await reset(); preview = true; await resync();
  assert.equal(created, 1); assert.deepEqual(loaded, [{ generation: 1, options: { preserveCamera: false } }]);
  assert.deepEqual(effects.at(-1), { generation: 1 }); assert.deepEqual(materials, [['B', 'g1'], ['A', 'g1']]);
  assert.equal(useStore.getState().parts[0].id, 'hull'); assert.equal(simulations.at(-1).ship.code, 'PFSB110');
  assert.equal(useStore.getState().phase, 'ready'); assert.equal(useStore.getState().fatal, null);
  await resync(); assert.equal(created, 1); assert.equal(loaded.length, 1); assert.equal(simulations.length, 1);
});
test('a newer source wins while the first restored model download is pending', async () => {
  await reset(); preview = true; pendingGeometry = true; const first = resync(); const pending = await waitForGeometry();
  generation = 2; await resync(); pending.release(); await first;
  assert.equal(created, 1); assert.deepEqual(loaded.map((value) => value.generation), [2]);
  assert.equal(useStore.getState().phase, 'ready'); assert.equal(simulations.at(-1).ship.code, 'PFSB110');
});
test('losing preview capability cancels committing a pending recovery and allows a later recovery', async () => {
  await reset(); preview = true; pendingGeometry = true; const first = resync(); const pending = await waitForGeometry();
  preview = false; await resync(); pending.release(); await first;
  assert.deepEqual(loaded, []); assert.equal(useStore.getState().phase, 'ready');
  preview = true; generation = 2; await resync(); assert.equal(created, 1);
  assert.deepEqual(loaded.map((value) => value.generation), [2]); assert.equal(useStore.getState().phase, 'ready');
});
test('a later refresh completes recovery even after geometry loaded while first materials are pending', async () => {
  await reset(); preview = true; pendingMaterial = true; const first = resync();
  for (let i = 0; i < 100 && pendingMaterial === true; i++) await new Promise((resolve) => setTimeout(resolve, 1));
  assert.equal(typeof pendingMaterial, 'function'); const release = pendingMaterial;
  assert.equal(loaded.length, 1); assert.equal(useStore.getState().phase, 'loading');
  await resync(); release(); await first;
  assert.equal(created, 1); assert.equal(loaded.length, 1); assert.equal(useStore.getState().phase, 'ready');
  assert.equal(simulations.at(-1).ship.code, 'PFSB110');
});
test('unmount aborts the restored model request and never updates the removed viewer', async () => {
  await reset(); preview = true; pendingGeometry = true; const first = resync(); const pending = await waitForGeometry();
  disposeWorkbench(); await first;
  assert.equal(pending.aborted, true); assert.deepEqual(loaded, []);
});
test('a failed first restored model remains visibly failed until a successful source refresh', async () => {
  await reset(); preview = true; failGeometry = true; await resync();
  assert.equal(useStore.getState().phase, 'fatal'); assert.equal(useStore.getState().fatal.message, 'Model data unavailable');
  failGeometry = false; await resync(); assert.equal(created, 1);
  assert.equal(useStore.getState().phase, 'ready'); assert.equal(useStore.getState().fatal, null); assert.equal(loaded.length, 1);
});
test('a failed recovery after geometry loaded clears its error after materials recover', async () => {
  await reset(); preview = true; failMaterial = true; await resync();
  assert.equal(loaded.length, 1); assert.equal(useStore.getState().phase, 'fatal');
  assert.equal(useStore.getState().fatal.message, 'Material data unavailable');
  failMaterial = false; await resync(); assert.equal(loaded.length, 1);
  assert.equal(useStore.getState().phase, 'ready'); assert.equal(useStore.getState().fatal, null);
});
test.after(() => disposeWorkbench());
