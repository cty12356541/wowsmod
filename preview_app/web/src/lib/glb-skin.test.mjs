import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import test from 'node:test';
import * as T from 'three';

registerHooks({ resolve(specifier, context, nextResolve) {
  const result = nextResolve(specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier) ? specifier + '.ts' : specifier, context);
  return specifier.endsWith('.json') ? { ...result, importAttributes: { type: 'json' } } : result;
} });

const { parseGLB } = await import('./glb.ts');
const { Viewer, QUALITY } = await import('../engine/viewer.ts');
const { mulberry32 } = await import('../engine/sim.ts');

const names = ['HP_FGM_1', 'HP_FGM_2', 'HP_FGM_3'];
const mountSpec = { group: 'main', model: 'fixture', bones: ['yaw', 'pitch', 'recoil'], numBarrels: 1, muzzles: 1, barrelDiameter: 0.38, horizSector: [-150, 150], vertSector: [-5, 30], rotationSpeed: [30, 20], maxDist: 20000, shotDelay: 10 };
const sim = { ship: { size_m: [200, 30, 30], maxSpeed_kn: 30, rudderTime_s: 6, turningRadius_m: 600 }, mounts: Object.fromEntries(names.map((id) => [id, mountSpec])) };
const localPoints = [[-0.5, 0, -2], [0.5, 0, -2], [0, 1, -2]];
const sample = (mesh, i) => mesh.boneTransform(i, new T.Vector3(...localPoints[i])).applyMatrix4(mesh.matrixWorld);
function near(a, b) { assert.ok(a.distanceTo(b) < 2e-6, `${a.toArray()} != ${b.toArray()}`); }

function fixture({ rigid = false, inverseBinds = true, oneWeight = false } = {}) {
  let offset = 0;
  const buffers = [], bufferViews = [], accessors = [], nodes = [], skins = [];
  const accessor = (array, componentType, type, count) => {
    buffers.push(Buffer.from(array.buffer));
    bufferViews.push({ buffer: 0, byteOffset: offset, byteLength: array.byteLength }); offset += array.byteLength;
    return accessors.push({ bufferView: bufferViews.length - 1, componentType, type, count }) - 1;
  };
  const position = accessor(new Float32Array(localPoints.flat()), 5126, 'VEC3', 3);
  const joints = accessor(new Uint16Array([0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]), 5123, 'VEC4', 3);
  const weights = accessor(new Float32Array(oneWeight ? [0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0] : [1, 0, 0, 0, 0, 1, 0, 0, 0.25, 0.75, 0, 0]), 5126, 'VEC4', 3);
  const identity = new T.Matrix4().toArray();
  const ibm = inverseBinds ? accessor(new Float32Array([...identity, ...identity]), 5126, 'MAT4', 2) : undefined;
  const add = (node) => nodes.push(node) - 1;
  const roots = names.map((id, i) => {
    const translation = [i === 0 ? 4 : i === 1 ? -3 : 0, 1, i - 2];
    if (i === 2 || rigid) return add({ name: id, translation, mesh: 1, extras: { part: id } });
    const muzzle = add({ name: 'HP_gunFire1', translation: [0, 0, -3], extras: { muzzle: 0 } });
    const recoil = add({ name: 'Roll_Back1', extras: { bone: 'recoil' }, children: [muzzle] });
    const pitch = add({ name: 'Rotate_X', extras: { bone: 'pitch' }, children: [recoil] });
    const yaw = add({ name: 'Rotate_Y', extras: { bone: 'yaw' }, children: [pitch] });
    const skin = skins.push({ joints: [yaw, recoil], inverseBindMatrices: ibm }) - 1;
    const mesh = add({ name: 'model', mesh: 0, skin, extras: { lod: 0 } });
    return add({ name: id, translation, rotation: i === 0 ? [0, 1, 0, 0] : [0, 0, 0, 1], children: [mesh, yaw],
      extras: { part: id, rig: { version: 1, status: 'skinned', bones: ['yaw', 'pitch', 'recoil'], muzzles: 1 } } });
  });
  const json = { asset: { version: '2.0' }, scene: 0, scenes: [{ nodes: roots }], nodes, skins, accessors, bufferViews, buffers: [{ byteLength: offset }],
    materials: [{ name: 'gun-material' }], meshes: [
      { primitives: [{ attributes: { POSITION: position, JOINTS_0: joints, WEIGHTS_0: weights }, material: 0, extras: { renderSet: 'barrel', identifier: 'gun' } }] },
      { primitives: [{ attributes: { POSITION: position }, material: 0 }] },
    ] };
  const raw = JSON.stringify(json), chunk = Buffer.from(raw.padEnd(Math.ceil(raw.length / 4) * 4, ' '));
  const bytes = Buffer.concat(buffers);
  const head = Buffer.alloc(20), bin = Buffer.alloc(8);
  head.writeUInt32LE(0x46546c67, 0); head.writeUInt32LE(2, 4); head.writeUInt32LE(28 + chunk.length + bytes.length, 8);
  head.writeUInt32LE(chunk.length, 12); head.writeUInt32LE(0x4e4f534a, 16);
  bin.writeUInt32LE(bytes.length, 0); bin.writeUInt32LE(0x004e4942, 4);
  return Uint8Array.from(Buffer.concat([head, chunk, bin, bytes])).buffer;
}

