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
let geometryByShip = false, pendingGeometry = null;
const requestedSimShips = [];
const viewerSimulation = [];
globalThis.fetch = async (url) => {
  if (url.includes('/geometry/')) { if (pendingGeometry && typeof pendingGeometry === 'string' && url.endsWith(`/${pendingGeometry}.glb`)) return new Promise((resolve) => { pendingGeometry = () => resolve(new Response(new Uint8Array([1]))); }); return new Response(new Uint8Array([1])); }
  if (url.endsWith('/sim')) { simRequests++; requestedSimShips.push(ship); if (simFails) return new Response(JSON.stringify({ ok: false, error: { code: 'no_sim', message: 'No simulation data' } }), { status: 404, headers: { 'Content-Type': 'application/json' } }); if (pendingSim === true) { const requestedShip = ship; return new Promise((resolve) => { pendingSim = () => resolve(new Response(JSON.stringify({ ship: { code: requestedShip }, mounts: {} }), { headers: { 'Content-Type': 'application/json' } })); }); } }
  const geometry = geometryByShip ? ship : 'one';
  const value = url.endsWith('/project') ? { project: { name: 'Changing ship', dir: 'C:/mods/B' }, ship: { code: ship, param: ship }, game: { running: false }, install: { state: 'not_installed' }, capabilities: { preview: true, simulation, reasons: {} } } : url.endsWith('/steps') ? { steps: [], head: 'current' } : url.includes('/model?') ? { rev: 'current', geometry: { hash: geometry, url: `/api/v1/projects/B/geometry/${geometry}.glb` }, materials: {} } : url.includes('/parts?') ? { parts: [] } : url.includes('/textures?') ? { textures: [] } : url.endsWith('/sim') ? { ship: { code: ship }, mounts: {} } : { ok: true, etag: 1 };
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
test('a newer same-ship resync starts simulation when the first resync is still awaiting geometry', async () => {
  disposeWorkbench(); ship = 'A'; simulation = true; simFails = false; pendingSim = null; geometryByShip = true;
  const before = requestedSimShips.length; await boot(() => fakeViewer);
  ship = 'B'; pendingGeometry = 'B'; const first = resync();
  for (let i = 0; i < 20 && typeof pendingGeometry === 'string'; i++) await new Promise((resolve) => setTimeout(resolve, 1));
  assert.equal(typeof pendingGeometry, 'function'); const releaseGeometry = pendingGeometry;
  assert.deepEqual(requestedSimShips.slice(before), ['A']);
  await resync(); releaseGeometry(); await first;
  assert.deepEqual(requestedSimShips.slice(before), ['A', 'B']);
  assert.equal(useStore.getState().project.ship.code, 'B'); assert.equal(useStore.getState().sim?.ship.code, 'B');
});
test('late simulation from an older ship identity cannot replace the newer supported ship', async () => {
  ship = 'C'; pendingSim = true; const first = resync();
  for (let i = 0; i < 20 && pendingSim === true; i++) await new Promise((resolve) => setTimeout(resolve, 1));
  assert.equal(typeof pendingSim, 'function'); const release = pendingSim;
  ship = 'D'; pendingSim = null; await resync(); release(); await first;
  assert.equal(useStore.getState().sim?.ship.code, 'D'); assert.equal(viewerSimulation.at(-1).ship.code, 'D');
});
test('unmount rejects a late same-identity simulation result', async () => {
  ship = 'E'; pendingSim = true; const first = resync();
  for (let i = 0; i < 20 && pendingSim === true; i++) await new Promise((resolve) => setTimeout(resolve, 1));
  assert.equal(typeof pendingSim, 'function'); const release = pendingSim; const count = viewerSimulation.length;
  disposeWorkbench(); release(); await first;
  assert.equal(viewerSimulation.length, count); assert.equal(useStore.getState().sim, null);
});
test.after(() => disposeWorkbench());
