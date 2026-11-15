import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import test from 'node:test';
import * as THREE from 'three';

registerHooks({ resolve(specifier, context, nextResolve) {
  const result = nextResolve(specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier) ? specifier + '.ts' : specifier, context);
  return specifier.endsWith('.json') ? { ...result, importAttributes: { type: 'json' } } : result;
} });

const { Viewer, QUALITY } = await import('../engine/viewer.ts');

test('changing and resetting wake tint keeps the environment render target', () => {
  // Exercise the real stage, lighting and FX methods, with only GPU work stubbed.
  const uniform = () => ({ value: 0 });
  const uniforms = (names) => Object.fromEntries(names.split(' ').map((name) => [name, uniform()]));
  let generated = 0;
  const viewer = Object.assign(Object.create(Viewer.prototype), {
    scene: new THREE.Scene(), envScene: new THREE.Scene(), shipFrame: new THREE.Group(), panels: [],
    BOX: new THREE.Box3(new THREE.Vector3(-1, -1, -5), new THREE.Vector3(1, 1, 5)), WL: 0,
    Q: QUALITY.low, env: 'studio', light: 'day', look: 'studio', wlOn: true, waterMode: 'full', channel: 0,
    U: uniforms('uTime uCut uCutDir uWaterOpa uWL uWater uWTint uWLLine uWLCol uGameOn uMgSwap uMgGamma uEmisK uAoAmt uGoldShift uPeriShift'), WU: {},
    compMat: new THREE.ShaderMaterial({ uniforms: uniforms('uExposure uVig uRaw') }), bloom: {},
    matCache: new Map(), renderer: { shadowMap: {} },
    pmrem: { fromScene() { generated++; return new THREE.WebGLRenderTarget(1, 1); } },
    dirty: 0,
  });
  viewer.buildStage(); viewer.applyEnv();
  const initialEnvironment = viewer.scene.environment;
  const foam = () => viewer.foamMat.uniforms.uFoam.value;
  const tint = { wake: { tint: '#ff0000', strength: 1 } };

  viewer.dirty = 0; viewer.setFx(tint);
  assert.equal(viewer.fx, tint);
  assert.deepEqual(foam().toArray(), [1 / 0.2126, 0, 0]);
  assert.equal(viewer.dirty, 2);
  assert.equal(viewer.scene.environment, initialEnvironment);
  assert.equal(generated, 1);

  viewer.setEnv('water'); viewer.setLight('night');
  const nightEnvironment = viewer.scene.environment;
  const nightFoam = new THREE.Color('#b9c6d2').convertSRGBToLinear();
  assert.notEqual(nightEnvironment, initialEnvironment);
  assert.equal(generated, 3);
  assert.deepEqual(foam().toArray(), [nightFoam.r * (1 / 0.2126), 0, 0]);

  viewer.dirty = 0; viewer.setFx(null);
  assert.equal(viewer.fx, null);
  assert.deepEqual(foam().toArray(), nightFoam.toArray());
  assert.equal(viewer.dirty, 2);
  assert.equal(viewer.scene.environment, nightEnvironment);
  assert.equal(generated, 3);
});