function skinAt(root, id) {
  const meshes = [];
  root.getObjectByName(id).traverse((o) => { if (o.isMesh) meshes.push(o); });
  assert.equal(meshes.length, 1);
  return meshes[0];
}

test('GLB shares geometry while each mount deforms its own vertices once in world space', () => {
  const { root } = parseGLB(fixture());
  const first = skinAt(root, names[0]), second = skinAt(root, names[1]), rigid = skinAt(root, names[2]);
  assert.ok(first.isSkinnedMesh && second.isSkinnedMesh);
  assert.ok(rigid.isMesh && !rigid.isSkinnedMesh);
  assert.equal(first.geometry, second.geometry);
  assert.notEqual(first.skeleton, second.skeleton);
  assert.notEqual(first.skeleton.bones[0], second.skeleton.bones[0]);
  assert.ok(first.skeleton.bones.every((b) => b.isBone));
  assert.deepEqual(first.userData, { part: names[0], lod: 0, bone: undefined, mfm: 'gun-material', renderSet: 'barrel', identifier: 'gun' });

  near(sample(first, 0), new T.Vector3(4.5, 1, 0));
  const unaffected = sample(second, 2);
  const top = root.getObjectByName(names[0]);
  top.getObjectByName('Rotate_Y').rotation.y = -0.31;
  top.getObjectByName('Rotate_X').rotation.x = 0.42;
  top.getObjectByName('Roll_Back1').position.z = 0.18;
  root.updateMatrixWorld(true);
  const yaw = new T.Matrix4().makeRotationY(Math.PI - 0.31).setPosition(4, 1, -2);
  const recoil = yaw.clone().multiply(new T.Matrix4().makeRotationX(0.42)).multiply(new T.Matrix4().makeTranslation(0, 0, 0.18));
  near(sample(first, 0), new T.Vector3(...localPoints[0]).applyMatrix4(yaw));
  near(sample(first, 1), new T.Vector3(...localPoints[1]).applyMatrix4(recoil));
  near(sample(first, 2), new T.Vector3(...localPoints[2]).applyMatrix4(yaw).multiplyScalar(0.25).addScaledVector(new T.Vector3(...localPoints[2]).applyMatrix4(recoil), 0.75));
  near(sample(second, 2), unaffected);
});

test('picking finds a skinned triangle moved beyond its rest bounds', () => {
  const { root } = parseGLB(fixture({ inverseBinds: false, oneWeight: true }));
  const first = skinAt(root, names[0]);
  root.getObjectByName(names[0]).getObjectByName('Roll_Back1').position.x += 10;
  root.updateMatrixWorld(true);
  const r = new T.Raycaster(new T.Vector3(-6, 1.3, -5), new T.Vector3(0, 0, 1));
  const hit = r.intersectObject(first, false)[0];
  assert.ok(hit);
  near(hit.point, new T.Vector3(-6, 1.3, 0));
});

function makeViewer() {
  const camera = new T.PerspectiveCamera(34, 1.4, 0.05, 1600);
  camera.position.set(18.2, 3.9, -9.6);
  const target = new T.Vector3(0, 0.95, -0.5);
  const uniform = (value = 0) => ({ value });
  const v = Object.assign(Object.create(Viewer.prototype), {
    camera, controls: { target, update() { camera.lookAt(target); camera.updateMatrixWorld(); } }, opts: { fixed: [1120, 800] },
    scene: new T.Scene(), envScene: new T.Scene(), shipFrame: new T.Group(), panels: [],
    BOX: new T.Box3(new T.Vector3(-1, -1, -5), new T.Vector3(1, 1, 5)), WL: 0, baseFov: 34, baseAspect: 1.4,
    Q: QUALITY.low, env: 'studio', light: 'day', look: 'studio', pal: { envI: 1 }, wlOn: true, waterMode: 'full', channel: 0,
    U: Object.fromEntries('uTime uCut uCutDir uWaterOpa uWL uWater uWTint uWLLine uWLCol uGameOn uMgSwap uEmisK uAoAmt uGoldShift uPeriShift'.split(' ').map((key) => [key, uniform()])),
    WU: { uShipP: uniform(new T.Vector2()), uShipPsi: uniform() }, renderer: { shadowMap: {} },
    parts: {}, meshes: [], sets: { A: {}, B: {} }, mode: 'single', lodMode: 0, lodBias: 1,
    mounts: [], pendingShots: [], singleIdx: {}, rng: mulberry32(7), emitAcc: 0, funnelAcc: 0,
    aimBearing: null, aimDistKm: 1, aimMarkDist: 10, combat: false, simOn: false, camMode: 'orbit', reloadReal: false, dirty: 0,
  });
  v.buildStage();
  return v;
}

