import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import test from 'node:test';
import * as THREE from 'three';

registerHooks({ resolve(specifier, context, nextResolve) {
  const result = nextResolve(specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier) ? specifier + '.ts' : specifier, context);
  return specifier.endsWith('.json') ? { ...result, importAttributes: { type: 'json' } } : result;
} });

const { Viewer, QUALITY, CHANNELS } = await import('../engine/viewer.ts');
const { gameLook, palette } = await import('../engine/env.ts');

// fx basenames and parameter names come from the real manifest/MFM contract
// audited in out/robin-refine-20261116/night-diagnosis/port-material-diagnosis.md.
const material = (fx, params = {}) => ({ family: 'emissive', fx, params, textures: {} });
function viewer() {
  return Object.assign(Object.create(Viewer.prototype), {
    look: 'game', lookOv: {}, light: 'night', Q: QUALITY.high,
    channel: 0, U: { uChannel: { value: 0 }, uMgGamma: { value: 1 } }, pal: palette('studio', 'night'),
    compMat: new THREE.ShaderMaterial({ uniforms: { uRaw: { value: 0 } } }),
    matCache: new Map(), sets: { A: {}, B: {} }, meshes: [],
    renderer: { shadowMap: {} }, dirty: 0, simOn: false,
    flashLight: { intensity: 0 }, particles: { n: 0 },
    bloom: { strength: 0, threshold: 0, radius: 0 },
  });
}
function compiled(view, def) {
  const m = view.buildMaterial(def, { mg: new THREE.Texture() }, 'fixture');
  const shader = {
    uniforms: THREE.UniformsUtils.clone(THREE.ShaderLib.standard.uniforms),
    vertexShader: THREE.ShaderLib.standard.vertexShader,
    fragmentShader: THREE.ShaderLib.standard.fragmentShader,
  };
  m.onBeforeCompile(shader, {});
  return { m, shader };
}

test('an fx-only manifest change replaces cached material and reports it changed', async () => {
  const v = viewer();
  const manifest = (fx) => ({ materials: { 'hull.mfm': material(fx) } });
  const legacy = manifest('PBS_ship_emissive.fx');
  v.setBaselineManifest(legacy);
  await v.applyManifest('B', legacy);
  const original = v.sets.B['hull.mfm'];
  assert.equal(v.changedMfm.size, 0);
  await v.applyManifest('B', manifest('ship_emissive_material_skinned.fx'));
  assert.notEqual(v.sets.B['hull.mfm'], original);
  assert.ok(v.changedMfm.has('hull.mfm'));
});

test('verified legacy effects bind their own defaults without treating modern emissive as legacy', () => {
  const v = viewer();
  const modern = compiled(v, material('ship_emissive_material_skinned.fx'));
  for (const fx of ['PBS_ship_emissive.fx', 'PBS_ship_emissive_skinned.fx']) {
    const { m, shader } = compiled(v, material(fx));
    assert.notEqual(m.customProgramCacheKey(), modern.m.customProgramCacheKey(), fx);
    const values = ['uLegacyGlossRemap', 'uLegacySpecularMul', 'uLegacySpecularPow', 'uLegacyAlbedoMul', 'uLegacyAlbedoToSpecular'].map((key) => shader.uniforms[key]?.value);
    assert.deepEqual(values, [0.75, 3, 4, 1.5, 0.9], fx);
  }
});

test('explicit zero legacy parameters survive manifest binding', () => {
  const { shader } = compiled(viewer(), material('PBS_ship_emissive.fx', {
    g_legacyGlossRemap: 0, g_legacySpecularMul: 0, g_legacySpecularPow: 0,
    g_legacyAlbedoMul: 0, g_legacyAlbedoToSpecular: 0,
  }));
  assert.deepEqual(['uLegacyGlossRemap', 'uLegacySpecularMul', 'uLegacySpecularPow', 'uLegacyAlbedoMul', 'uLegacyAlbedoToSpecular'].map((key) => shader.uniforms[key]?.value), [0, 0, 0, 0, 0]);
});

test('invalid legacy scalars use reflected defaults instead of poisoning shader uniforms', () => {
  const { shader } = compiled(viewer(), material('PBS_ship_emissive.fx', {
    g_legacyGlossRemap: Number.NaN, g_legacySpecularMul: Infinity,
    g_legacySpecularPow: '4', g_legacyAlbedoMul: null, g_legacyAlbedoToSpecular: -Infinity,
  }));
  assert.deepEqual(['uLegacyGlossRemap', 'uLegacySpecularMul', 'uLegacySpecularPow', 'uLegacyAlbedoMul', 'uLegacyAlbedoToSpecular'].map((key) => shader.uniforms[key]?.value), [0.75, 3, 4, 1.5, 0.9]);
});

