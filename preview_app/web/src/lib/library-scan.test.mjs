import test from 'node:test';
import assert from 'node:assert/strict';
import { selectedCandidates, createScanLoader } from './library-scan.ts';
test('only explicitly chosen new candidates can be imported, without duplicates', () => {
  const candidates = [{ candidate_id:'a', status:'new' }, {candidate_id:'b',status:'new'}, {candidate_id:'c',status:'registered'}, {candidate_id:'d',status:'invalid'}];
  assert.deepEqual(selectedCandidates(candidates, []), []);
  assert.deepEqual(selectedCandidates(candidates, ['b','b','c','d','foreign']), ['b']);
});
test('changing root clears previous results and late scans cannot publish into another root', async () => {
  const pending=[];let state;
  const loader=createScanLoader((root,signal)=>new Promise((resolve,reject)=>pending.push({root,signal,resolve,reject})),(data,error,loading)=>{state={data,error,loading};});
  const first=loader.load('a');loader.clear();const second=loader.load('b');
  assert.equal(pending[0].signal.aborted,true);
  pending[1].resolve({root:'b',scan_id:'b',truncated:true,candidates:[]});await second;
  pending[0].resolve({root:'a',scan_id:'a',candidates:[]});await first;
  assert.equal(state.data.root,'b');assert.equal(state.data.truncated,true);
});
test('scan failures are visible and unmounted loaders do not publish', async () => {
  const pending=[];let state;
  const loader=createScanLoader(()=>new Promise((resolve,reject)=>pending.push({resolve,reject})),(data,error,loading)=>{state={data,error,loading};});
  const failed=loader.load('a');pending[0].reject(new Error('scan failed'));await failed;
  assert.equal(state.error,'scan failed');assert.equal(state.loading,false);
  const stale=loader.load('b');loader.clear();pending[1].reject(new Error('stale'));await stale;
  assert.deepEqual(state,{data:null,error:'',loading:false});
});
