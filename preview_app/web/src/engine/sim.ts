/* 模拟：船的运动（按 GameParams 航速/舵效）、尾迹与白沫（近似示意）、炮塔转动与开火（按 GameParams 转速/射界/俯仰）。
 * 全部是确定性的：给定相同的命令序列与固定 dt，结果完全相同（无头出图依赖这一点）。 */
import * as THREE from 'three';
import type { MountSpec, SimData } from '../lib/types';

export const TELEGRAPH = ['rev', 'stop', 'q1', 'q2', 'q3', 'full'] as const;
export type Telegraph = typeof TELEGRAPH[number];
export const TEL_FRAC: Record<Telegraph, number> = { rev: -1, stop: 0, q1: 0.25, q2: 0.5, q3: 0.75, full: 1 };
export const RUDDER = [-1, -0.5, 0, 0.5, 1] as const;

/** 游戏里的航行观感比真实尺度快：节数 → 模型单位/秒 时再乘这个系数（近似，见 API.md §11） */
export const TIME_K = 2.6;
export const REVERSE_FRAC = 0.25;   // 倒车最高速度 ≈ 前进的 1/4（近似；GameParams 没有直接给出）

export function mulberry32(seed: number) {
  let a = seed >>> 0;
  return () => { a = (a + 0x6d2b79f5) >>> 0; let t = a; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
}

// ---------------------------------------------------------------- 航行
export class ShipMotion {
  ship: SimData['ship'];
  m2u = 0.067;                 // 1 米 = 多少模型单位（模型长度 / GameParams size[0]）
  tel: Telegraph = 'stop'; rudderCmd = 0;
  speedKn = 0; rudder = 0;     // rudder: -1..1（实际舵角比例）
  P = new THREE.Vector2(); psi = 0; t = 0;
  roll = 0; pitch = 0; heave = 0; heel = 0; accelPitch = 0;
  waves = 1;                   // 涌浪强度（0 = 静水）
  fixedKn: number | null = null; // 无头出图：锁定航速（节），忽略车钟
  constructor(ship: SimData['ship']) { this.ship = ship; }
  get maxKn() { return (this.ship.maxSpeed_kn || 30) * (this.ship.speedCoef || 1); }
  targetKn() { if (this.fixedKn !== null) return this.fixedKn; const f = TEL_FRAC[this.tel]; return f >= 0 ? f * this.maxKn : -REVERSE_FRAC * this.maxKn; }
  /** 模型单位 / 秒 */
  speedU() { return this.speedKn * 0.5144 * this.m2u * TIME_K; }
  step(dt: number) {
    this.t += dt;
    const tgt = this.targetKn(), maxKn = this.maxKn;
    const up = Math.max(5, (this.speedKn < 0 || tgt < 0 ? this.ship.backwardEngineUpTime_s : this.ship.forwardEngineUpTime_s) || 40);
    const rate = maxKn / up;                        // 节/秒（线性加速，近似）
    const prev = this.speedKn;
    const d = tgt - this.speedKn; this.speedKn += Math.sign(d) * Math.min(Math.abs(d), rate * dt);
    // 舵：从中位转到满舵用 rudderTime
    const rt = Math.max(1, this.ship.rudderTime_s || 12);
    const dr = this.rudderCmd - this.rudder; this.rudder += Math.sign(dr) * Math.min(Math.abs(dr), dt / rt);
    // 转向：角速度 = 线速度 / 回转半径 × 舵量（倒车时反向）
    const vM = this.speedKn * 0.5144;
    const R = Math.max(50, this.ship.turningRadius_m || 700);
    const omega = (vM / R) * this.rudder * TIME_K;
    this.psi -= omega * dt;
    // 前进方向（世界）：本地 −Z 旋转 ψ → (−sinψ, −cosψ)
    const v = this.speedU();
    this.P.x += -Math.sin(this.psi) * v * dt; this.P.y += -Math.cos(this.psi) * v * dt;
    // 外倾：向转弯外侧倾斜，随速度平方增长（近似）；船首随加速轻微上抬
    const sp = Math.min(1, Math.abs(this.speedKn) / maxKn);
    const heelT = 3.2 * this.rudder * sp * sp * Math.sign(this.speedKn || 1);
    this.heel += (heelT - this.heel) * Math.min(1, dt / 2.2);
    const acc = (this.speedKn - prev) / Math.max(dt, 1e-3) / Math.max(rate, 1e-3);
    this.accelPitch += (0.35 * acc - this.accelPitch) * Math.min(1, dt / 1.5);
    const t = this.t, w = this.waves;
    this.roll = this.heel + w * (0.45 * Math.sin(t * 0.83) + 0.18 * Math.sin(t * 1.71 + 1.3));
    this.pitch = this.accelPitch + w * (0.22 * Math.sin(t * 0.61 + 0.7) + 0.08 * Math.sin(t * 1.37)) + 0.25 * sp;
    this.heave = w * (0.010 * Math.sin(t * 0.74 + 0.2) + 0.004 * Math.sin(t * 1.9));
  }
  /** 船体坐标(x,z) → 世界 */
  toWorld(x: number, z: number, out = new THREE.Vector2()) { const c = Math.cos(this.psi), s = Math.sin(this.psi); return out.set(c * x + s * z + this.P.x, -s * x + c * z + this.P.y); }
  toLocal(wx: number, wz: number, out = new THREE.Vector2()) { const c = Math.cos(this.psi), s = Math.sin(this.psi); const x = wx - this.P.x, z = wz - this.P.y; return out.set(c * x - s * z, s * x + c * z); }
}

// ---------------------------------------------------------------- 尾迹与白沫（近似示意）
interface TrailPt { x: number; z: number; dx: number; dz: number; age: number; a: number }
/** 一条随时间扩散的带子：点在世界坐标里固定（或带侧向漂移），每帧转到船体坐标重建网格 */
export class Trail {
  pts: TrailPt[] = []; max: number; life: number; w0: number; spread: number;
  geo = new THREE.BufferGeometry(); mesh: THREE.Mesh;
  pos: Float32Array; uv: Float32Array; al: Float32Array;
  constructor(mat: THREE.Material, max: number, life: number, w0: number, spread: number) {
    this.max = max; this.life = life; this.w0 = w0; this.spread = spread;
    this.pos = new Float32Array(max * 2 * 3); this.uv = new Float32Array(max * 2 * 2); this.al = new Float32Array(max * 2);
    this.geo.setAttribute('position', new THREE.BufferAttribute(this.pos, 3).setUsage(THREE.DynamicDrawUsage));
    this.geo.setAttribute('aUv', new THREE.BufferAttribute(this.uv, 2).setUsage(THREE.DynamicDrawUsage));
    this.geo.setAttribute('aA', new THREE.BufferAttribute(this.al, 1).setUsage(THREE.DynamicDrawUsage));
    const idx: number[] = []; for (let i = 0; i < max - 1; i++) { const a = i * 2; idx.push(a, a + 1, a + 2, a + 1, a + 3, a + 2); }
    this.geo.setIndex(idx); this.geo.setDrawRange(0, 0);
    this.mesh = new THREE.Mesh(this.geo, mat); this.mesh.frustumCulled = false; this.mesh.renderOrder = 4;
  }
  push(x: number, z: number, dx: number, dz: number, a: number) {
    this.pts.unshift({ x, z, dx, dz, age: 0, a });
    if (this.pts.length > this.max) this.pts.length = this.max;
  }
  step(dt: number) { for (const p of this.pts) { p.age += dt; p.x += p.dx * dt; p.z += p.dz * dt; } while (this.pts.length && this.pts[this.pts.length - 1].age > this.life) this.pts.pop(); }
  clear() { this.pts.length = 0; this.geo.setDrawRange(0, 0); }
  /** head: 船上发射点（船体坐标），保证带子和船体相接 */
  build(m: ShipMotion, head: THREE.Vector2 | null, headA: number, y: number) {
    const L: { x: number; z: number; age: number; a: number }[] = [];
    if (head) L.push({ x: head.x, z: head.y, age: 0, a: headA });
    const v = new THREE.Vector2();
    for (const p of this.pts) { m.toLocal(p.x, p.z, v); L.push({ x: v.x, z: v.y, age: p.age, a: p.a }); }
    const n = Math.min(L.length, this.max);
    if (n < 2) { this.geo.setDrawRange(0, 0); return; }
    let acc = 0;
    for (let i = 0; i < n; i++) {
      const a = L[Math.max(0, i - 1)], b = L[Math.min(n - 1, i + 1)];
      let tx = b.x - a.x, tz = b.z - a.z; const l = Math.hypot(tx, tz) || 1; tx /= l; tz /= l;
      const p = L[i]; const w = this.w0 + this.spread * p.age;
      const fade = Math.max(0, 1 - p.age / this.life); const al = p.a * fade * fade;
      if (i > 0) acc += Math.hypot(L[i].x - L[i - 1].x, L[i].z - L[i - 1].z);
      const k = i * 6;
      this.pos[k] = p.x - tz * w; this.pos[k + 1] = y; this.pos[k + 2] = p.z + tx * w;
      this.pos[k + 3] = p.x + tz * w; this.pos[k + 4] = y; this.pos[k + 5] = p.z - tx * w;
      this.uv[i * 4] = acc; this.uv[i * 4 + 1] = -1; this.uv[i * 4 + 2] = acc; this.uv[i * 4 + 3] = 1;
      this.al[i * 2] = this.al[i * 2 + 1] = i === 0 ? al * 0.6 : al;
    }
    (this.geo.attributes.position as THREE.BufferAttribute).needsUpdate = true;
    (this.geo.attributes.aUv as THREE.BufferAttribute).needsUpdate = true;
    (this.geo.attributes.aA as THREE.BufferAttribute).needsUpdate = true;
    this.geo.setDrawRange(0, (n - 1) * 6);
  }
}

// ---------------------------------------------------------------- 粒子（炮口焰 / 硝烟，近似示意）
export class Particles {
  N: number; n = 0;
  wp: Float32Array; vel: Float32Array; age: Float32Array; life: Float32Array; s0: Float32Array; s1: Float32Array; kind: Float32Array; col: Float32Array; a0: Float32Array; seed: Float32Array;
  geo = new THREE.BufferGeometry(); points: THREE.Points; mat: THREE.ShaderMaterial; matAdd: THREE.ShaderMaterial; pointsAdd: THREE.Points; geoAdd = new THREE.BufferGeometry();
  constructor(N: number, vs: string, fs: string) {
    this.N = N;
    const f = (k = 1) => new Float32Array(N * k);
    this.wp = f(3); this.vel = f(3); this.age = f(); this.life = f(); this.s0 = f(); this.s1 = f(); this.kind = f(); this.col = f(3); this.a0 = f(); this.seed = f();
    const mk = (g: THREE.BufferGeometry) => {
      g.setAttribute('position', new THREE.BufferAttribute(new Float32Array(N * 3), 3).setUsage(THREE.DynamicDrawUsage));
      g.setAttribute('aSize', new THREE.BufferAttribute(new Float32Array(N), 1).setUsage(THREE.DynamicDrawUsage));
      g.setAttribute('aAlpha', new THREE.BufferAttribute(new Float32Array(N), 1).setUsage(THREE.DynamicDrawUsage));
      g.setAttribute('aCol', new THREE.BufferAttribute(new Float32Array(N * 3), 3).setUsage(THREE.DynamicDrawUsage));
      g.setAttribute('aKind', new THREE.BufferAttribute(new Float32Array(N), 1).setUsage(THREE.DynamicDrawUsage));
      g.setAttribute('aSeed', new THREE.BufferAttribute(new Float32Array(N), 1).setUsage(THREE.DynamicDrawUsage));
      g.setDrawRange(0, 0);
    };
    mk(this.geo); mk(this.geoAdd);
    const uScale = { value: 600 };
    this.mat = new THREE.ShaderMaterial({ vertexShader: vs, fragmentShader: fs, transparent: true, depthWrite: false, uniforms: { uScale } });
    this.matAdd = new THREE.ShaderMaterial({ vertexShader: vs, fragmentShader: fs, transparent: true, depthWrite: false, blending: THREE.AdditiveBlending, uniforms: { uScale } });
    this.points = new THREE.Points(this.geo, this.mat); this.points.frustumCulled = false; this.points.renderOrder = 6;
    this.pointsAdd = new THREE.Points(this.geoAdd, this.matAdd); this.pointsAdd.frustumCulled = false; this.pointsAdd.renderOrder = 7;
  }
  setScale(px: number) { this.mat.uniforms.uScale.value = px; }
  spawn(o: { p: THREE.Vector3; v: THREE.Vector3; life: number; s0: number; s1: number; kind: number; col: THREE.Color; a: number; seed: number }) {
    let i = this.n < this.N ? this.n++ : -1;
    if (i < 0) { let best = 0, ba = -1; for (let k = 0; k < this.N; k++) { const r = this.age[k] / this.life[k]; if (r > ba) { ba = r; best = k; } } i = best; }
    this.wp.set([o.p.x, o.p.y, o.p.z], i * 3); this.vel.set([o.v.x, o.v.y, o.v.z], i * 3);
    this.age[i] = 0; this.life[i] = o.life; this.s0[i] = o.s0; this.s1[i] = o.s1; this.kind[i] = o.kind; this.col.set([o.col.r, o.col.g, o.col.b], i * 3); this.a0[i] = o.a; this.seed[i] = o.seed;
  }
  step(dt: number) {
    for (let i = 0; i < this.n; i++) {
      this.age[i] += dt;
      if (this.age[i] >= this.life[i]) {
        const j = --this.n;   // 和最后一个交换
        if (i !== j) {
          for (const [arr, k] of [[this.wp, 3], [this.vel, 3], [this.col, 3]] as [Float32Array, number][]) arr.copyWithin(i * k, j * k, j * k + k);
          for (const arr of [this.age, this.life, this.s0, this.s1, this.kind, this.a0, this.seed]) arr[i] = arr[j];
          i--;
        }
        continue;
      }
      const k = i * 3, drag = this.kind[i] > 0.5 ? 6 : 1.4;
      const f = Math.exp(-drag * dt);
      this.vel[k] *= f; this.vel[k + 2] *= f; this.vel[k + 1] = this.vel[k + 1] * f + (this.kind[i] > 0.5 ? 0 : 0.012 * dt);
      this.wp[k] += this.vel[k] * dt; this.wp[k + 1] += this.vel[k + 1] * dt; this.wp[k + 2] += this.vel[k + 2] * dt;
    }
  }
  clear() { this.n = 0; this.geo.setDrawRange(0, 0); this.geoAdd.setDrawRange(0, 0); }
  build(m: ShipMotion) {
    const ga = this.geo.attributes, gb = this.geoAdd.attributes; let na = 0, nb = 0; const v = new THREE.Vector2();
    for (let i = 0; i < this.n; i++) {
      const add = this.kind[i] > 0.5; const at = add ? gb : ga; const j = add ? nb++ : na++;
      const k = i * 3; m.toLocal(this.wp[k], this.wp[k + 2], v);
      (at.position.array as Float32Array).set([v.x, this.wp[k + 1], v.y], j * 3);
      const r = this.age[i] / this.life[i];
      (at.aSize.array as Float32Array)[j] = this.s0[i] + (this.s1[i] - this.s0[i]) * (1 - Math.pow(1 - r, 2.5));
      (at.aAlpha.array as Float32Array)[j] = add ? this.a0[i] * Math.pow(1 - r, 1.6) : this.a0[i] * Math.min(1, r * 12) * Math.pow(1 - r, 1.8);
      (at.aCol.array as Float32Array).set([this.col[k], this.col[k + 1], this.col[k + 2]], j * 3);
      (at.aKind.array as Float32Array)[j] = this.kind[i]; (at.aSeed.array as Float32Array)[j] = this.seed[i];
    }
    for (const at of [ga, gb]) for (const key of ['position', 'aSize', 'aAlpha', 'aCol', 'aKind', 'aSeed']) (at[key] as THREE.BufferAttribute).needsUpdate = true;
    this.geo.setDrawRange(0, na); this.geoAdd.setDrawRange(0, nb);
  }
}

// ---------------------------------------------------------------- 炮塔
export interface Mount {
  id: string; spec: MountSpec; top: THREE.Object3D; yawN?: THREE.Object3D; pitchN?: THREE.Object3D; recoilN?: THREE.Object3D; muzzles: THREE.Object3D[];
  yawQ0?: THREE.Quaternion; pitchQ0?: THREE.Quaternion; recoil0?: THREE.Vector3;
  yaw: number; pitch: number; wantYaw: number; wantPitch: number; inSector: boolean; aimed: boolean;
  reload: number; shotT: number; order: number;
}
const D2R = Math.PI / 180;
export function clampSector(az: number, sec?: [number, number] | null, dead?: [number, number][]) {
  let a = az, ok = true;
  if (sec && sec.length === 2) { const [lo, hi] = sec; if (a < lo) { a = lo; ok = false; } if (a > hi) { a = hi; ok = false; } }
  for (const d of dead || []) if (a > d[0] && a < d[1]) { a = a - d[0] < d[1] - a ? d[0] : d[1]; ok = false; }
  return { a, ok };
}
export function makeMount(id: string, spec: MountSpec, top: THREE.Object3D, order: number): Mount {
  const find = (role: string) => top.children.length ? findBone(top, role) : undefined;
  const yawN = find('yaw'), pitchN = find('pitch'), recoilN = find('recoil');
  const muzzles: THREE.Object3D[] = [];
  top.traverse((o) => { if (o.userData && o.userData.muzzle !== undefined) muzzles.push(o); });
  muzzles.sort((a, b) => a.userData.muzzle - b.userData.muzzle);
  return { id, spec, top, yawN, pitchN, recoilN, muzzles, yawQ0: yawN?.quaternion.clone(), pitchQ0: pitchN?.quaternion.clone(), recoil0: recoilN?.position.clone(),
    yaw: 0, pitch: 0, wantYaw: 0, wantPitch: 0, inSector: true, aimed: false, reload: 0, shotT: 99, order };
}
function findBone(o: THREE.Object3D, role: string): THREE.Object3D | undefined {
  let r: THREE.Object3D | undefined; o.traverse((c) => { if (!r && c.userData && c.userData.bone === role) r = c; }); return r;
}
const _q = new THREE.Quaternion(), _ax = new THREE.Vector3();
export function poseMount(m: Mount, m2u: number) {
  if (m.yawN && m.yawQ0) m.yawN.quaternion.copy(m.yawQ0).multiply(_q.setFromAxisAngle(_ax.set(0, 1, 0), -m.yaw * D2R));
  if (m.pitchN && m.pitchQ0) m.pitchN.quaternion.copy(m.pitchQ0).multiply(_q.setFromAxisAngle(_ax.set(1, 0, 0), m.pitch * D2R));
  if (m.recoilN && m.recoil0) {
    const dist = (m.spec.barrelDiameter || 0.1) * 3.0 * m2u;                // 后坐行程 ≈ 3 倍口径（近似）
    const t = m.shotT, back = 0.05, ret = (m.spec.barrelDiameter || 0.1) > 0.2 ? 1.4 : 0.5;
    const k = t < back ? t / back : t < back + ret ? 1 - easeInOut((t - back) / ret) : 0;
    m.recoilN.position.copy(m.recoil0); m.recoilN.position.z += dist * k;
  }
}
const easeInOut = (x: number) => (x < 0.5 ? 2 * x * x : 1 - Math.pow(-2 * x + 2, 2) / 2);
/** 由目标距离估算抬炮角（近似：GameParams 不含弹道表；按最大射程线性-幂次插值） */
export function elevFor(spec: MountSpec, distM: number) {
  const vs = spec.vertSector || [0, 30]; const md = spec.maxDist || 15000;
  const f = Math.min(1, Math.max(0, distM / md));
  return Math.min(vs[1], Math.max(Math.max(vs[0], 0), 0.4 + 0.62 * vs[1] * Math.pow(f, 1.35)));
}
/** 每帧：转向目标（按 rotationSpeed 限速） */
export function stepMount(m: Mount, dt: number, targetLocal: THREE.Vector3 | null, distM: number, combat: boolean, instant = false) {
  const s = m.spec; const rs = s.rotationSpeed || [20, 10];
  if (combat && targetLocal) {
    const az = Math.atan2(targetLocal.x, -targetLocal.z) / D2R;
    const c = clampSector(az, s.horizSector, s.deadZone); m.wantYaw = c.a; m.inSector = c.ok;
    m.wantPitch = elevFor(s, distM);
  } else { m.wantYaw = 0; m.wantPitch = 0; m.inSector = true; }
  // smallGun 的 rotationSpeed[1] 不是俯仰速度（推测），俯仰沿用水平转速
  const ys = rs[0], ps = s.smallGun ? rs[0] : rs[1];
  if (instant) { m.yaw = m.wantYaw; m.pitch = m.wantPitch; }
  else {
    const dy = m.wantYaw - m.yaw; m.yaw += Math.sign(dy) * Math.min(Math.abs(dy), ys * dt);
    const dp = m.wantPitch - m.pitch; m.pitch += Math.sign(dp) * Math.min(Math.abs(dp), ps * dt);
  }
  m.aimed = combat && m.inSector && Math.abs(m.wantYaw - m.yaw) < 1.0 && Math.abs(m.wantPitch - m.pitch) < 1.0;
  m.reload = Math.max(0, m.reload - dt); m.shotT += dt;
}
