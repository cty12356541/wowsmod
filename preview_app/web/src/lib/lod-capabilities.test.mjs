import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import test from 'node:test';
import * as THREE from 'three';

registerHooks({ resolve(specifier, context, nextResolve) {
  const result = nextResolve(specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier) ? specifier + '.ts' : specifier, context);
  return specifier.endsWith('.json') ? { ...result, importAttributes: { type: 'json' } } : result;
} });

const { Viewer } = await import('../engine/viewer.ts');
const { ShipMotion } = await import('../engine/sim.ts');
const extents = [6, 10, 20, 40].map((extent, lod) => ({ lod, extent }));
const singleLod = { 'hull/bow': [0], 'hull/stern': [0] };
const sparseLods = { 'hull/bow': [0, 2], 'hull/stern': [0] };

function fixture(parts) {
  const bin = Buffer.from(new Float32Array([0, 0, -1, 1, 0, -1, 0, 1, 0]).buffer);
  const nodes = [], roots = [];
  for (const [part, levels] of Object.entries(parts)) {
    roots.push(nodes.length);
    const children = [];
    nodes.push({ name: part, children, extras: { part, lods: extents } });
    for (const lod of levels) {
      children.push(nodes.length);
      nodes.push({ mesh: 0, extras: { lod } });
    }
  }
  const data = Buffer.from(JSON.stringify({
    asset: { version: '2.0' }, scenes: [{ nodes: roots }], nodes,
    buffers: [{ byteLength: bin.length }], bufferViews: [{ buffer: 0, byteLength: bin.length }],
    accessors: [{ bufferView: 0, componentType: 5126, type: 'VEC3', count: 3 }],
    meshes: [{ primitives: [{ attributes: { POSITION: 0 } }] }],
  }));
  const json = Buffer.concat([data, Buffer.alloc((4 - data.length % 4) % 4, 0x20)]);
  const bytes = Buffer.alloc(28 + json.length + bin.length);
  bytes.writeUInt32LE(0x46546c67, 0); bytes.writeUInt32LE(2, 4); bytes.writeUInt32LE(bytes.length, 8);
  bytes.writeUInt32LE(json.length, 12); bytes.writeUInt32LE(0x4e4f534a, 16); json.copy(bytes, 20);
  bytes.writeUInt32LE(bin.length, 20 + json.length); bytes.writeUInt32LE(0x004e4942, 24 + json.length);
  bin.copy(bytes, 28 + json.length);
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
}

function viewerForGeometry() {
  const camera = new THREE.PerspectiveCamera();
  camera.position.set(0, 0, 3);
  return Object.assign(Object.create(Viewer.prototype), {
    camera, controls: { target: new THREE.Vector3() }, shipFrame: new THREE.Group(),
    motion: new ShipMotion({}), mounts: [], pendingShots: [], aimBearing: null, simOn: true, parts: {}, meshes: [],
    lodMode: 0, lodBias: 1, renderer: { shadowMap: {} }, cameraEdited: true, sets: { A: {}, B: {} },
    fitStage() {}, neutralMat: () => new THREE.MeshBasicMaterial(), applyMeshFlags() {},
    buildSideFoam() {}, setBaselineManifest() {}, setFx() {}, gc() {},
    applyManifest: async () => ({ failed: [] }), setMode() {},
  });
}

const meshesAt = (v, id) => v.parts[id].meshes.filter((mesh) => mesh.visible).map((mesh) => mesh.userData.lod);

test('GLB nodes that declare absent levels do not advertise those levels', () => {
  const v = viewerForGeometry();
  const stats = v.loadGeometry(fixture(singleLod));
  assert.deepEqual(v.simSnapshot().availableLods, [0]);
  assert.equal(stats.parts['hull/bow'].lods, 1);
  v.setLodMode(2);
  assert.equal(v.lodMode, 0);
  assert.deepEqual(meshesAt(v, 'hull/bow'), [0]);
});

test('manual LOD chooses the real level for each part and preserves visibility toggles', () => {
  const v = viewerForGeometry();
  v.loadGeometry(fixture(sparseLods));
  assert.deepEqual(v.simSnapshot().availableLods, [0, 2]);
  v.setLodMode(2);
  assert.deepEqual(v.lodState(), { 'hull/bow': 2, 'hull/stern': 0 });
  assert.deepEqual(meshesAt(v, 'hull/bow'), [2]);
  assert.deepEqual(meshesAt(v, 'hull/stern'), [0]);
  v.setPartVisible('hull/stern', false);
  assert.deepEqual(v.lodState(), { 'hull/bow': 2, 'hull/stern': -1 });
  assert.deepEqual(meshesAt(v, 'hull/stern'), []);
});

test('auto uses real distance thresholds and keeps the last real geometry visible', () => {
  const v = viewerForGeometry();
  v.loadGeometry(fixture(sparseLods));
  v.setLodMode('auto');
  assert.deepEqual(v.lodState(), { 'hull/bow': 0, 'hull/stern': 0 });
  v.camera.position.z = 8;
  v.applyLod();
  assert.deepEqual(meshesAt(v, 'hull/bow'), [2]);
  assert.deepEqual(meshesAt(v, 'hull/stern'), [0]);
  v.camera.position.z = 200;
  v.applyLod();
  assert.deepEqual(meshesAt(v, 'hull/bow'), [2]);
  assert.deepEqual(meshesAt(v, 'hull/stern'), [0]);
  v.loadGeometry(fixture(singleLod), { preserveCamera: true });
  assert.equal(v.lodMode, 0);
  assert.deepEqual(meshesAt(v, 'hull/bow'), [0]);
});

test('a live geometry refresh falls back from a removed level in the store and viewer', async () => {
  globalThis.window = { WM_API_BASE: '', WM_NO_SSE: true, setInterval, clearInterval };
  globalThis.location = { search: '?project=lod-fixture' };
  globalThis.localStorage = { getItem: () => null };
  globalThis.document = { title: '', documentElement: {} };
  let generation = 0;
  globalThis.fetch = async (url) => {
    if (url.includes('/geometry/')) return new Response(fixture(generation ? singleLod : sparseLods));
    const value = url.endsWith('/project')
      ? { project: { name: 'LOD fixture' }, ship: { code: 'fixture' }, capabilities: { preview: true, simulation: false } }
      : url.endsWith('/steps') ? { head: 'current', steps: [] }
      : url.includes('/model?') ? { geometry: { hash: String(generation), url: `/api/v1/projects/lod-fixture/geometry/${generation}.glb` }, materials: {} }
      : url.includes('/parts?') ? { parts: [] } : url.includes('/textures?') ? { textures: [] } : { ok: true };
    return new Response(JSON.stringify(value), { headers: { 'Content-Type': 'application/json' } });
  };
  const { actions, boot, disposeWorkbench, resync, useStore } = await import('../store.ts');
  const v = viewerForGeometry();
  try {
    await boot(() => v);
    assert.equal(useStore.getState().phase, 'ready', JSON.stringify(useStore.getState().fatal));
    actions.setLodMode(2);
    assert.equal(useStore.getState().lodMode, 2);
    generation++;
    await resync();
    assert.equal(v.lodMode, 0);
    assert.equal(useStore.getState().lodMode, 0);
    assert.deepEqual(meshesAt(v, 'hull/bow'), [0]);
  } finally {
    disposeWorkbench();
  }
});
