import test from 'node:test';
import assert from 'node:assert/strict';
import * as T from 'three';
import { explicitCamera, applyExplicitCamera, renderCameraMeta } from '../render/camera.ts';

test('explicit cameras preserve numerical precision and reject ambiguous or invalid views', () => {
  const params={pos:'1.123456789012,2,3',tgt:'0,0.000000001,0',fov:34,near:.02,far:900};
  const camera=explicitCamera(params);
  assert.deepEqual(camera,{pos:[1.123456789012,2,3],tgt:[0,.000000001,0],up:[0,1,0],fov:34,near:.02,far:900});
  assert.equal(explicitCamera({view:'deck'}),null);
  for (const patch of [{pos:'NaN,2,3'},{pos:'Infinity,2,3'},{pos:'1,,3'},{pos:'0,0.000000001,0'},{tgt:''},{fov:NaN},{fov:180},{near:0},{far:.01},{focus:'hull'},{dist:12},{az:20}]) {
    assert.throws(()=>explicitCamera({...params,...patch}),{code:'bad_param'});
  }
});
test('applying exact camera never fits and metadata preserves its full projection', () => {
  const camera=new T.PerspectiveCamera(50,640/360,.05,1600);
  const target=new T.Vector3();
  let sets=0;
  const v={camera,controls:{target},BOX:new T.Box3(new T.Vector3(-1,-2,-3),new T.Vector3(2,3,4)),baseFov:34,
    setView(pos,tgt){sets++;this.camera.position.fromArray(pos);this.controls.target.fromArray(tgt);},
    preset(){assert.fail('unexpected auto fit');},focus(){assert.fail('unexpected focus');}};
  const exact={pos:[1.123456789012,2,3],tgt:[0,0,0],up:[0,1,0],fov:34,near:.02,far:900};
  applyExplicitCamera(v,exact);const meta=renderCameraMeta(v);
  assert.equal(sets,1);assert.deepEqual(meta.camera,{...exact,aspect:640/360});
  assert.deepEqual(meta.bounds,{min:[-1,-2,-3],max:[2,3,4]});
  assert.equal(camera.fov,34);assert.equal(v.baseFov,34);
  const projection=camera.projectionMatrix.toArray();applyExplicitCamera(v,exact);
  assert.deepEqual(camera.projectionMatrix.toArray(),projection);
});
