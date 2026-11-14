import assert from 'node:assert/strict';
import test from 'node:test';
import { filterProjects, groupProjects, defaultFilters } from './library.ts';
import { LibraryThumbnailCache, representativeProject } from './library-thumbnail.ts';

const project = (id, patch = {}) => ({
  id, path: `C:/mods/${id}`, available: true, status: 'working', tags: [],
  display_name: id, added_unix: 1, opened_unix: 1, favorite: false,
  ship_metadata: { key: 'same-ship' }, ...patch,
});
const status = (state, url = null, stale = false) => ({ ok: true, state, url, stale, reason: null, width: 640, height: 360 });
const turn = () => new Promise((resolve) => setImmediate(resolve));
const nextPoll = async (t) => { t.mock.timers.tick(2000); await turn(); };

test('a ship cover uses the first available project in its current sorted filter results', () => {
  const a = project('a');
  const b = project('b', { favorite: true, opened_unix: 2 });
  const missing = project('missing', { available: false, opened_unix: 9 });
  const group = (filters) => groupProjects(filterProjects([a, b, missing], { ...defaultFilters, ...filters }))[0].projects;
  assert.equal(representativeProject(group()).id, 'b');
  assert.equal(representativeProject(group({ sort: 'name' })).id, 'a');
  assert.equal(representativeProject(group({ sort: 'name', favorites: true })).id, 'b');
  assert.equal(representativeProject([missing]), undefined);
});

test('visible cards for the same project share status requests and stop polling at ready', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const results = [status('queued'), status('rendering'), status('ready', '/a.png')];
  const a = project('a');
  const cache = new LibraryThumbnailCache(async () => results.shift());
  assert.equal(cache.read(a).thumbnail, null);
  assert.equal(results.length, 3);
  const leave1 = cache.subscribe(a, () => {});
  const leave2 = cache.subscribe(a, () => {});
  await turn();
  assert.equal(cache.read(a).thumbnail.state, 'queued');
  await nextPoll(t);
  assert.equal(cache.read(a).thumbnail.state, 'rendering');
  leave1();
  await nextPoll(t);
  assert.equal(cache.read(a).thumbnail.url, '/a.png');
  await nextPoll(t);
  assert.equal(cache.read(a).error, null);
  assert.equal(cache.read(a).thumbnail.state, 'ready');
  leave2();
});

test('existing cover survives refresh, rendering and failure until a new URL is ready', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const a = project('a');
  const results = [status('ready', '/old.png'), status('rendering', '/old.png', true), status('failed', '/old.png', true), status('ready', '/new.png')];
  const cache = new LibraryThumbnailCache(async () => results.shift());
  const leave = cache.subscribe(a, () => {});
  await turn();
  cache.revalidateVisible();
  assert.equal(cache.read(a).thumbnail.url, '/old.png');
  await turn();
  assert.equal(cache.read(a).thumbnail.stale, true);
  await nextPoll(t);
  assert.equal(cache.read(a).thumbnail.state, 'failed');
  assert.equal(cache.read(a).thumbnail.url, '/old.png');
  cache.retry(a);
  await turn();
  assert.equal(cache.read(a).thumbnail.url, '/new.png');
  leave();
});

test('the last subscriber cancels scheduled polls and aborts any in-flight request', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const a = project('a');
  const requests = [];
  const cache = new LibraryThumbnailCache((_id, signal) => {
    requests.push(signal);
    return requests.length === 1 ? Promise.resolve(status('queued')) : new Promise((resolve) => { signal.addEventListener('abort', () => resolve(status('ready', '/cancelled.png'))); });
  });
  const stop1 = cache.subscribe(a, () => {});
  await turn();
  stop1();
  await nextPoll(t);
  assert.equal(requests.length, 1);
  const stop2 = cache.subscribe(a, () => {});
  assert.equal(requests.length, 2);
  stop2();
  assert.equal(requests[1].aborted, true);
  await turn();
  assert.equal(cache.read(a).thumbnail.url, null);
  assert.equal(cache.read(a).pending, false);
});

test('a long-running cover has a finite polling budget and can explicitly retry', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
  const a = project('a');
  const methods = [];
  const cache = new LibraryThumbnailCache(async (_id, _signal, refresh) => {
    methods.push(refresh);
    return refresh ? status('ready', '/a.png') : status('queued');
  });
  const stop = cache.subscribe(a, () => {});
  await turn();
  for (let i = 0; i < 40; i++) await nextPoll(t);
  assert.equal(cache.read(a).pollingExpired, true);
  const count = methods.length;
  assert.ok(count > 1 && count <= 31);
  await nextPoll(t);
  assert.equal(methods.length, count);
  cache.retry(a);
  await turn();
  assert.equal(methods.at(-1), true);
  assert.equal(cache.read(a).thumbnail.state, 'ready');
  stop();
});

test('image errors can retry the same version and source identity prevents stale covers on relocation', async () => {
  const a = project('a');
  const b = project('b');
  const relocated = project('a', { path: 'D:/new/a' });
  const cache = new LibraryThumbnailCache(async (id) => status('ready', `/${id}.png`));
  const stopA = cache.subscribe(a, () => {});
  const stopB = cache.subscribe(b, () => {});
  await turn();
  assert.equal(cache.read(a).thumbnail.url, '/a.png');
  assert.equal(cache.read(b).thumbnail.url, '/b.png');
  assert.equal(cache.read(relocated).thumbnail, null);
  cache.imageFailed(a, '/a.png');
  assert.equal(cache.read(a).imageFailed, true);
  cache.retry(a);
  await turn();
  assert.equal(cache.read(a).thumbnail.url, '/a.png');
  assert.equal(cache.read(a).imageFailed, false);
  assert.equal(cache.read(a).imageAttempt, 1);
  assert.equal(cache.read(b).imageAttempt, 0);
  stopA();
  stopB();
});
