import assert from 'node:assert/strict';
import test from 'node:test';
import { createLibraryListLoader } from './library.ts';

function fixture() {
  const requests = [];
  const before = { projects: [{ id: 'variant', favorite: false }] };
  const after = { projects: [{ id: 'variant', favorite: true }] };
  const state = { data: before, error: '', loading: false };
  const loader = createLibraryListLoader(
    () => new Promise((resolve, reject) => requests.push({ resolve, reject })),
    {
      data: (value) => { state.data = value; },
      error: (value) => { state.error = value; },
      loading: (value) => { state.loading = value; },
    },
  );
  return { loader, requests, state, before, after };
}

test('a slow manual refresh cannot overwrite the catalog loaded after editing a variant', async () => {
  const { loader, requests, state, before, after } = fixture();
  const manual = loader.load();
  const edited = loader.load();
  requests[1].resolve(after);
  await edited;
  requests[0].resolve(before);
  await manual;
  assert.equal(state.data, after);
});

test('a stale failed refresh cannot publish an error or clear the current loading state', async () => {
  const { loader, requests, state, after } = fixture();
  const manual = loader.load();
  const edited = loader.load();
  requests[0].reject(new Error('Old connection failed'));
  await manual;
  assert.equal(state.error, '');
  assert.equal(state.loading, true);
  requests[1].resolve(after);
  await edited;
  assert.equal(state.data, after);
  assert.equal(state.loading, false);
});

test('unmount invalidates pending results even if the library starts loading again', async () => {
  const { loader, requests, state, before, after } = fixture();
  const old = loader.load();
  loader.invalidate();
  requests[0].resolve(after);
  await old;
  assert.deepEqual(state, { data: before, error: '', loading: true });
  const fresh = loader.load();
  requests[1].resolve(after);
  await fresh;
  assert.deepEqual(state, { data: after, error: '', loading: false });
});
