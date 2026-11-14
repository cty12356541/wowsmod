import assert from 'node:assert/strict';
import test from 'node:test';
import { matchingVerification, buildDiffers, createCompatibilityLoader } from './compatibility.ts';

const record = (id, build, revision, outcome, recorded_unix) => ({ id, project_id: 'a', build, revision, outcome, recorded_unix });
const info = (records = []) => ({ project_id: 'a', current_revision: 'sha256:current', target_build: '123', records, install: { state: 'installed' } });

test('installed and production status do not imply verification', () => {
  assert.equal(matchingVerification({ ...info(), status: 'complete' }), null);
  assert.equal(buildDiffers('122', '123'), true);
  assert.equal(buildDiffers('', '123'), false);
  assert.equal(buildDiffers('123', null), false);
});
test('only exact project, content and build match; newer failure supersedes pass', () => {
  const passed = record('old', '123', 'sha256:current', 'passed', 1);
  const failed = record('new', '123', 'sha256:current', 'failed', 2);
  assert.equal(matchingVerification(info([passed, failed])).id, 'new');
  assert.equal(matchingVerification(info([record('old-build', '122', 'sha256:current', 'passed', 5)])), null);
  assert.equal(matchingVerification(info([record('old-content', '123', 'sha256:old', 'passed', 5)])), null);
  assert.equal(matchingVerification(info([{ ...passed, project_id: 'b' }])), null);
  assert.equal(matchingVerification({ ...info([passed]), current_revision: null }), null);
});
test('same-second entries use append order to choose latest observation', () => {
  assert.equal(matchingVerification(info([record('pass', '123', 'sha256:current', 'passed', 2), record('fail', '123', 'sha256:current', 'failed', 2)])).id, 'fail');
});
test('clock rollback cannot resurrect an earlier passing record', () => {
  assert.equal(matchingVerification(info([record('pass', '123', 'sha256:current', 'passed', 20), record('fail', '123', 'sha256:current', 'failed', 10)])).id, 'fail');
});
test('slow old detail or error cannot replace a different project or unmounted view', async () => {
  const pending = []; let state;
  const loader = createCompatibilityLoader((id, signal) => new Promise((resolve, reject) => pending.push({ id, signal, resolve, reject })), (data, error, loading) => { state = { data, error, loading }; });
  const first = loader.load('a'); const second = loader.load('b');
  assert.equal(pending[0].signal.aborted, true);
  pending[1].resolve({ ...info(), project_id: 'b' }); await second;
  pending[0].reject(new Error('old failure')); await first;
  assert.equal(state.data.project_id, 'b'); assert.equal(state.error, '');
  const last = loader.load('a'); loader.cancel();
  pending[2].resolve(info()); await last;
  assert.equal(state.data, null); assert.equal(state.loading, true);
});
