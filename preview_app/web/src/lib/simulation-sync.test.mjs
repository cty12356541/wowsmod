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
const { actions, boot, disposeWorkbench, resync, useStore } = await import('../store.ts');
const fakeViewer = { loadGeometry() {}, setFx() {}, applyManifest: async () => ({ failed: [] }), gc() {}, setSimData(value) { viewerSimulation.push(value); },
  setWaves() {}, setCombat() {}, setTelegraph() {}, setRudder() {}, setReloadReal() {}, setAimDistKm() {} };
async function waitFor(predicate) {
  for (let i = 0; i < 100 && !predicate(); i++) await new Promise((resolve) => setTimeout(resolve, 1));
  assert.ok(predicate(), 'expected workbench transition did not occur');
}
test('ordinary material/source refresh does not repeatedly fetch unchanged ship simulation', async () => {
  await boot(() => fakeViewer); assert.equal(simRequests, 1);
  await waitFor(() => useStore.getState().sim?.ship.code === 'A');
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

test('boot finishes both material sides before starting optional simulation', async () => {
  disposeWorkbench(); ship = 'F'; simulation = true; pendingSim = true;
  const before = simRequests;
  const B = Promise.withResolvers(), A = Promise.withResolvers();
  const started = [], committed = [];
  const target = { ...fakeViewer, async applyManifest(side) {
    started.push(side);
    await (side === 'B' ? B.promise : A.promise);
    committed.push(side);
    return { failed: [] };
  } };
  const ready = [];
  const unsubscribe = useStore.subscribe((state) => {
    if (state.phase === 'ready' && !state.texProg) ready.push(committed.slice());
  });
  let bootFinished = false;
  const loading = boot(() => target).then(() => { bootFinished = true; });
  try {
    await waitFor(() => started.length || simRequests > before);
    assert.equal(simRequests, before, 'simulation must not start ahead of B materials');
    assert.deepEqual(started, ['B']); assert.deepEqual(ready, []);
    B.resolve(); await waitFor(() => started.includes('A'));
    assert.equal(simRequests, before, 'simulation must not start ahead of A materials');
    assert.deepEqual(committed, ['B']); assert.deepEqual(ready, []);
    A.resolve(); await waitFor(() => bootFinished && typeof pendingSim === 'function');
    assert.deepEqual(ready.at(-1), ['B', 'A']);
    assert.equal(useStore.getState().sim, null);
    await Promise.all([resync(), resync()]);
    assert.equal(simRequests, before + 1);
    pendingSim(); await waitFor(() => useStore.getState().sim?.ship.code === 'F');
    assert.equal(viewerSimulation.at(-1).ship.code, 'F');
  } finally {
    unsubscribe(); B.resolve(); A.resolve();
    if (typeof pendingSim === 'function') pendingSim();
    disposeWorkbench(); await loading;
  }
});

test('a supported ship refresh supersedes a held startup simulation request', async () => {
  disposeWorkbench(); ship = 'G'; simulation = true; pendingSim = true;
  const loading = boot(() => fakeViewer);
  await waitFor(() => typeof pendingSim === 'function');
  const release = pendingSim;
  try {
    ship = 'H'; pendingSim = null; await resync();
    assert.equal(useStore.getState().sim?.ship.code, 'H');
    release(); await loading; await new Promise(setImmediate);
    assert.equal(useStore.getState().sim?.ship.code, 'H');
    assert.equal(viewerSimulation.at(-1).ship.code, 'H');
  } finally { release(); disposeWorkbench(); }
});

test('unmount prevents a held startup simulation from reaching the viewer', async () => {
  disposeWorkbench(); ship = 'I'; simulation = true; pendingSim = true;
  const loading = boot(() => fakeViewer);
  await waitFor(() => typeof pendingSim === 'function');
  const release = pendingSim;
  const count = viewerSimulation.length;
  disposeWorkbench(); release(); await loading; await new Promise(setImmediate);
  assert.equal(viewerSimulation.length, count);
  assert.notEqual(useStore.getState().sim?.ship.code, 'I');
});

test('startup probes a legacy project with no simulation capability field', async () => {
  disposeWorkbench(); ship = 'J'; simulation = undefined; pendingSim = null;
  const before = simRequests;
  await boot(() => fakeViewer);
  await waitFor(() => useStore.getState().sim?.ship.code === 'J');
  await resync(); assert.equal(simRequests, before + 1);
});

test('waves stay applied after simulation loads, resets and re-entry; game camera exits in sync', async () => {
  disposeWorkbench(); ship = 'K'; simulation = true; pendingSim = null;
  const target = { ...fakeViewer,
    setSimData(data) { this.motion = data ? { waves: 1 } : null; },
    simEnable(on) { if (!on && this.camMode === 'game') this.setCamMode('orbit'); },
    simReset() { this.motion = { waves: 1 }; },
    setWaves(w) { if (this.motion) this.motion.waves = w; },
    setCamMode(m) { this.camMode = m; },
    setTelegraph(t) { this.motion.tel = t; },
    setRudder(r) { this.motion.rudder = r; },
    setLodMode() {}, setLodTint() {}, setLodBias() {}, setCombat() {}, setReloadReal() {}, setAimDistKm() {}, setEmisOverride() {}, setMode() {},
  };
  actions.setWaves(0.35);
  await boot(() => target);
  await waitFor(() => useStore.getState().sim?.ship.code === 'K');
  assert.equal(target.motion.waves, 0.35);
  actions.setWorkspace('sim');
  actions.setTel('full'); actions.setRudder(1); actions.setCamMode('game');
  actions.simReset();
  assert.equal(target.motion.waves, 0.35);
  assert.equal(target.motion.tel, 'stop'); assert.equal(target.motion.rudder, 0);
  assert.equal(useStore.getState().tel, 'stop'); assert.equal(useStore.getState().rudder, 0);
  assert.equal(useStore.getState().camMode, 'game');
  actions.setWorkspace('review');
  assert.equal(useStore.getState().camMode, 'orbit');
  assert.equal(target.camMode, 'orbit');
  actions.setWorkspace('sim');
  assert.equal(target.camMode, 'orbit'); assert.equal(target.motion.waves, 0.35);
});
test.after(() => disposeWorkbench());
