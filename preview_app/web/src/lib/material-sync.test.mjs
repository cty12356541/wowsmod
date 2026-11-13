import { registerHooks } from 'node:module';
import test from 'node:test';
import assert from 'node:assert/strict';
registerHooks({ resolve(specifier, context, nextResolve) {
  const result = nextResolve(specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier) ? specifier + '.ts' : specifier, context);
  return specifier.endsWith('.json') ? { ...result, importAttributes: { type: 'json' } } : result;
} });
globalThis.window = { WM_API_BASE: '', WM_NO_SSE: true, setInterval, clearInterval, createImageBitmap: true };
globalThis.createImageBitmap = async () => ({ width: 1, height: 1, close() {} });
globalThis.location = { search: '?project=recovery' };
globalThis.localStorage = { getItem: (key) => key === 'wm.quality' ? 'low' : null };
globalThis.document = { title: '', documentElement: {} };
let preview = false, generation = 1, delayedTexture = '', releaseTexture = null;
const geometry = [];
function manifest(side, version) {
  return { rev: side === 'a' ? 'base' : 'current', geometry: { hash: `g${version}`, url: `/api/v1/projects/recovery/geometry/g${version}.glb` }, camo: { color0: [version / 10, 0, 0] }, materials: {
    hull: { family: 'pbs', params: {}, textures: { a: { name: 'paint', hash: `tex-${side}${version}`, url: `/api/v1/projects/recovery/blob/${side}${version}.png`, width: 1, height: 1 } } },
  } };
}
globalThis.fetch = async (url) => {
  if (url.includes('/blob/')) {
    if (url.endsWith(`/${delayedTexture}.png`)) return new Promise((resolve) => { releaseTexture = () => resolve(new Response(new Uint8Array([1]))); });
    return new Response(new Uint8Array([generation]));
  }
  if (url.includes('/geometry/')) return new Response(new Uint8Array([generation]));
  const value = url.endsWith('/project') ? { project: { name: `Source ${generation}`, dir: 'C:/review' }, ship: { code: 'X' }, game: { running: false }, install: { state: 'not_installed' }, capabilities: { preview, simulation: false, reasons: {} } }
    : url.endsWith('/steps') ? { steps: [], head: 'current' }
    : url.includes('/model?') ? manifest(url.includes('rev=base') ? 'a' : 'b', generation)
    : url.includes('/parts?') ? { parts: [{ id: 'hull' }] }
    : url.includes('/textures?') ? { textures: [] } : { ok: true, etag: 1 };
  return new Response(JSON.stringify(value), { headers: { 'Content-Type': 'application/json' } });
};
const THREE = await import('three');
const { Viewer } = await import('../engine/viewer.ts');
const { clearTexCache } = await import('./tex.ts');
const { disableLive } = await import('./api.ts');
const { boot, disposeWorkbench, resync, useStore } = await import('../store.ts');
let target;
async function reset(texture) {
  disposeWorkbench(); clearTexCache(); preview = false; generation = 1; delayedTexture = texture; releaseTexture = null; geometry.length = 0;
  // Skip only WebGL/scene construction: use real material construction,
  // applyManifest, mesh flags, change detection, getTex and texture GC.
  const mesh = new THREE.Mesh(); mesh.userData = { mfm: 'hull', part: 'hull' };
  target = Object.assign(Object.create(Viewer.prototype), {
    Q: { tex: 1 }, U: { uCamoCol: { value: new THREE.Color() } }, pal: { envI: 1 }, look: 'studio', channel: 0,
    matCache: new Map(), sets: { A: {}, B: {} }, meshes: [mesh], renderer: { shadowMap: {} },
    loadGeometry(bytes, options) { geometry.push({ version: new Uint8Array(bytes)[0], options }); }, setFx() {}, poke() {}, setPartVisible() {},
  });
  await boot(() => target); disableLive(); preview = true;
}
async function waitForTexture() {
  for (let i = 0; i < 100 && !releaseTexture; i++) await new Promise((resolve) => setTimeout(resolve, 1));
  assert.equal(typeof releaseTexture, 'function', 'real getTex must reach the deferred PNG request');
  return releaseTexture;
}
test('late initial recovery texture cannot overwrite newer geometry and B/A materials', async () => {
  await reset('b1'); const old = resync(); const release = await waitForTexture();
  generation = 2; await resync();
  assert.equal(target.meshes[0].material.map.userData.hash, 'tex-b2');
  release(); await old;
  assert.equal(useStore.getState().project.project.name, 'Source 2'); assert.equal(useStore.getState().phase, 'ready');
  assert.deepEqual(geometry, [{ version: 1, options: { preserveCamera: false } }, { version: 2, options: { preserveCamera: true } }]);
  assert.equal(target.meshes[0].material, target.sets.B.hull);
  assert.equal(target.sets.B.hull.map.userData.hash, 'tex-b2'); assert.equal(target.sets.A.hull.map.userData.hash, 'tex-a2');
  assert.equal(target.U.uCamoCol.value.r, 0.2); assert.equal(useStore.getState().texProg, null);
});
test('late baseline texture cannot replace the newest comparison side A', async () => {
  await reset('a1'); const old = resync(); const release = await waitForTexture();
  generation = 2; await resync(); release(); await old;
  assert.equal(target.sets.A.hull.map.userData.hash, 'tex-a2'); assert.equal(target.sets.B.hull.map.userData.hash, 'tex-b2');
  assert.equal(target.meshes[0].material, target.sets.B.hull); assert.equal(useStore.getState().phase, 'ready');
});
test('losing preview capability prevents a pending real material commit without a replacement request', async () => {
  await reset('b1'); const old = resync(); const release = await waitForTexture();
  preview = false; await resync(); release(); await old;
  assert.deepEqual(target.sets, { A: {}, B: {} }); assert.equal(target.meshes[0].material.map, null);
  assert.equal(target.matCache.size, 0); assert.equal(useStore.getState().project.capabilities.preview, false);
  preview = true; generation = 2; await resync(); assert.equal(target.sets.B.hull.map.userData.hash, 'tex-b2');
});
test('unmount prevents a late real texture from committing to the removed viewer', async () => {
  await reset('b1'); const old = resync(); const release = await waitForTexture();
  disposeWorkbench(); release(); await old;
  assert.deepEqual(target.sets, { A: {}, B: {} }); assert.equal(target.meshes[0].material.map, null); assert.equal(target.matCache.size, 0);
});
test('direct render calls without a workbench lifetime guard still apply both material sides', async () => {
  await reset(''); await target.applyManifest('B', manifest('b', 2)); await target.applyManifest('A', manifest('a', 2));
  assert.equal(target.meshes[0].material.map.userData.hash, 'tex-b2'); assert.equal(target.sets.A.hull.map.userData.hash, 'tex-a2');
});
test.after(() => { disposeWorkbench(); clearTexCache(); });