test('first load shows the deck, with the bow below and left of the stern; manual camera survives refresh and reset', () => {
  const v = makeViewer();
  v.loadGeometry(fixture());
  const bow = new T.Vector3(0, 0, -1).project(v.camera);
  const stern = new T.Vector3(0, 0, 1).project(v.camera);
  assert.ok(bow.x < stern.x && bow.y < stern.y);
  assert.ok(v.camera.position.clone().sub(v.controls.target).normalize().y > 0.45);
  v.setView([11, 7, 3], [0, 1, 0]);
  const manual = v.getView();
  v.setSimData(sim); v.simEnable(true); v.simReset(); v.simEnable(false); v.simEnable(true);
  v.loadGeometry(fixture(), { preserveCamera: true });
  assert.deepEqual(v.getView(), manual);
});

test('a free camera reanchors to the review angle on simulation re-entry', () => {
  const v = makeViewer();
  v.loadGeometry(fixture()); v.setSimData(sim); v.simEnable(true); v.setCamMode('free');
  v.simEnable(false);
  // OrbitControls changed the angle in review, where the free camera isn't advancing.
  v.camera.position.set(7, 7, 7); v.controls.target.set(0, 1, 0); v.controls.update();
  const reviewAngle = v.getView();
  v.simEnable(true); v.applyFreeCam();
  assert.deepEqual(v.getView(), reviewAngle);
});

test('normal firing checks the latest horizontal and vertical aim, and uses the turned muzzle', () => {
  const v = makeViewer();
  v.loadGeometry(fixture()); v.setSimData(sim); v.simEnable(true); v.setCombat(true);
  v.setWaves(0);
  assert.equal(v.fire('main', 'salvo'), 0, 'no target on the first combat frame');
  v.aimAt(23, 3);
  v.simWarm(10);
  assert.deepEqual(v.readyMounts('main').map((m) => m.id), [names[1]], 'rear-facing mount is outside its arc, static mount never counts');
  v.setAimDistKm(16);
  assert.equal(v.fire('main', 'single'), 0, 'previous ready yaw cannot fire at the new pitch');
  v.simWarm(4);
  const m = v.readyMounts('main')[0];
  assert.equal(m.id, names[1]);
  assert.equal(v.fire('main', 'single'), 1);
  v.stepSim(0); // Execute due shot without moving its particles.
  assert.ok(v.particles.n > 7);
  const muzzle = m.muzzles[0];
  const dir = new T.Vector3(0, 0, -1).transformDirection(muzzle.matrixWorld);
  const point = muzzle.getWorldPosition(new T.Vector3());
  const flashS = Math.max(0.08, mountSpec.barrelDiameter * 22 * v.motion.m2u);
  near(new T.Vector3().fromArray(v.particles.wp), point.addScaledVector(dir, 0.15 * flashS));
  near(new T.Vector3().fromArray(v.particles.vel).normalize(), dir);
});

test('reset preserves waves and camera, cancels shots and restores the real rig immediately', () => {
  const v = makeViewer();
  v.loadGeometry(fixture()); v.setSimData(sim); v.simEnable(true);
  const initial = sample(skinAt(v.ship, names[1]), 1);
  v.setWaves(0.35); v.setCombat(true); v.aimAt(35);
  v.setCamMode('free'); const manual = v.getView();
  v.simWarm(6);
  const mesh = skinAt(v.ship, names[1]);
  assert.ok(sample(mesh, 1).distanceTo(initial) > 0.2);
  v.fire('main', 'salvo', true); assert.ok(v.pendingShots.length > 1);
  v.simReset();
  assert.equal(v.motion.waves, 0.35); assert.equal(v.pendingShots.length, 0);
  assert.deepEqual(v.getView(), manual); near(sample(mesh, 1), initial);
  v.fire('main', 'salvo', true); v.simEnable(false); v.simEnable(true);
  v.stepSim(0.05);
  assert.equal(v.particles.n, 0, 'shots queued before exit do not play on re-entry');
});

test('a legacy rigid GLB still displays but cannot claim combat readiness', () => {
  const v = makeViewer();
  v.loadGeometry(fixture({ rigid: true })); v.setSimData(sim); v.simEnable(true); v.setCombat(true); v.aimAt(0);
  v.simWarm(5);
  const { mounts } = v.simSnapshot();
  assert.equal(mounts.length, names.length);
  assert.ok(mounts.every((m) => m.rigged === false && !m.aimed));
  assert.equal(v.fire('main', 'salvo'), 0);
});
