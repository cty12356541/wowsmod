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
let ship = 'A', simulation = true, simFails = false, simRequests = 0;
let pendingSim = null;
const viewerSimulation = [];
globalThis.fetch = async (url) => {
  if (url.includes('/geometry/')) return new Response(new Uint8Array([1]));
  if (url.endsWith('/sim')) { simRequests++; if (simFails) return new Response(JSON.stringify({ ok: false, error: { code: 'no_sim', message: 'No simulation data' } }), { status: 404, headers: { 'Content-Type': 'application/json' } }); if (pendingSim === true) { const requestedShip = ship; return new Promise((resolve) => { pendingSim = () => resolve(new Response(JSON.stringify({ ship: { code: requestedShip }, mounts: {} }), { headers: { 'Content-Type': 'application/json' } })); }); } }
  const value = url.endsWith('/project') ? { project: { name: 'Changing ship', dir: 'C:/mods/B' }, ship: { code: ship, param: ship }, game: { running: false }, install: { state: 'not_installed' }, capabilities: { preview: true, simulation, reasons: {} } } : url.endsWith('/steps') ? { steps: [], head: 'current' } : url.includes('/model?') ? { rev: 'current', geometry: { hash: 'one', url: '/api/v1/projects/B/geometry/one.glb' }, materials: {} } : url.includes('/parts?') ? { parts: [] } : url.includes('/textures?') ? { textures: [] } : url.endsWith('/sim') ? { ship: { code: ship }, mounts: {} } : { ok: true, etag: 1 };
  return new Response(JSON.stringify(value), { headers: { 'Content-Type': 'application/json' } });
};
const { boot, disposeWorkbench, resync, useStore } = await import('../store.ts');
const fakeViewer = { loadGeometry() {}, setFx() {}, applyManifest: async () => ({ failed: [] }), gc() {}, setSimData(value) { viewerSimulation.push(value); } };
test('ordinary material/source refresh does not repeatedly fetch unchanged ship simulation', async () => {
  await boot(() => fakeViewer); assert.equal(simRequests, 1);
  await resync(); await resync(); assert.equal(simRequests, 1);
  assert.equal(useStore.getState().sim.ship.code, 'A');
});
test('losing simulation capability clears old data and exits the simulation workspace', async () => {
  useStore.setState({ workspace: 'sim', rightTab: 'sim', simOn: true, snap: {}, tel: 'full', rudder: 1 });
  simulation = false; await resync();
  assert.equal(useStore.getState().sim, null); assert.equal(useStore.getState().workspace, 'review');
  assert.equal(useStore.getState().simOn, false); assert.equal(useStore.getState().snap, null);
  assert.equal(viewerSimulation.at(-1), null); assert.equal(simRequests, 1);
});
test('restored explicit capability fetches the supported simulation once', async () => {
  simulation = true; await resync(); assert.equal(simRequests, 2);
  assert.equal(useStore.getState().sim.ship.code, 'A'); await resync(); assert.equal(simRequests, 2);
});
test('changing ship clears old simulation, exits sim, and fetches new supported ship data once', async () => {
  useStore.setState({ workspace: 'sim', simOn: true }); ship = 'B'; await resync();
  assert.equal(useStore.getState().workspace, 'review'); assert.equal(useStore.getState().simOn, false);
  assert.equal(simRequests, 3); assert.equal(useStore.getState().sim.ship.code, 'B');
  assert.equal(viewerSimulation.at(-2), null); assert.equal(viewerSimulation.at(-1).ship.code, 'B');
});
test('failed new-ship simulation stays unavailable and unchanged resync does not retry in a loop', async () => {
  ship = 'C'; simFails = true; await resync();
  assert.equal(simRequests, 4); assert.equal(useStore.getState().sim, null); assert.equal(viewerSimulation.at(-1), null);
  await resync(); assert.equal(simRequests, 4);
});
test('an unchanged resync preserves the one pending supported-ship simulation request', async () => {
  ship = 'D'; simFails = false; pendingSim = true; const changing = resync();
  for (let i = 0; i < 20 && pendingSim === true; i++) await new Promise((resolve) => setTimeout(resolve, 1));
  assert.equal(typeof pendingSim, 'function'); const finish = pendingSim;
  await resync(); finish(); await changing;
  assert.equal(simRequests, 5); assert.equal(useStore.getState().sim?.ship.code, 'D');
});
test.after(() => disposeWorkbench());
