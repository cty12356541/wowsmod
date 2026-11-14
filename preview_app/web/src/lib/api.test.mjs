import { registerHooks } from 'node:module';
import test from 'node:test';
import assert from 'node:assert/strict';
registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier)) return nextResolve(specifier + '.ts', context);
  return nextResolve(specifier, context);
} });
globalThis.window = { WM_API_BASE: '', setInterval, clearInterval };
globalThis.location = { search: '?project=variant-B' };
const requests = [];
globalThis.fetch = async (url, options) => {
  requests.push({ url, options });
  return new Response(JSON.stringify(url.endsWith('/apply') ? { ok: true, previous: [], selected: { name: 'B' }, dry_run: JSON.parse(options.body).dry_run } : { ok: true }), { headers: { 'Content-Type': 'application/json' } });
};
const { api, libraryApi, disableLive, disposeRequests } = await import('./api.ts');
test('health/library stay at root while workbench operations carry explicit project scope', async () => {
  await api.health(); await libraryApi.list(); await api.project(); await api.uninstall();
  assert.deepEqual(requests.slice(-4).map((r) => r.url), ['/api/v1/health', '/api/v1/library', '/api/v1/projects/variant-B/project', '/api/v1/projects/variant-B/uninstall']);
});
test('apply is explicit dry run followed only by a separately invoked real request', async () => {
  const before = requests.length;
  const plan = await api.apply(true);
  assert.equal(plan.dry_run, true); assert.equal(requests.length, before + 1);
  assert.deepEqual(JSON.parse(requests.at(-1).options.body), { dry_run: true });
  await api.apply(false);
  assert.deepEqual(JSON.parse(requests.at(-1).options.body), { dry_run: false });
});
test('blob and legacy resource URLs remain within the selected project', () => {
  assert.equal(api.blobUrl('abc'), '/api/v1/projects/variant-B/blob/abc.png');
  assert.equal(api.resourceUrl('/api/v1/geometry/abc.glb'), '/api/v1/projects/variant-B/geometry/abc.glb');
});
test('thumbnail status and refresh address the encoded library project, independent of the workbench scope', async () => {
  globalThis.fetch = async (url, options) => {
    requests.push({ url, options });
    return Response.json({ ok: true, state: 'ready', url: '/api/v1/library/projects/a%2Fb/thumbnail/test.png', width: 640, height: 360, stale: false, reason: null });
  };
  const expected = '/api/v1/library/projects/a%2Fb/thumbnail';
  const status = await libraryApi.thumbnail('a/b');
  assert.equal(requests.at(-1).url, expected);
  assert.equal(requests.at(-1).options.method, 'GET');
  assert.equal(status.url, expected + '/test.png');
  await libraryApi.refreshThumbnail('a/b');
  assert.equal(requests.at(-1).url, expected + '/refresh');
  assert.equal(requests.at(-1).options.method, 'POST');
  assert.deepEqual(JSON.parse(requests.at(-1).options.body), {});
});
test('unobserving a thumbnail aborts its request without changing the workbench connection', async () => {
  globalThis.fetch = (_url, options) => new Promise((_resolve, reject) => options.signal.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError'))));
  const before = api.conn();
  const controller = new AbortController();
  const pending = libraryApi.thumbnail('a', controller.signal);
  controller.abort();
  await assert.rejects(pending, { name: 'AbortError' });
  assert.equal(api.conn(), before);
});
test('disposing workbench cancels outstanding geometry requests', async () => {
  globalThis.fetch = (_url, options) => new Promise((_resolve, reject) => options?.signal?.addEventListener('abort', () => reject(Object.assign(new Error('aborted'), { name: 'AbortError' }))));
  const pending = api.geometry('/api/v1/geometry/test.glb').then(() => 'resolved', () => 'cancelled');
  disposeRequests();
  const result = await Promise.race([pending, new Promise((resolve) => setTimeout(() => resolve('still pending'), 30))]);
  assert.equal(result, 'cancelled');
});
test.after(() => disableLive());
