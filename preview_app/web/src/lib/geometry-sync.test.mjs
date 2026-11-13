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
let generation = 1;
let pendingGeometry = null;
const requested = [], loaded = [], geometryOptions = [], materials = [], visibility = [];
const project = () => ({ project: { name: `Source name ${generation}`, dir: 'C:/mods/B' }, ship: { code: 'PFSB110', name: { en: 'Bourgogne' } }, game: { running: false }, install: { state: 'not_installed', rev: null, files: 0 }, capabilities: { library: true, preview: true, apply: true, simulation: false, reasons: {} }, head: 'current', etag: generation });
globalThis.fetch = async (url, options = {}) => {
  requested.push(url);
  if (url.includes('/geometry/')) {
    const value = Number(url.match(/g(\d+)\.glb/)[1]);
    if (pendingGeometry === value) return new Promise((resolve, reject) => { pendingGeometry = { value, resolve: () => resolve(new Response(new Uint8Array([value]))) }; options.signal?.addEventListener('abort', () => reject(Object.assign(new Error('aborted'), { name: 'AbortError' }))); });
    return new Response(new Uint8Array([value]));
  }
  const value = url.endsWith('/project') ? project() : url.endsWith('/steps') ? { steps: [], head: 'current' } : url.includes('/model?') ? { rev: 'current', geometry: { hash: `hash-${generation}`, url: `/api/v1/projects/B/geometry/g${generation}.glb` }, materials: { hull: { family: 'pbs', params: { generation }, textures: {} } } } : url.includes('/parts?') ? { parts: [{ id: 'hull', parts: [] }, { id: 'gun', parts: [] }] } : url.includes('/textures?') ? { textures: [] } : { ok: true, etag: generation };
  return new Response(JSON.stringify(value), { headers: { 'Content-Type': 'application/json' } });
};
const { boot, disposeWorkbench, resync, useStore } = await import('../store.ts');
const fakeViewer = { loadGeometry(bytes, options) { loaded.push(new Uint8Array(bytes)[0]); geometryOptions.push(options); }, setFx() {}, applyManifest: async (side, manifest) => { materials.push([side, manifest.materials.hull.params.generation]); return { failed: [] }; }, gc() {}, setPartVisible(id, visible) { visibility.push([id, visible]); } };
test('live current snapshot replaces changed geometry and keeps material/selection/source metadata in sync', async () => {
  await boot(() => fakeViewer); assert.deepEqual(loaded, [1]);
  useStore.setState({ selected: 'hull', hidden: { gun: true } });
  generation = 2; await resync();
  assert.deepEqual(loaded, [1, 2]);
  assert.deepEqual(geometryOptions.at(-1), { preserveCamera: true });
  assert.equal(useStore.getState().selected, 'hull');
  assert.equal(useStore.getState().project.project.name, 'Source name 2');
  assert.match(document.title, /Source name 2/);
  assert.deepEqual(useStore.getState().steps, []); assert.equal(useStore.getState().head, 'current');
  assert.deepEqual(materials.slice(-2), [['B', 2], ['A', 2]]);
  assert(visibility.some(([id, visible]) => id === 'gun' && visible === false));
});
test('unchanged geometry across repeated resyncs is not downloaded or reloaded again', async () => {
  const before = requested.filter((url) => url.includes('/geometry/')).length;
  await resync(); await resync();
  assert.equal(requested.filter((url) => url.includes('/geometry/')).length, before);
  assert.deepEqual(loaded, [1, 2]);
});
test('late geometry from a superseded resync cannot overwrite a newer snapshot', async () => {
  generation = 3; pendingGeometry = 3; const old = resync();
  for (let i = 0; i < 20 && typeof pendingGeometry === 'number'; i++) await new Promise((resolve) => setTimeout(resolve, 1));
  assert.equal(typeof pendingGeometry, 'object'); const finishOld = pendingGeometry.resolve;
  generation = 4; await resync(); finishOld(); await old;
  assert.deepEqual(loaded, [1, 2, 4]);
});
test('unmount cancels outstanding geometry and prevents viewer updates', async () => {
  generation = 5; pendingGeometry = 5; const pending = resync();
  for (let i = 0; i < 20 && typeof pendingGeometry === 'number'; i++) await new Promise((resolve) => setTimeout(resolve, 1));
  disposeWorkbench(); await pending;
  assert.deepEqual(loaded, [1, 2, 4]);
});
test.after(() => disposeWorkbench());
