/* 预览 GLB 解析（无动态 import）：静态/蒙皮网格、节点 matrix 或 TRS，保留 extras 与材质名。
 * 节点层级原样保留（炮塔骨骼 yaw → pitch → recoil → 炮口），mesh 共享 BufferGeometry，skin 按炮位独立。
 * mesh.userData = { mfm, renderSet, identifier, part, lod }；part / lod 从最近的祖先节点 extras 继承。 */
import * as THREE from 'three';

const CT: Record<number, any> = { 5120: Int8Array, 5121: Uint8Array, 5122: Int16Array, 5123: Uint16Array, 5125: Uint32Array, 5126: Float32Array };
const NC: Record<string, number> = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 };
const badGLB = (reason: string) => Object.assign(new Error(reason), { code: 'bad_glb' });

const jointBox = new THREE.Box3(), jointMatrix = new THREE.Matrix4();
/** The union encloses every normalized blend of the joint-transformed rest vertices. */
export function skinBounds(mesh: THREE.SkinnedMesh, into: THREE.Box3) {
  const { geometry, skeleton, bindMatrix, bindMatrixInverse } = mesh;
  const rest = geometry.boundingBox!;
  into.makeEmpty();
  for (let i = 0; i < skeleton.bones.length; i++) {
    jointMatrix.copy(bindMatrixInverse).multiply(skeleton.bones[i].matrixWorld).multiply(skeleton.boneInverses[i]).multiply(bindMatrix);
    into.union(jointBox.copy(rest).applyMatrix4(jointMatrix));
  }
  return into;
}

const pickBox = new THREE.Box3(), pickSphere = new THREE.Sphere();
class PreviewSkinnedMesh extends THREE.SkinnedMesh {
  override raycast(raycaster: THREE.Raycaster, intersects: THREE.Intersection[]) {
    // Three r147 skins triangle vertices in Mesh.raycast, but first tests the
    // shared geometry's rest bounds. Supply posed bounds for this synchronous
    // pick only, leaving other instances and camera fitting untouched.
    const g = this.geometry, box = g.boundingBox, sphere = g.boundingSphere;
    skinBounds(this, pickBox).getBoundingSphere(pickSphere);
    g.boundingBox = pickBox; g.boundingSphere = pickSphere;
    try { super.raycast(raycaster, intersects); }
    finally { g.boundingBox = box; g.boundingSphere = sphere; }
  }
}

export function disposeModelGeometry(root: THREE.Object3D) {
  const geometries = new Set<THREE.BufferGeometry>(), skeletons = new Set<THREE.Skeleton>();
  root.traverse((node) => {
    const mesh = node as THREE.SkinnedMesh;
    if (mesh.geometry) geometries.add(mesh.geometry);
    if (mesh.isSkinnedMesh) skeletons.add(mesh.skeleton);
  });
  geometries.forEach((g) => g.dispose());
  skeletons.forEach((s) => s.dispose());
}

