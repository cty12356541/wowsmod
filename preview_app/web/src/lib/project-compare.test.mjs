import test from 'node:test';
import assert from 'node:assert/strict';
import { comparableProjects, validPair, createComparisonLoader } from './project-compare.ts';
const entry=(id,ship='PFSB510',available=true)=>({id,display_name:'same',available,project:{ship_key:ship},ship_metadata:{key:ship}});
const pair=(state='queued',left='a',right='b')=>({id:'pair',state,left:{project_id:left,url:null},right:{project_id:right,url:null}});
test('separate IDs under the same nonempty ship are required',()=>{
 const entries=[entry('a'),entry('b'),entry('c','OTHER'),entry('d','PFSB510',false),entry('global','')];
 assert.deepEqual(comparableProjects(entries,'a').map(x=>x.id),['b']);
 assert.equal(validPair(entries,'a','b'),true);
 for(const other of ['a','c','d','global','missing']) assert.equal(validPair(entries,'a',other),false);
 assert.equal(validPair([entry('g1',''),entry('g2','')],'g1','g2'),false);
});
test('a late old pair never replaces current pair and no independent covers imply ready',async()=>{
 const pending=[];let result;
 const loader=createComparisonLoader({create:(left,right,refresh,signal)=>new Promise(resolve=>pending.push({left,right,resolve,signal})),status:async()=>{throw Error('unexpected poll');}},(status,error)=>{result={status,error};},async()=>{});
 const old=loader.load('a','b');const current=loader.load('c','d');
 pending[1].resolve({...pair('failed','c','d'),reason:'right failed'});await current;
 pending[0].resolve(pair('ready'));await old;
 assert.equal(result.status.left.project_id,'c');assert.equal(result.status.state,'failed');
 assert.equal(pending[0].signal.aborted,true);
});
test('polling stops at 120 pending responses and exposes expiration',async()=>{
 let count=0,result;
 const loader=createComparisonLoader({create:async()=>pair(),status:async()=>{count++;return pair('rendering');}},(status,error,expired)=>{result={status,error,expired};},async()=>{});
 await loader.load('a','b');assert.equal(count,120);assert.equal(result.expired,true);
});
test('wrong identities and ready pairs missing one image are rejected',async()=>{
 for(const status of [pair('failed','wrong','b'),pair('ready')]) {
  let error='';const loader=createComparisonLoader({create:async()=>status,status:async()=>status},(_status,value)=>{error=value;},async()=>{});
  await loader.load('a','b');assert.ok(error);
 }
});