test('unreviewed legacy-looking effects keep the fallback instead of inheriting legacy conversion', () => {
  const v = viewer();
  const fallback = compiled(v, material('unknown.fx'));
  const legacy = compiled(v, material('PBS_ship_emissive.fx'));
  for (const fx of ['PBS_ship.fx', 'PBS_ship_glass.fx']) {
    const { m } = compiled(v, material(fx));
    assert.equal(m.customProgramCacheKey(), fallback.m.customProgramCacheKey());
    assert.notEqual(m.customProgramCacheKey(), legacy.m.customProgramCacheKey());
  }
});

test('game day and night default to R gloss / G metal while explicit swap remains diagnostic', () => {
  const v = viewer();
  for (const light of ['day', 'night']) {
    v.light = light;
    assert.equal(v.gameParams().mgSwap, 0);
    v.lookOv = { mgSwap: 1 };
    assert.equal(v.gameParams().mgSwap, 1);
    v.lookOv = {};
  }
});

test('legacy gamma override reaches existing shader uniforms and resets without changing lights', () => {
  const v = viewer();
  const uniform = () => ({ value: 0 });
  Object.assign(v, {
    scene: new THREE.Scene(), envScene: new THREE.Scene(), shipFrame: new THREE.Group(), panels: [],
    BOX: new THREE.Box3(new THREE.Vector3(-1, -1, -5), new THREE.Vector3(1, 1, 5)), WL: 0,
    env: 'studio', wlOn: true, waterMode: 'full', WU: {},
    pmrem: { fromScene() { return new THREE.WebGLRenderTarget(1, 1); } },
  });
  Object.assign(v.U, Object.fromEntries('uTime uCut uCutDir uWaterOpa uWL uWater uWTint uWLLine uWLCol uGameOn uMgSwap uEmisK uAoAmt uAoFloor uGoldShift uPeriShift uTmP uTmS uSat uFogCol uFogDen uGameRaw'.split(' ').map((key) => [key, uniform()])));
  v.U.uGain = { value: new THREE.Vector3() }; v.U.uToe = { value: new THREE.Vector3() };
  Object.assign(v.compMat.uniforms, { uExposure: uniform(), uVig: uniform() });
  v.buildStage(); v.applyEnv();
  const { shader } = compiled(v, material('PBS_ship_emissive.fx'));
  const lights = [v.sun.intensity, v.hemi.intensity];
  assert.equal(shader.uniforms.uMgGamma.value, 1);
  v.setLookOverride({ mgGamma: 2 });
  assert.equal(shader.uniforms.uMgGamma.value, 2);
  assert.deepEqual([v.sun.intensity, v.hemi.intensity], lights);
  v.setLookOverride({});
  assert.equal(shader.uniforms.uMgGamma.value, 1);
  v.setLookOverride({ mgGamma: Infinity });
  assert.equal(shader.uniforms.uMgGamma.value, 1);
});

test('night and active effects never Bloom albedo/normal/wire/mask diagnostics', () => {
  const v = viewer();
  v.sets.B.hull = { userData: { family: 'emissive' } };
  v.simOn = true; v.flashLight.intensity = 8; v.particles.n = 3;
  for (const name of ['albedo', 'normal', 'wire', 'mask']) {
    v.setChannel(CHANNELS.indexOf(name));
    assert.equal(v.bloomOn(), false, name);
    assert.equal(v.compMat.uniforms.uRaw.value, 1, name);
  }
});

test('final and contextual emissive preserve their Bloom responses and quality guard', () => {
  const v = viewer();
  v.setChannel(0);
  assert.equal(v.bloomOn(), true);
  assert.equal(v.bloom.strength, gameLook('night').bloomS);
  assert.equal(v.bloom.threshold, gameLook('night').bloomT);
  v.setChannel(3);
  assert.equal(v.bloomOn(), true);
  assert.equal(v.bloom.strength, 0.8);
  assert.equal(v.bloom.threshold, 0.3);
  v.Q = QUALITY.low;
  assert.equal(v.bloomOn(), false);
  v.Q = QUALITY.high; v.light = 'day'; v.setChannel(0);
  assert.equal(v.bloomOn(), false);
  v.particles.n = 1;
  assert.equal(v.bloomOn(), true);
});
