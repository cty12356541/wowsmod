/* 极简 GLB 解析（无动态 import）：POSITION / NORMAL / TEXCOORD_0 / indices，节点 matrix 或 TRS，保留 extras 与材质名。
 * 节点层级原样保留（炮塔骨骼 yaw → pitch → recoil → 炮口），mesh 共享 BufferGeometry。
 * mesh.userData = { mfm, renderSet, identifier, part, lod }；part / lod 从最近的祖先节点 extras 继承。 */
import * as THREE from 'three';

const CT: Record<number, any> = { 5120: Int8Array, 5121: Uint8Array, 5122: Int16Array, 5123: Uint16Array, 5125: Uint32Array, 5126: Float32Array };
const NC: Record<string, number> = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 };

export function parseGLB(buf: ArrayBuffer): { root: THREE.Group; json: any } {
  const dv = new DataView(buf);
  if (buf.byteLength < 20 || dv.getUint32(0, true) !== 0x46546c67) throw Object.assign(new Error('not a GLB'), { code: 'bad_glb' });
  let off = 12, json: any = null, bin: ArrayBuffer | null = null;
  while (off + 8 <= buf.byteLength) {
    const len = dv.getUint32(off, true), type = dv.getUint32(off + 4, true);
    if (type === 0x4e4f534a) json = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, off + 8, len)));
    else if (type === 0x004e4942) bin = buf.slice(off + 8, off + 8 + len);
    off += 8 + len;
  }
  if (!json || !bin) throw Object.assign(new Error('GLB missing JSON/BIN chunk'), { code: 'bad_glb' });
  const B = bin;
  const acc = (ai: number) => {
    const a = json.accessors[ai], v = json.bufferViews[a.bufferView];
    const T = CT[a.componentType], n = a.count * NC[a.type], o = (v.byteOffset || 0) + (a.byteOffset || 0);
    if (v.byteStride && v.byteStride !== T.BYTES_PER_ELEMENT * NC[a.type]) throw Object.assign(new Error('interleaved buffers not supported'), { code: 'bad_glb' });
    if (o % T.BYTES_PER_ELEMENT) return new T(B.slice(o, o + n * T.BYTES_PER_ELEMENT));
    return new T(B, o, n);
  };
  const matName = (i?: number) => (i !== undefined && json.materials && json.materials[i] ? json.materials[i].name : null);
  const geoms: THREE.BufferGeometry[][] = (json.meshes || []).map((m: any) => m.primitives.map((p: any) => {
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(acc(p.attributes.POSITION), 3));
    if (p.attributes.NORMAL !== undefined) g.setAttribute('normal', new THREE.BufferAttribute(acc(p.attributes.NORMAL), 3));
    if (p.attributes.TEXCOORD_0 !== undefined) g.setAttribute('uv', new THREE.BufferAttribute(acc(p.attributes.TEXCOORD_0), 2));
    if (p.indices !== undefined) g.setIndex(new THREE.BufferAttribute(acc(p.indices), 1));
    if (!g.attributes.normal) g.computeVertexNormals();
    g.computeBoundingBox(); g.computeBoundingSphere();
    g.userData = Object.assign({ mfm: matName(p.material) }, p.extras || {});
    return g;
  }));
  const root = new THREE.Group();
  const walk = (ni: number, parent: THREE.Object3D, inh: { part?: string; lod?: number; bone?: string }) => {
    const n = json.nodes[ni], grp = new THREE.Group();
    grp.name = n.name || 'node' + ni; grp.userData = n.extras || {};
    if (n.matrix) grp.applyMatrix4(new THREE.Matrix4().fromArray(n.matrix));
    else {
      if (n.translation) grp.position.fromArray(n.translation);
      if (n.rotation) grp.quaternion.fromArray(n.rotation);
      if (n.scale) grp.scale.fromArray(n.scale);
    }
    const ex = n.extras || {};
    const here = { part: ex.part ?? inh.part ?? (inh.part === undefined && n.mesh !== undefined ? grp.name : undefined), lod: ex.lod ?? inh.lod, bone: ex.bone ?? inh.bone };
    if (n.mesh !== undefined) geoms[n.mesh].forEach((g, i) => {
      const m = new THREE.Mesh(g); m.name = grp.name + '#' + i;
      m.userData = { mfm: g.userData.mfm, renderSet: g.userData.renderSet, identifier: g.userData.identifier, part: here.part, lod: here.lod ?? 0, bone: here.bone };
      grp.add(m);
    });
    (n.children || []).forEach((c: number) => walk(c, grp, here));
    parent.add(grp);
  };
  const sc = json.scenes ? json.scenes[json.scene || 0] : { nodes: json.nodes.map((_: unknown, i: number) => i) };
  (sc.nodes || []).forEach((ni: number) => walk(ni, root, {}));
  return { root, json };
}
