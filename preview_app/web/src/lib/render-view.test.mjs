import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import test from 'node:test';

registerHooks({ resolve(specifier, context, nextResolve) {
  const result = nextResolve(specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier) ? specifier + '.ts' : specifier, context);
  return specifier.endsWith('.json') ? { ...result, importAttributes: { type: 'json' } } : result;
} });
globalThis.window = { WM_API_BASE: '' };
globalThis.location = { search: '?project=variant-A&render=wait' };
const { renderViewPreset } = await import('../render/render.ts');
const { Viewer } = await import('../engine/viewer.ts');

test('headless deck selects the same fitted preset as the workbench', () => {
  const viewer = { fitCentered: (direction, target, fill) => ({ direction, target, fill }) };
  const pose = (name) => Viewer.prototype.presetPose.call(viewer, name);
  const deck = pose('deck');
  assert.ok(deck);
  assert.deepEqual(pose(renderViewPreset('deck')), deck);
  assert.notDeepEqual(deck, pose('hero'));
});

test('existing headless aliases and the hero fallback are preserved', () => {
  assert.equal(renderViewPreset('fq'), 'bow');
  assert.equal(renderViewPreset('rear-quarter'), 'stern');
  assert.equal(renderViewPreset('quarter'), 'hero');
  assert.equal(renderViewPreset(''), 'hero');
});
