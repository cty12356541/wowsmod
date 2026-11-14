import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import test from 'node:test';
import * as THREE from 'three';

registerHooks({ resolve(specifier, context, nextResolve) {
  const result = nextResolve(specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier) ? specifier + '.ts' : specifier, context);
  return specifier.endsWith('.json') ? { ...result, importAttributes: { type: 'json' } } : result;
} });

const { Viewer } = await import('../engine/viewer.ts');
const { makeMount, ShipMotion } = await import('../engine/sim.ts');

const spec = { group: 'main', model: 'fixture', horizSector: [-45, 45], vertSector: [0, 30], rotationSpeed: [30, 20], shotDelay: 5 };
function mount(id, { rig = true, muzzle = true } = {}) {
  const top = new THREE.Group();
  const yaw = new THREE.Bone(), pitch = new THREE.Bone(), recoil = new THREE.Bone();
  yaw.userData.bone = 'yaw'; pitch.userData.bone = 'pitch'; recoil.userData.bone = 'recoil';
  top.add(yaw); yaw.add(pitch); pitch.add(recoil);
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute([0, 0, 0, 1, 0, 0, 0, 1, 0], 3));
  geometry.setAttribute('skinIndex', new THREE.Uint16BufferAttribute([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 4));
  geometry.setAttribute('skinWeight', new THREE.Float32BufferAttribute([1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], 4));
  const mesh = rig ? new THREE.SkinnedMesh(geometry) : new THREE.Mesh(geometry);
  top.add(mesh);
  if (rig) mesh.bind(new THREE.Skeleton([recoil]));
  if (muzzle) {
    const point = new THREE.Object3D();
    point.userData.muzzle = 0;
    recoil.add(point);
  }
  return makeMount(id, spec, top, 0);
}

function viewerFor(...mounts) {
  const shipFrame = new THREE.Group();
  for (const m of mounts) shipFrame.add(m.top);
  return Object.assign(Object.create(Viewer.prototype), {
    mounts, shipFrame, motion: new ShipMotion({}), parts: {}, availableLods: [],
    camera: new THREE.PerspectiveCamera(), controls: { target: new THREE.Vector3() },
    WL: 0, aimBearing: null, aimDistKm: 1, simOn: true, combat: true,
    pendingShots: [], singleIdx: {}, reloadReal: true,
  });
}

const diagnostics = (v) => v.simSnapshot().mounts.map((m) => m.readiness);
const block = (reasons, reload_s = 0, missingData = []) => ({ ready: false, reasons, reload_s, missingData });

test('snapshot and normal firing agree before and after the target moves or a gun reloads', () => {
  const v = viewerFor(mount('main/1'), mount('main/2'));
  v.simOn = false;
  assert.deepEqual(diagnostics(v)[0], block(['simulation_off', 'no_target']));
  assert.equal(v.fire('main', 'salvo'), 0);

  v.simOn = true;
  v.combat = false;
  assert.deepEqual(diagnostics(v)[0], block(['combat_off', 'no_target']));
  assert.equal(v.fire('main', 'salvo'), 0);

  v.combat = true;
  assert.deepEqual(diagnostics(v)[0], block(['no_target']));
  assert.equal(v.fire('main', 'salvo'), 0);

  v.aimAt(90);
  assert.deepEqual(diagnostics(v)[0], block(['outside_sector']));
  assert.equal(v.fire('main', 'salvo'), 0);

  v.aimAt(0, 15);
  assert.deepEqual(diagnostics(v)[0], block(['aiming']));
  assert.equal(v.fire('main', 'salvo'), 0);
  v.stepMounts(2);
  assert.deepEqual(diagnostics(v), [
    { ready: true, reasons: [], reload_s: 0, missingData: [] },
    { ready: true, reasons: [], reload_s: 0, missingData: [] },
  ]);

  // Move the aim before another frame to prove firing performs its own check.
  v.setAimDistKm(7);
  assert.equal(v.fire('main', 'salvo'), 0);
  assert.deepEqual(diagnostics(v)[0], block(['aiming']));
  v.stepMounts(2);
  assert.equal(v.fire('main', 'salvo'), 2);
  assert.deepEqual(v.pendingShots.map((shot) => [shot.m.id, shot.t]), [['main/1', 0], ['main/2', 0.04]]);
  assert.deepEqual(diagnostics(v)[0], block(['reloading'], 5));
  assert.equal(v.fire('main', 'single'), 0);
  v.aimAt(90);
  assert.deepEqual(diagnostics(v)[0], block(['outside_sector', 'reloading'], 5));
});

test('diagnostics identify missing rig and muzzle separately and reject an estimated muzzle', () => {
  const noRig = mount('main/no-rig', { rig: false });
  const noMuzzle = mount('main/no-muzzle', { muzzle: false });
  const neither = mount('main/neither', { rig: false, muzzle: false });
  const v = viewerFor(noRig, noMuzzle, neither);
  // An estimated muzzle can be attached by the separate forced render path.
  const estimated = new THREE.Object3D();
  estimated.userData = { muzzle: 0, synthetic: true };
  noMuzzle.recoilN.add(estimated);
  noMuzzle.muzzles.push(estimated);
  v.aimAt(0);
  assert.deepEqual(diagnostics(v), [
    block(['data_missing'], 0, ['rig']),
    block(['data_missing'], 0, ['muzzle']),
    block(['data_missing'], 0, ['rig', 'muzzle']),
  ]);
  assert.equal(v.fire('main', 'salvo'), 0);
  assert.equal(v.pendingShots.length, 0);

  v.simOn = false;
  v.combat = false;
  v.clearAim();
  neither.reload = 2.25;
  assert.deepEqual(diagnostics(v)[2], block(['simulation_off', 'data_missing', 'combat_off', 'no_target', 'reloading'], 2.25, ['rig', 'muzzle']));
});

test('a gun listed in simulation without a geometry part appears with missing data', () => {
  const v = viewerFor();
  v.ship = new THREE.Group();
  v.BOX = new THREE.Box3(new THREE.Vector3(-1, -1, -1), new THREE.Vector3(1, 1, 1));
  v.setSimData({ ship: {}, mounts: { missing: spec } });
  assert.equal(v.simSnapshot().mounts[0]?.id, 'missing');
  assert.deepEqual(diagnostics(v)[0], block(['data_missing', 'no_target'], 0, ['rig', 'muzzle']));
  assert.equal(v.fire('main', 'salvo'), 0);
});

test('workbench combat and keyboard firing preserve the explicit no-target and port states', async () => {
  globalThis.window = { WM_API_BASE: '', WM_NO_SSE: true };
  globalThis.location = { search: '' };
  globalThis.localStorage = { getItem: () => null };
  globalThis.document = { documentElement: {}, querySelector: () => null };
  const { actions, setViewer, useStore } = await import('../store.ts');
  const v = viewerFor(mount('main/1'));
  setViewer(v);
  try {
    actions.setCombat(true);
    assert.equal(v.aimBearing, null);
    assert.deepEqual(diagnostics(v)[0], block(['no_target']));
    actions.setCombat(false);
    useStore.setState({ workspace: 'sim' });
    const { handleWorkbenchKey } = await import('./keyboard.ts');
    handleWorkbenchKey({ key: ' ', preventDefault() {} });
    assert.equal(useStore.getState().combat, false);
    assert.equal(v.combat, false);
    assert.equal(v.pendingShots.length, 0);
  } finally {
    setViewer(null);
  }
});
