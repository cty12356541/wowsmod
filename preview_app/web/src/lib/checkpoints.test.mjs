import assert from 'node:assert/strict';
import test from 'node:test';
import { checkpointChanges, createCheckpointLoader } from './checkpoints.ts';
test('checkpoint diff totals include current additions, removals and changed files',()=>{
 assert.equal(checkpointChanges({added:['new'],removed:['gone'],changed:['paint'],unchanged_count:20}),3);
 assert.equal(checkpointChanges({added:[],removed:[],changed:[],unchanged_count:20}),0);
});
test('late history and failures cannot overwrite a newer snapshot list or detached view',async()=>{
 const pending=[];let state;
 const loader=createCheckpointLoader(signal=>new Promise((resolve,reject)=>pending.push({signal,resolve,reject})),(data,error,busy)=>{state={data,error,busy};});
 const old=loader.load(),fresh=loader.load();pending[1].resolve({checkpoints:[{id:'new',preview:{state:'failed',reason:'no model'}}]});await fresh;
 pending[0].reject(new Error('stale'));await old;assert.equal(state.data.checkpoints[0].id,'new');assert.equal(state.error,'');
 const late=loader.load();loader.cancel();pending[2].resolve({checkpoints:[]});await late;
 assert.equal(state.data.checkpoints[0].id,'new');assert.equal(pending[2].signal.aborted,true);
});
test('failures preserve previously saved checkpoints and report the reason',async()=>{
 let fail=false,state;
 const loader=createCheckpointLoader(async()=>{if(fail)throw Error('source_changed');return {checkpoints:[{id:'saved'}]};},(data,error,busy)=>{state={data,error,busy};});
 await loader.load();fail=true;await loader.load();assert.equal(state.error,'source_changed');assert.equal(state.data.checkpoints[0].id,'saved');assert.equal(state.busy,false);
});
