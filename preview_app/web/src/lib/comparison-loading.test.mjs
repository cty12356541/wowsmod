import { registerHooks } from 'node:module';
import test from 'node:test';
import assert from 'node:assert/strict';
registerHooks({ resolve(specifier, context, nextResolve) {
  const result = nextResolve(specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier) ? specifier + '.ts' : specifier, context);
  return specifier.endsWith('.json') ? { ...result, importAttributes: { type: 'json' } } : result;
} });
globalThis.window = { WM_API_BASE: '', WM_NO_SSE: true, setInterval, clearInterval, createImageBitmap: true };
globalThis.createImageBitmap = async () => ({ width: 1, height: 1, close() {} });
globalThis.location = { search: '?project=deferred-base' };
globalThis.localStorage = { getItem: (key) => key === 'wm.quality' ? 'high' : null, setItem() {} };
globalThis.document = { title: '', documentElement: {} };
let generation = 1, hold = '', failed = '', failedModel = '', pending = null;
const requested = [], presets = [];
function model(rev) {
  const side = rev === 'base' || rev === 'previous' ? 'a' : 'b';
  const key = `${side}-${generation}-${rev}`;
  return { rev, geometry: { hash: `g${generation}`, url: `/api/v1/projects/deferred-base/geometry/g${generation}.glb` }, materials: {
    hull: { family: 'pbs', params: {}, textures: { a: { name: key, hash: key, url: `/api/v1/projects/deferred-base/blob/${key}.png`, width: 1, height: 1 } } },
  } };
}
globalThis.fetch = async (url) => {
  requested.push(url);
  if (failedModel && url.includes('/model?') && url.includes(failedModel)) return new Response(JSON.stringify({ error: { code: 'model_failed', message: 'Revision manifest unavailable' } }), { status: 404, headers: { 'Content-Type': 'application/json' } });
  if (url.includes('/blob/')) {
    if (hold && url.includes(hold)) return new Promise((resolve) => { pending = () => resolve(new Response(new Uint8Array([1]))); });
    return new Response(new Uint8Array([1]), { status: failed && url.includes(failed) ? 404 : 200 });
  }
  if (url.includes('/geometry/')) return new Response(new Uint8Array([generation]));
  const value = url.endsWith('/project') ? { project: { name: `Project ${generation}`, dir: `C:/mods/${generation}` }, ship: { code: 'X' }, game: {}, capabilities: { preview: true, simulation: false, reasons: {} } }
    : url.endsWith('/steps') ? { steps: [{ id: 'previous', state: 'active', parts: [] }, { id: 'current', state: 'active', parts: [] }, { id: 'newer', state: 'active', parts: [] }], head: 'current' }
    : url.includes('/model?') ? model(new URL(url, 'http://test').searchParams.get('rev'))
    : url.includes('/parts?') ? { parts: [{ id: 'hull', changed: true }] }
    : url.includes('/textures?') ? { textures: [] } : { ok: true, etag: generation };
  return new Response(JSON.stringify(value), { headers: { 'Content-Type': 'application/json' } });
};
const THREE = await import('three');
const { Viewer } = await import('../engine/viewer.ts');
const { clearTexCache } = await import('./tex.ts');
const { disableLive } = await import('./api.ts');
const { actions, boot, disposeWorkbench, resync, useStore } = await import('../store.ts');
let target;
async function reset(mode = 'single', deferred = '') {
  disposeWorkbench(); clearTexCache(); generation = 1; hold = ''; failed = ''; failedModel = ''; pending = null; requested.length = 0;
  hold = deferred;
  presets.length = 0;
  const mesh = new THREE.Mesh(); mesh.userData = { mfm: 'hull', part: 'hull' };
  target = Object.assign(Object.create(Viewer.prototype), {
    Q: { tex: 2048, overlay: true }, U: { uCamoCol: { value: new THREE.Color() } }, pal: { envI: 1 }, look: 'studio', channel: 0,
    mode: 'single', compMat: { uniforms: { uMode: { value: 0 } } },
    matCache: new Map(), sets: { A: {}, B: {} }, meshes: [mesh], renderer: { shadowMap: {} },
    loadGeometry() {}, setFx() {}, poke() {}, setPartVisible() {}, preset(name) { presets.push(name); }, flash() {}, simEnable() {}, setLodMode() {}, setLodTint() {}, setEmisOverride() {},
    setQuality(quality) { this.Q = { tex: quality === 'high' ? 2048 : 1024, overlay: true }; return true; },
  });
  useStore.setState({ mode, workspace: 'review', baseSel: 'base' });
  await boot(() => target); disableLive();
}
async function waitFor(predicate) {
  for (let i = 0; i < 100 && !predicate(); i++) await new Promise((resolve) => setTimeout(resolve, 1));
  assert.ok(predicate(), 'expected deferred comparison transition');
}
const pngs = () => requested.filter((url) => url.includes('/blob/'));
test('single boot loads full B without fetching A PNGs and preserves changed-part highlighting', async () => {
  await reset();
  assert.deepEqual(pngs(), ['/api/v1/projects/deferred-base/blob/b-1-current.png']);
  assert.equal(useStore.getState().phase, 'ready');
  assert.equal(target.sets.B.hull.map.userData.hash, 'b-1-current');
  assert.deepEqual(target.sets.A, {});
  assert.deepEqual(target.changedParts(), ['hull']);
});
test('split waits for full A while retaining B; later compare modes reuse the completed side', async () => {
  await reset(); hold = 'a-1-base';
  const switching = actions.setMode('split'); await waitFor(() => pending);
  assert.equal(useStore.getState().compareStatus, 'loading'); assert.equal(target.mode, 'single');
  assert.equal(target.meshes[0].material.map.userData.hash, 'b-1-current');
  pending(); await switching;
  assert.equal(useStore.getState().compareStatus, 'ready'); assert.equal(target.mode, 'split');
  assert.equal(target.sets.A.hull.map.userData.hash, 'a-1-base');
  const before = pngs().length;
  await actions.setMode('single'); await actions.setMode('side'); await actions.setMode('overlay');
  assert.equal(pngs().length, before); assert.equal(target.mode, 'overlay');
});
test('returning to single during A loading never re-enables comparison and retains completed cache', async () => {
  await reset(); hold = 'a-1-base';
  const switching = actions.setMode('split'); await waitFor(() => pending);
  await actions.setMode('single'); pending(); await switching;
  assert.equal(target.mode, 'single'); assert.equal(useStore.getState().mode, 'single');
  const before = pngs().length; await actions.setMode('overlay');
  assert.equal(pngs().length, before); assert.equal(target.mode, 'overlay');
});
test('failed A textures retain full B and an explicit retry fetches the failed texture', async () => {
  await reset(); failed = 'a-1-base'; await actions.setMode('split');
  assert.equal(useStore.getState().compareStatus, 'error'); assert.equal(target.mode, 'single');
  assert.ok(useStore.getState().compareError); assert.deepEqual(target.sets.A, {});
  assert.equal(target.meshes[0].material.map.userData.hash, 'b-1-current');
  failed = ''; await actions.retryComparison();
  assert.equal(useStore.getState().compareStatus, 'ready'); assert.equal(target.mode, 'split');
  assert.equal(pngs().filter((url) => url.includes('a-1-base')).length, 2);
  assert.equal(target.sets.A.hull.map.userData.hash, 'a-1-base');
});
test('a source and geometry refresh supersedes an in-flight A and only publishes the new comparison', async () => {
  await reset(); hold = 'a-1-base';
  const switching = actions.setMode('split'); await waitFor(() => pending); const release = pending;
  generation = 2; hold = ''; await resync(); release(); await switching;
  assert.equal(useStore.getState().project.project.name, 'Project 2');
  assert.equal(target.sets.A.hull.map.userData.hash, 'a-2-base');
  assert.equal(target.sets.B.hull.map.userData.hash, 'b-2-current'); assert.equal(target.mode, 'split');
});
test('revision and previous-base selection supersede pending comparison work', async () => {
  await reset(); hold = 'a-1-base';
  const switching = actions.setMode('split'); await waitFor(() => pending); const release = pending;
  await actions.setMode('single'); await actions.view('newer'); await actions.setBase('prev'); hold = '';
  await actions.setMode('side'); release(); await switching;
  assert.equal(target.sets.B.hull.map.userData.hash, 'b-1-newer');
  assert.equal(target.sets.A.hull.map.userData.hash, 'b-1-current'); assert.equal(target.mode, 'side');
});
test('compare boot finishes complete A and B before publishing ready', async () => {
  await reset('split');
  assert.equal(useStore.getState().phase, 'ready'); assert.equal(useStore.getState().compareStatus, 'ready');
  assert.equal(target.sets.A.hull.map.userData.hash, 'a-1-base'); assert.equal(target.sets.B.hull.map.userData.hash, 'b-1-current');
  assert.equal(target.mode, 'split');
});
test('single can finish boot with full B while its previously requested comparison is still loading', async () => {
  let finished = false;
  const loading = reset('split', 'a-1-base').then(() => { finished = true; });
  await waitFor(() => pending);
  assert.equal(useStore.getState().phase, 'loading');
  await actions.setMode('single'); await waitFor(() => finished);
  assert.equal(useStore.getState().phase, 'ready'); assert.equal(target.mode, 'single');
  assert.equal(target.sets.B.hull.map.userData.hash, 'b-1-current');
  pending(); await loading; await waitFor(() => useStore.getState().compareStatus === 'ready');
  assert.equal(target.mode, 'single');
});
test('a new workbench lifetime cannot receive A from a removed project viewer', async () => {
  await reset(); hold = 'a-1-base';
  const switching = actions.setMode('split'); await waitFor(() => pending);
  const release = pending, removed = target;
  await reset(); release(); await switching;
  assert.deepEqual(removed.sets.A, {}); assert.deepEqual(target.sets.A, {});
  assert.equal(useStore.getState().compareStatus, 'idle'); assert.equal(target.mode, 'single');
});
test('changing texture quality during A loading only publishes materials from the new generation', async () => {
  await reset(); hold = 'a-1-base';
  const switching = actions.setMode('overlay'); await waitFor(() => pending); const release = pending;
  hold = ''; actions.setQuality('mid');
  await waitFor(() => useStore.getState().compareStatus === 'ready'); release(); await switching;
  assert.match(target.sets.A.hull.userData.sig, /\|1024$/);
  assert.match(target.sets.B.hull.userData.sig, /\|1024$/); assert.equal(target.mode, 'overlay');
});
test('cached B fallback textures are retried on the next source refresh', async () => {
  await reset(); generation = 2; failed = 'b-2-current'; await resync();
  assert.equal(target.sets.B.hull.userData.textureFailures, true);
  failed = ''; await resync();
  assert.equal(target.sets.B.hull.map.userData.hash, 'b-2-current');
  assert.equal(target.sets.B.hull.userData.textureFailures, false);
  assert.equal(pngs().filter((url) => url.includes('b-2-current')).length, 2);
});
test('a side-by-side source refresh preserves the user camera while A is being replaced', async () => {
  await reset(); await actions.setMode('side');
  assert.deepEqual(presets, ['hero']);
  generation = 2; await resync();
  assert.equal(target.mode, 'side'); assert.deepEqual(presets, ['hero']);
});
test('comparison cannot claim readiness with incomplete B; retry restores full B before A', async () => {
  await reset(); generation = 2; failed = 'b-2-current'; await resync();
  await actions.setMode('split');
  assert.equal(useStore.getState().compareStatus, 'error'); assert.equal(target.mode, 'single');
  assert.equal(pngs().filter((url) => url.includes('a-2-base')).length, 0);
  failed = ''; await actions.retryComparison();
  assert.equal(useStore.getState().compareStatus, 'ready'); assert.equal(target.mode, 'split');
  assert.equal(target.sets.B.hull.map.userData.hash, 'b-2-current');
  assert.equal(target.sets.A.hull.map.userData.hash, 'a-2-base');
});
test('a failed revision manifest exits comparison loading and can be retried without changing selection', async () => {
  await reset(); await actions.setMode('split'); failedModel = 'rev=newer';
  await assert.rejects(actions.view('newer'), /Revision manifest unavailable/);
  assert.equal(useStore.getState().compareStatus, 'error'); assert.equal(target.mode, 'single');
  assert.equal(useStore.getState().viewRev, 'newer');
  failedModel = ''; await actions.retryComparison();
  assert.equal(useStore.getState().compareStatus, 'ready'); assert.equal(target.mode, 'split');
  assert.equal(target.sets.B.hull.map.userData.hash, 'b-1-newer');
});
for (const trigger of ['history', 'resync']) test(`${trigger} replacing compare boot keeps loading until the latest B and A commit`, async () => {
  let finished = false;
  const initial = reset('split', 'a-1-base').then(() => { finished = true; });
  await waitFor(() => pending); const releaseOldA = pending;
  pending = null;
  if (trigger === 'resync') generation = 2;
  const expectedB = trigger === 'history' ? 'b-1-newer' : 'b-2-current';
  hold = expectedB;
  const replacement = trigger === 'history' ? actions.view('newer') : resync();
  await waitFor(() => pending); const releaseNewB = pending;
  const readyMaterials = [];
  const unsubscribe = useStore.subscribe((state) => {
    if (state.phase === 'ready') readyMaterials.push(target.sets.B.hull?.map?.userData.hash);
  });
  try {
    assert.equal(useStore.getState().phase, 'loading'); assert.equal(finished, false);
    releaseOldA(); await new Promise(setImmediate);
    assert.equal(useStore.getState().phase, 'loading', 'late A cannot finish the replaced boot');
    assert.equal(finished, false);
    hold = ''; releaseNewB(); await replacement; await initial;
    assert.equal(useStore.getState().phase, 'ready'); assert.equal(finished, true);
    assert.equal(useStore.getState().compareStatus, 'ready');
    assert.equal(target.sets.B.hull.map.userData.hash, expectedB);
    assert.equal(target.sets.A.hull.map.userData.hash, `a-${generation}-base`);
    assert.ok(readyMaterials.length); assert.ok(readyMaterials.every((hash) => hash === expectedB));
  } finally { unsubscribe(); hold = ''; releaseOldA(); releaseNewB(); await Promise.allSettled([initial, replacement]); }
});
for (const failure of ['texture', 'manifest']) test(`replacement B ${failure} failure during compare boot stays failed until the same revision retries`, async () => {
  const initial = reset('split', 'a-1-base');
  await waitFor(() => pending); const releaseOldA = pending;
  hold = '';
  if (failure === 'texture') failed = 'b-1-newer'; else failedModel = 'rev=newer';
  let publishedReady = false;
  const unsubscribe = useStore.subscribe((state) => { if (state.phase === 'ready') publishedReady = true; });
  try {
    const replacement = actions.view('newer');
    if (failure === 'manifest') await assert.rejects(replacement, /Revision manifest unavailable/); else await replacement;
    releaseOldA(); await initial;
    assert.equal(publishedReady, false); assert.equal(useStore.getState().phase, 'fatal');
    assert.equal(useStore.getState().viewRev, 'newer'); assert.equal(useStore.getState().compareStatus, 'error');
    assert.ok(useStore.getState().fatal.message);
    failed = ''; failedModel = ''; await actions.retryComparison();
    assert.equal(useStore.getState().phase, 'ready'); assert.equal(useStore.getState().fatal, null);
    assert.equal(useStore.getState().viewRev, 'newer'); assert.equal(useStore.getState().compareStatus, 'ready');
    assert.equal(target.sets.B.hull.map.userData.hash, 'b-1-newer');
  } finally { unsubscribe(); releaseOldA(); await initial; }
});
for (const side of ['A', 'B']) test(`latest source finishes boot without waiting for superseded ${side} network completion`, async () => {
  let finished = false;
  const initial = reset(side === 'A' ? 'split' : 'single', side === 'A' ? 'a-1-base' : 'b-1-current').then(() => { finished = true; });
  await waitFor(() => pending); const releaseOld = pending;
  try {
    generation = 2; hold = ''; await resync();
    await waitFor(() => finished);
    assert.equal(useStore.getState().phase, 'ready');
    assert.equal(target.sets.B.hull.map.userData.hash, 'b-2-current');
    if (side === 'A') assert.equal(target.sets.A.hull.map.userData.hash, 'a-2-base');
    releaseOld(); await new Promise(setImmediate);
    assert.equal(target.sets.B.hull.map.userData.hash, 'b-2-current');
  } finally { releaseOld(); await initial; }
});
test.after(() => { disposeWorkbench(); clearTexCache(); });