export function parseGLB(buf: ArrayBuffer): { root: THREE.Group; json: any } {
  const dv = new DataView(buf);
  if (buf.byteLength < 20 || dv.getUint32(0, true) !== 0x46546c67) throw badGLB('not a GLB');
  let off = 12, json: any = null, bin: ArrayBuffer | null = null;
  while (off + 8 <= buf.byteLength) {
    const len = dv.getUint32(off, true), type = dv.getUint32(off + 4, true);
    if (type === 0x4e4f534a) json = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, off + 8, len)));
    else if (type === 0x004e4942) bin = buf.slice(off + 8, off + 8 + len);
    off += 8 + len;
  }
  if (!json || !bin) throw badGLB('GLB missing JSON/BIN chunk');
  const B = bin;
  const acc = (ai: number) => {
    const a = json.accessors[ai], v = json.bufferViews[a.bufferView];
    const T = CT[a.componentType], n = a.count * NC[a.type], o = (v.byteOffset || 0) + (a.byteOffset || 0);
    if (v.byteStride && v.byteStride !== T.BYTES_PER_ELEMENT * NC[a.type]) throw badGLB('interleaved buffers not supported');
    if (o % T.BYTES_PER_ELEMENT) return new T(B.slice(o, o + n * T.BYTES_PER_ELEMENT));
    return new T(B, o, n);
  };
  const matName = (i?: number) => (i !== undefined && json.materials && json.materials[i] ? json.materials[i].name : null);
  const geoms: THREE.BufferGeometry[][] = (json.meshes || []).map((m: any) => m.primitives.map((p: any) => {
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(acc(p.attributes.POSITION), 3));
    if (p.attributes.NORMAL !== undefined) g.setAttribute('normal', new THREE.BufferAttribute(acc(p.attributes.NORMAL), 3));
    if (p.attributes.TEXCOORD_0 !== undefined) g.setAttribute('uv', new THREE.BufferAttribute(acc(p.attributes.TEXCOORD_0), 2));
    if (p.attributes.JOINTS_0 !== undefined) g.setAttribute('skinIndex', new THREE.BufferAttribute(acc(p.attributes.JOINTS_0), 4));
    if (p.attributes.WEIGHTS_0 !== undefined) g.setAttribute('skinWeight', new THREE.BufferAttribute(acc(p.attributes.WEIGHTS_0), 4, !!json.accessors[p.attributes.WEIGHTS_0].normalized));
    if (p.indices !== undefined) g.setIndex(new THREE.BufferAttribute(acc(p.indices), 1));
    if (!g.attributes.normal) g.computeVertexNormals();
    g.computeBoundingBox(); g.computeBoundingSphere();
    g.userData = Object.assign({ mfm: matName(p.material) }, p.extras || {});
    return g;
  }));
  const root = new THREE.Group();
  const skins: any[] = json.skins || [];
  const jointIds = new Set<number>(skins.flatMap((s) => s.joints));
  const nodes: THREE.Object3D[] = json.nodes.map((n: any, ni: number) => {
    const grp = jointIds.has(ni) ? new THREE.Bone() : new THREE.Group();
    grp.name = n.name || 'node' + ni; grp.userData = n.extras || {};
    if (n.matrix) grp.applyMatrix4(new THREE.Matrix4().fromArray(n.matrix));
    else {
      if (n.translation) grp.position.fromArray(n.translation);
      if (n.rotation) grp.quaternion.fromArray(n.rotation);
      if (n.scale) grp.scale.fromArray(n.scale);
    }
    return grp;
  });
  const skeletons = new Map<number, THREE.Skeleton>();
  const makeSkeleton = (id: number) => {
    const cached = skeletons.get(id); if (cached) return cached;
    const s = skins[id];
    if (!s?.joints?.length || s.joints.some((j: number) => !nodes[j])) throw badGLB('invalid skin joints');
    const bones: THREE.Bone[] = s.joints.map((j: number) => nodes[j] as THREE.Bone);
    const data = s.inverseBindMatrices === undefined ? null : acc(s.inverseBindMatrices);
    if (data && data.length !== bones.length * 16) throw badGLB('invalid inverse bind matrices');
    const inverses = bones.map((_, i) => data ? new THREE.Matrix4().fromArray(data, i * 16) : new THREE.Matrix4());
    const skeleton = new THREE.Skeleton(bones, inverses); skeletons.set(id, skeleton); return skeleton;
  };
  const identity = new THREE.Matrix4();
  const walk = (ni: number, parent: THREE.Object3D, inh: { part?: string; lod?: number; bone?: string }) => {
    const n = json.nodes[ni], grp = nodes[ni];
    parent.add(grp);
    const ex = n.extras || {};
    const here = { part: ex.part ?? inh.part ?? (inh.part === undefined && n.mesh !== undefined ? grp.name : undefined), lod: ex.lod ?? inh.lod, bone: ex.bone ?? inh.bone };
    if (n.mesh !== undefined) geoms[n.mesh].forEach((g, i) => {
      let m: THREE.Mesh;
      if (n.skin === undefined) m = new THREE.Mesh(g);
      else {
        const { position, skinIndex, skinWeight } = g.attributes;
        if (skinIndex?.count !== position.count || skinWeight?.count !== position.count) throw badGLB('skinned primitive missing vertex influences');
        const skinned = new PreviewSkinnedMesh(g);
        // Identity is deliberate. The exported BlendBone nodes already hold
        // their inverse-bind corrections. In attached mode updateMatrixWorld
        // cancels the mesh's world matrix so the mount is applied only once.
        skinned.bind(makeSkeleton(n.skin), identity);
        skinned.frustumCulled = false;
        m = skinned;
      }
      m.name = grp.name + '#' + i;
      m.userData = { mfm: g.userData.mfm, renderSet: g.userData.renderSet, identifier: g.userData.identifier, part: here.part, lod: here.lod ?? 0, bone: here.bone };
      grp.add(m);
    });
    (n.children || []).forEach((c: number) => walk(c, grp, here));
  };
  const children = json.scenes ? null : new Set(json.nodes.flatMap((n: any) => n.children || []));
  const sc = json.scenes ? json.scenes[json.scene || 0] : { nodes: nodes.map((_, i) => i).filter((i) => !children!.has(i)) };
  (sc.nodes || []).forEach((ni: number) => walk(ni, root, {}));
  root.updateMatrixWorld(true);
  return { root, json };
}
