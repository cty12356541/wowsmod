import { registerHooks } from 'node:module';
import test from 'node:test';
import assert from 'node:assert/strict';
registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier)) return nextResolve(specifier + '.ts', context);
  return nextResolve(specifier, context);
} });
globalThis.window = { createImageBitmap: true };
globalThis.createImageBitmap = async () => ({ width: 2, height: 2 });
const urls = [];
globalThis.fetch = async (url) => { urls.push(url); return new Response('png'); };
const { getTex, texGC } = await import('./tex.ts');
test('same texture hash in two project scopes cannot reuse another project resource request', async () => {
  const a = await getTex({ hash: 'same', url: '/api/v1/projects/A/blob/same.png' }, { srgb: true, max: 512 });
  const b = await getTex({ hash: 'same', url: '/api/v1/projects/B/blob/same.png' }, { srgb: true, max: 512 });
  assert.notEqual(a, b);
  assert.deepEqual(urls, ['/api/v1/projects/A/blob/same.png?max=512', '/api/v1/projects/B/blob/same.png?max=512']);
});
test.after(() => texGC(new Set()));
