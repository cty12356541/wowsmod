/* wowsmod 预览 · 3D 视口
 * 场景（线性 HDR，HalfFloat 离屏）→ 可选 Bloom → 合成（PBR Neutral 色调映射 + sRGB + 对比模式）→ 屏幕
 * 对比：A = 基准版本，B = 正在查看的版本；单视图 / 分屏（裁剪框一遍）/ 并排 / 叠加（两遍）。
 * 模拟：船体固定在原点，世界（水面/地面网格/尾迹/硝烟）按船的位置与航向反向运动；见 sim.ts。 */
import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';
import { parseGLB } from '../lib/glb';
import { getTex, FALLBACK, setAniso, texGC, texGpuBytes } from '../lib/tex';
import type { Manifest, MaterialDef, SimData } from '../lib/types';
import * as S from './shaders';
import { palette, gameLook, type EnvName, type LightName, type Palette, type LookName, type GameLook } from './env';
import { ShipMotion, Trail, Particles, makeMount, poseMount, stepMount, mulberry32, type Mount, type Telegraph } from './sim';

const T = THREE;
const lin = (hex: string) => new T.Color(hex).convertSRGBToLinear();

export type Quality = 'high' | 'mid' | 'low';

/** 特效主题(项目 fx.json 的 preview 段; wowsmod help workflow.layers.fx)。
 *  语义 = 游戏里能做到的「贴图保亮度换色」: 每类特效一个 tint(sRGB) + strength(0..1); keep = 高光保留(炮口焰白热核)。
 *  未给的字段 = 原版。alpha = 贴图 alpha 倍数(只允许 ≤1, 战斗可读性)。 */
export type FxTint = { tint?: string; strength?: number; keep?: number; alpha?: number };
export type FxTheme = { name?: string; muzzle?: FxTint; shotSmoke?: FxTint; funnel?: FxTint; wake?: FxTint };
/** 保亮度换色: base ⊙ mix(1, chroma(tint), k·(1-高光保留)) */
export function fxTint(base: THREE.Color, t: FxTint | undefined, hi = 0): THREE.Color {
  if (!t || !t.tint || !(t.strength! > 0)) return base.clone();
  const c = new T.Color(t.tint).convertSRGBToLinear(); const L = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b || 1;
  const k = Math.min(1, t.strength!) * (1 - (t.keep || 0) * hi);
  return new T.Color(base.r * (1 + (c.r / L - 1) * k), base.g * (1 + (c.g / L - 1) * k), base.b * (1 + (c.b / L - 1) * k));
}
const fxA = (t: FxTint | undefined) => Math.min(1, Math.max(0, t?.alpha ?? 1));
export const QUALITY: Record<Quality, { pr: number; msaa: number; shadow: number; bloom: boolean; tex: number; aniso: number; overlay: boolean; particles: number; simShadowEvery: number }> = {
  high: { pr: 2, msaa: 4, shadow: 2048, bloom: true, tex: 2048, aniso: 8, overlay: true, particles: 900, simShadowEvery: 1 },
  mid: { pr: 1, msaa: 2, shadow: 1024, bloom: true, tex: 1024, aniso: 4, overlay: true, particles: 500, simShadowEvery: 3 },
  low: { pr: 1, msaa: 0, shadow: 0, bloom: false, tex: 512, aniso: 1, overlay: false, particles: 220, simShadowEvery: 0 },
};
export type Mode = 'single' | 'split' | 'side' | 'overlay';
export type WaterMode = 'full' | 'cut' | 'off';
export type CamMode = 'orbit' | 'free' | 'game';
export const CHANNELS = ['final', 'albedo', 'normal', 'emissive', 'wire', 'mask'] as const;
export type Channel = typeof CHANNELS[number];
const LOD_COLORS = ['#3f9a6b', '#3b78d0', '#c88a1e', '#c0473f'];

interface PartRec { id: string; meshes: THREE.Mesh[]; visible: boolean; verts: number; tris: number; lods: { lod: number; extent: number }[]; lod: number; center: THREE.Vector3; top: THREE.Object3D }

export interface SimSnapshot {
  speedKn: number; tel: Telegraph; rudder: number; rudderCmd: number; heading: number; heel: number; roll: number; pitch: number;
  mounts: { id: string; group: string; yaw: number; pitch: number; wantYaw: number; aimed: boolean; inSector: boolean; reload: number; reloadMax: number }[];
  lod: Record<string, number>; camDist: number; aim: { az: number; distM: number } | null;
}

export interface ViewerOpts { quality?: Quality; preserve?: boolean; fixed?: [number, number]; deterministic?: boolean; onContextLost?: () => void }

export class Viewer {
  el: HTMLElement; renderer: THREE.WebGLRenderer; scene = new T.Scene(); camera: THREE.PerspectiveCamera; controls: OrbitControls;
  Q = QUALITY.high; qName: Quality = 'high'; opts: ViewerOpts;
  U: Record<string, { value: any }>; WU: Record<string, { value: any }>;
  skyMat!: THREE.ShaderMaterial; water!: THREE.Mesh; floor!: THREE.Mesh; blob!: THREE.Mesh; wlSheet!: THREE.Mesh; cutWall!: THREE.Mesh;
  sun!: THREE.DirectionalLight; hemi!: THREE.HemisphereLight; flashLight!: THREE.PointLight; envScene = new T.Scene(); pmrem: THREE.PMREMGenerator; envRT: THREE.WebGLRenderTarget | null = null;
  panels: THREE.Mesh[] = [];
  rtA!: THREE.WebGLRenderTarget; rtB!: THREE.WebGLRenderTarget; bloom: UnrealBloomPass; compScene = new T.Scene(); compCam = new T.OrthographicCamera(-1, 1, 1, -1, 0, 1); compMat: THREE.ShaderMaterial;
  shipFrame = new T.Group(); ship: THREE.Group | null = null;
  parts: Record<string, PartRec> = {}; meshes: THREE.Mesh[] = [];
  sets: { A: Record<string, THREE.Material>; B: Record<string, THREE.Material> } = { A: {}, B: {} };
  changedMfm = new Set<string>(); matCache = new Map<string, THREE.MeshStandardMaterial>(); neutral: THREE.MeshStandardMaterial | null = null;
  mode: Mode = 'single'; channel = 0; env: EnvName = 'studio'; light: LightName = 'day'; pal: Palette = palette('studio', 'day');
  /** 外观: game = 港口对齐(默认, 用于定稿与验收), studio = 影棚展台 */
  look: LookName = 'game'; lookOv: Partial<GameLook> = {}; envSkyMat!: THREE.ShaderMaterial;
  glowGain = 1; wlOn = true; waterMode: WaterMode = 'full'; hiOn = true; camoOn = true; split = 0.5;
  flashT = 0; flashSet = new Set<string>(); passIsA = false;
  BOX = new T.Box3(new T.Vector3(-1.2, -0.65, -8.9), new T.Vector3(1.2, 3.2, 8.9)); WL = 0; FLOOR_Y = -0.98; baseFov = 34; baseAspect = 1;
  camTween: { from: { pos: THREE.Vector3; tgt: THREE.Vector3 }; to: { pos: THREE.Vector3; tgt: THREE.Vector3 }; t: number } | null = null;
  clock = new T.Clock(); time = 0; dirty = 3; lastInput = 0; frameTimes: number[] = []; fps = 0; cpuMs = 0; idle = false; drawCalls = 0; tris = 0; paused = false; lost = false; bench = false;
  pickCb: ((id: string | null) => void) | null = null; frameCb: (() => void) | null = null; aimCb: ((p: THREE.Vector3 | null) => void) | null = null;
  // 模拟
  simData: SimData | null = null; motion: ShipMotion | null = null; simOn = false; combat = false; camMode: CamMode = 'orbit';
  mounts: Mount[] = []; aimLocal: THREE.Vector3 | null = null; aimWorld: THREE.Vector2 | null = null; aimFromCenter = false; aimMarker!: THREE.Mesh;
  wake!: Trail; bowL!: Trail; bowR!: Trail; sideFoam!: THREE.Mesh; foamMat!: THREE.ShaderMaterial; particles!: Particles; rng = mulberry32(7);
  lodMode: 'auto' | number = 0; lodBias = 1; lodTint = false; reloadReal = true; emitAcc = 0; freeCamWorld: { pos: THREE.Vector3; tgt: THREE.Vector3 } | null = null;
  fx: FxTheme | null = null; fxNodes: THREE.Object3D[] = []; funnelAcc = 0;
  footprint: { z: number; w: number }[] = []; shadowTick = 0; emisOverride = -1; emisOvSpeed = 1; singleIdx: Record<string, number> = {};

  constructor(el: HTMLElement, opts: ViewerOpts = {}) {
    this.el = el; this.opts = opts;
    this.qName = opts.quality && QUALITY[opts.quality] ? opts.quality : 'high'; this.Q = QUALITY[this.qName];
    const r = this.renderer = new T.WebGLRenderer({ antialias: false, preserveDrawingBuffer: !!opts.preserve, powerPreference: 'high-performance', alpha: false });
    r.setPixelRatio(opts.fixed ? 1 : Math.min(devicePixelRatio || 1, this.Q.pr));
    r.shadowMap.enabled = true; r.shadowMap.type = T.PCFSoftShadowMap; r.shadowMap.autoUpdate = false; r.toneMapping = T.NoToneMapping;
    r.domElement.addEventListener('webglcontextlost', (e) => { e.preventDefault(); this.lost = true; opts.onContextLost && opts.onContextLost(); });
    el.appendChild(r.domElement);
    const cam = this.camera = new T.PerspectiveCamera(this.baseFov, 1, 0.05, 1600);
    cam.position.set(18.2, 3.9, -9.6);
    const c = this.controls = new OrbitControls(cam, r.domElement);
    c.target.set(0, 0.95, -0.5); c.enableDamping = !opts.deterministic; c.dampingFactor = 0.09;
    c.minDistance = 1.2; c.maxDistance = 260; c.maxPolarAngle = Math.PI * 0.62;
    c.addEventListener('start', () => { this.camTween = null; this.poke(); });
    c.addEventListener('change', () => this.poke());

    this.U = {
      uCamoCol: { value: new T.Color(0.239, 0.255, 0.267) }, uCamoOn: { value: 1 }, uEmisK: { value: 4 }, uGlowGain: { value: 1 },
      uChannel: { value: 0 }, uWL: { value: 0 }, uTime: { value: 0 }, uWLLine: { value: 1 }, uWater: { value: 0 }, uCut: { value: 0 }, uAO: { value: 1 },
      uCutDir: { value: new T.Vector2(1, 0) }, uWTint: { value: lin('#4aa3b4') }, uWLCol: { value: lin('#2a93ad') },
      uHiColor: { value: lin('#d9822b') }, uWire: { value: lin('#1f6f8b') }, uHi: { value: 0 }, uLodCol: { value: new T.Color() }, uLodMix: { value: 0 },
      uAnimOverride: { value: -1 }, uAnimOvSpeed: { value: 1 },
      uGameOn: { value: 0 }, uGain: { value: new T.Vector3(1, 1, 1) }, uToe: { value: new T.Vector3(0, 0, 0) }, uSat: { value: 1 },
      uFogCol: { value: new T.Color() }, uFogDen: { value: 0 }, uMgSwap: { value: 0 }, uTmP: { value: 1 }, uTmS: { value: 1 }, uGameRaw: { value: 0 },
      uAoAmt: { value: 0 }, uAoFloor: { value: 0.52 }, uGoldShift: { value: 0 }, uPeriShift: { value: 0 }, uWaterOpa: { value: 1 },
    };
    this.WU = { uShipP: { value: new T.Vector2() }, uShipPsi: { value: 0 } };
    this.buildStage();
    this.pmrem = new T.PMREMGenerator(r);
    this.bloom = new UnrealBloomPass(new T.Vector2(256, 256), 0.35, 0.0, 0.92);
    (this.bloom as any).compositeMaterial.uniforms.bloomFactors.value = [1.0, 0.85, 0.55, 0.0, 0.0];
    this.compMat = new T.ShaderMaterial({
      uniforms: { tA: { value: null }, tB: { value: null }, uMode: { value: 0 }, uOverlay: { value: 0.5 }, uRaw: { value: 0 }, uExposure: { value: 1 }, uVig: { value: 0.1 }, uDither: { value: 1 } },
      vertexShader: 'varying vec2 vUv; void main(){ vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }', fragmentShader: S.compFS, depthTest: false, depthWrite: false,
    });
    const q = new T.Mesh(new T.PlaneGeometry(2, 2), this.compMat); q.frustumCulled = false; this.compScene.add(q);
    this.makeTargets(); this.applyQuality(); this.applyEnv();
    if (!opts.fixed) new ResizeObserver(() => this.resize()).observe(el);
    this.resize();
    this.bindPointer();
    if (!opts.deterministic) r.setAnimationLoop((now) => this.loop(now));
  }

  // ---------------------------------------------------------------- 舞台
  private buildStage() {
    const sc = this.scene;
    this.skyMat = new T.ShaderMaterial({ vertexShader: S.skyVS, fragmentShader: S.skyFS, side: T.BackSide, depthWrite: false,
      uniforms: { uZenith: { value: new T.Color() }, uHorizon: { value: new T.Color() }, uBelow: { value: new T.Color() }, uSunDir: { value: new T.Vector3() }, uSunCol: { value: new T.Color() }, uSunDisk: { value: 0 } } });
    const sky = new T.Mesh(new T.SphereGeometry(900, 48, 24), this.skyMat); sky.renderOrder = -10; sky.frustumCulled = false; sc.add(sky);
    this.envSkyMat = this.skyMat.clone();   // 环境光照用的天空(游戏外观时与可见背景分开: 背景可保持浅色影棚)
    const envSky = new T.Mesh(new T.SphereGeometry(40, 32, 16), this.envSkyMat); this.envScene.add(envSky);
    for (const [w, h, p] of [[30, 10, [-10, 18, 20]], [24, 6, [18, 10, -16]]] as [number, number, number[]][]) {
      const m = new T.Mesh(new T.PlaneGeometry(w, h), new T.MeshBasicMaterial({ color: new T.Color(3, 3, 3), side: T.DoubleSide }));
      m.position.set(p[0], p[1], p[2]); m.lookAt(0, 0, 0); this.envScene.add(m); this.panels.push(m);
    }
    this.hemi = new T.HemisphereLight(0xdfeaf3, 0x5e6e78, 0.55); sc.add(this.hemi);
    const sun = this.sun = new T.DirectionalLight(0xfff1de, 2.1); sun.castShadow = true;
    Object.assign(sun.shadow.camera, { left: -11, right: 11, top: 11, bottom: -11, near: 1, far: 60 });
    sun.shadow.bias = -0.0004; sun.shadow.normalBias = 0.01; (sun.shadow as any).radius = 4;
    sc.add(sun); sc.add(sun.target);
    this.flashLight = new T.PointLight(0xffc27a, 0, 6, 2); sc.add(this.flashLight);   // 炮口闪光（常驻，强度 0 时不影响着色器变体）
    const U = this.U, WU = this.WU, su = this.skyMat.uniforms;
    const edge = { value: lin('#1f8aa3') };
    this.water = new T.Mesh(new T.PlaneGeometry(1600, 1600, 1, 1), new T.ShaderMaterial({ vertexShader: S.flatVS, fragmentShader: S.waterFS, transparent: true, depthWrite: false, side: T.DoubleSide,
      uniforms: { uTime: U.uTime, uCut: U.uCut, uCutDir: U.uCutDir, uSunDir: su.uSunDir, uSunCol: su.uSunCol, uZenith: su.uZenith, uHorizon: su.uHorizon,
        uDeep: { value: new T.Color() }, uShallow: { value: new T.Color() }, uEdge: edge, uFoamK: { value: 0 }, uWaterOpa: U.uWaterOpa, ...WU } }));
    this.water.rotation.x = -Math.PI / 2; this.water.renderOrder = 2; sc.add(this.water);
    this.cutWall = new T.Mesh(new T.PlaneGeometry(28, 3.2), new T.ShaderMaterial({ vertexShader: S.flatVS, fragmentShader: S.cutFS, transparent: true, depthWrite: false, side: T.DoubleSide,
      uniforms: { uDeep: (this.water.material as any).uniforms.uDeep, uEdge: edge, uWL: U.uWL, uR: { value: 10 } } }));
    this.cutWall.position.y = -1.6; this.cutWall.renderOrder = 1; sc.add(this.cutWall);
    this.floor = new T.Mesh(new T.PlaneGeometry(220, 220), new T.ShaderMaterial({ vertexShader: S.flatVS, fragmentShader: S.floorFS, transparent: true, depthWrite: false,
      extensions: { derivatives: true } as any,
      uniforms: { uCol: { value: new T.Color() }, uLine: { value: new T.Color() }, uGrid: { value: 1.0 }, uFade: { value: 60 }, uLineA: { value: 0.0 }, ...WU } }));
    this.floor.rotation.x = -Math.PI / 2; this.floor.renderOrder = -1; sc.add(this.floor);
    // 阴影接收面（只画阴影，地面颜色由 floor 负责）
    const shadowCatcher = new T.Mesh(new T.PlaneGeometry(90, 90), new T.ShadowMaterial({ opacity: 0.16 }));
    shadowCatcher.rotation.x = -Math.PI / 2; shadowCatcher.receiveShadow = true; shadowCatcher.renderOrder = 0; sc.add(shadowCatcher); (this as any).shadowCatcher = shadowCatcher;
    this.blob = new T.Mesh(new T.PlaneGeometry(1, 1), new T.ShaderMaterial({ vertexShader: S.flatVS, fragmentShader: S.blobFS, transparent: true, depthWrite: false, uniforms: { uA: { value: 0.2 }, uHalf: { value: new T.Vector2(1.45, 8.9) } } }));
    this.blob.rotation.x = -Math.PI / 2; this.blob.renderOrder = 1; sc.add(this.blob);
    this.wlSheet = new T.Mesh(new T.PlaneGeometry(1, 1), new T.ShaderMaterial({ vertexShader: S.flatVS, fragmentShader: S.sheetFS, transparent: true, depthWrite: false, side: T.DoubleSide,
      uniforms: { uCol: { value: new T.Color() }, uA: { value: 0.07 }, uHalf: { value: new T.Vector2(3.6, 11.5) } } }));
    this.wlSheet.rotation.x = -Math.PI / 2; this.wlSheet.renderOrder = 3; sc.add(this.wlSheet);
    sc.add(this.shipFrame);
    // 模拟用：白沫、尾迹、粒子、目标标记
    this.foamMat = new T.ShaderMaterial({ vertexShader: S.foamVS, fragmentShader: S.foamFS, transparent: true, depthWrite: false, side: T.DoubleSide,
      polygonOffset: true, polygonOffsetFactor: -2, uniforms: { uTime: U.uTime, uK: { value: 1 }, uFoam: { value: new T.Color(1, 1, 1) }, ...WU } });
    this.wake = new Trail(this.foamMat, 160, 16, 0.32, 0.16);
    this.bowL = new Trail(this.foamMat, 90, 7, 0.07, 0.05); this.bowR = new Trail(this.foamMat, 90, 7, 0.07, 0.05);
    for (const t of [this.wake, this.bowL, this.bowR]) sc.add(t.mesh);
    this.particles = new Particles(this.Q.particles, S.smokeVS, S.smokeFS); sc.add(this.particles.points); sc.add(this.particles.pointsAdd);
    this.aimMarker = new T.Mesh(new T.RingGeometry(0.42, 0.5, 48), new T.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.75, depthWrite: false, side: T.DoubleSide }));
    this.aimMarker.rotation.x = -Math.PI / 2; this.aimMarker.visible = false; this.aimMarker.renderOrder = 8; sc.add(this.aimMarker);
    this.fitStage();
  }

  private fitStage() {
    const B = this.BOX;
    const cx = (B.min.x + B.max.x) / 2, cz = (B.min.z + B.max.z) / 2, hx = (B.max.x - B.min.x) / 2, hz = (B.max.z - B.min.z) / 2;
    this.FLOOR_Y = B.min.y - 0.33;
    this.floor.position.y = this.FLOOR_Y; (this as any).shadowCatcher.position.y = this.FLOOR_Y + 0.001;
    this.blob.position.set(cx, this.FLOOR_Y + 0.003, cz); this.blob.scale.set(hx * 2.5, hz * 2.5, 1); (this.blob.material as any).uniforms.uHalf.value.set(hx * 1.2, hz * 1.0);
    this.wlSheet.position.set(cx, this.WL, cz); this.wlSheet.scale.set(hx * 7.6, hz * 3.0, 1); (this.wlSheet.material as any).uniforms.uHalf.value.set(hx * 3.0, hz * 1.3);
    this.cutWall.geometry.dispose(); this.cutWall.geometry = new T.PlaneGeometry(hz * 3.2, 3.2); (this.cutWall.material as any).uniforms.uR.value = hz * 1.1;
    const r = Math.max(hx, hz) + 2.5;
    Object.assign(this.sun.shadow.camera, { left: -r, right: r, top: r, bottom: -r }); this.sun.shadow.camera.updateProjectionMatrix();
    (this.floor.material as any).uniforms.uFade.value = hz * 3.2;
  }

  private makeTargets() {
    if (this.rtA) { this.rtA.dispose(); this.rtB.dispose(); }
    const opt: any = { type: T.HalfFloatType, samples: this.Q.msaa };
    this.rtA = new T.WebGLRenderTarget(4, 4, opt); this.rtB = new T.WebGLRenderTarget(4, 4, opt);
    this.compMat.uniforms.tA.value = this.rtA.texture; this.compMat.uniforms.tB.value = this.rtB.texture;
  }
  private applyQuality() {
    if (!this.opts.fixed) this.renderer.setPixelRatio(Math.min(devicePixelRatio || 1, this.Q.pr));
    this.sun.castShadow = this.Q.shadow > 0;
    if (this.Q.shadow) { this.sun.shadow.mapSize.set(this.Q.shadow, this.Q.shadow); if (this.sun.shadow.map) { this.sun.shadow.map.dispose(); (this.sun.shadow as any).map = null; } }
    this.renderer.shadowMap.needsUpdate = true; setAniso(this.Q.aniso); this.poke();
  }
  setQuality(name: Quality) {
    if (!QUALITY[name] || name === this.qName) return false;
    const prev = this.Q.msaa; this.qName = name; this.Q = QUALITY[name];
    if (this.Q.msaa !== prev) this.makeTargets();
    this.applyQuality(); this.resize();
    if (!this.Q.overlay && this.mode === 'overlay') this.setMode('split');
    this.matCache.forEach((m) => { m.needsUpdate = true; });
    return true;
  }
  texMax() { return this.Q.tex; }

  /** 当前生效的游戏外观参数(look=studio 时为 null) */
  gameParams(): GameLook | null { return this.look === 'game' ? { ...gameLook(this.light), ...this.lookOv } as GameLook : null; }
  applyEnv() {
    const e = this.pal = palette(this.env, this.light === 'dusk' && this.look === 'game' ? 'day' : this.light); const isWater = this.env === 'water', dark = this.light === 'night';
    const g = this.gameParams();
    const su = this.skyMat.uniforms, eu = this.envSkyMat.uniforms;
    // 可见背景: 影棚 = 影棚色; 清水 + 游戏外观 = 港口天空/海色
    const bg = g && isWater ? { zenith: g.zenith, horizon: g.horizon, below: g.below } : e;
    su.uZenith.value = lin(bg.zenith); su.uHorizon.value = lin(bg.horizon); su.uBelow.value = lin(bg.below);
    const sunCol = g ? g.sun : e.sun, sunI = g ? g.sunI : e.sunI;
    su.uSunCol.value = lin(sunCol); su.uSunDisk.value = isWater ? (g ? 0.6 : e.sunDisk) : 0;
    const sd = new T.Vector3(...(g ? g.sunDir : e.sunDir)).normalize(); su.uSunDir.value.copy(sd);
    // 环境贴图(IBL): 游戏外观用港口天空 + 海面反光, 不要影棚灯板
    const env = g ? { zenith: g.zenith, horizon: g.horizon, below: g.below } : e;
    eu.uZenith.value = lin(env.zenith); eu.uHorizon.value = lin(env.horizon); eu.uBelow.value = lin(env.below); eu.uSunCol.value = lin(sunCol); eu.uSunDir.value.copy(sd); eu.uSunDisk.value = 0;
    this.sun.position.copy(sd).multiplyScalar(25); this.sun.color = lin(sunCol); this.sun.intensity = sunI;
    this.hemi.intensity = g ? g.hemi : e.hemi; this.hemi.color = lin(g ? g.hemiSky : e.hemiSky); this.hemi.groundColor = lin(g ? g.hemiGround : e.hemiGround);
    const wu = (this.water.material as any).uniforms;
    if (isWater) { wu.uDeep.value = lin(g ? g.deep : e.deep!); wu.uShallow.value = lin(g ? g.shallow : e.shallow!); this.U.uWTint.value = lin(g ? g.tint : e.tint!); }
    const wActive = isWater && this.waterMode !== 'off';
    this.water.visible = wActive; this.cutWall.visible = wActive && this.waterMode === 'cut';
    this.U.uWater.value = wActive ? 1 : 0; this.U.uCut.value = this.waterMode === 'cut' && wActive ? 1 : 0;
    const fu = (this.floor.material as any).uniforms; fu.uCol.value = lin(e.floor); fu.uLine.value = lin(e.grid);
    this.floor.visible = !isWater; (this as any).shadowCatcher.visible = !isWater; (this as any).shadowCatcher.material.opacity = dark ? 0.3 : 0.16;
    this.blob.visible = !isWater && !this.simOn; (this.blob.material as any).uniforms.uA.value = e.shadow;
    this.wlSheet.visible = this.wlOn && !wActive && !this.simOn; (this.wlSheet.material as any).uniforms.uCol.value = lin(e.sheet); (this.wlSheet.material as any).uniforms.uA.value = dark ? 0.09 : 0.07;
    this.U.uWLLine.value = this.wlOn && !(wActive && this.waterMode === 'full') ? 1 : 0; this.U.uWLCol.value = lin(e.wl);
    this.foamMat.uniforms.uFoam.value = fxTint(lin(e.foam), this.fx?.wake); this.foamMat.uniforms.uK.value = isWater ? 1 : 0.85;
    this.panels.forEach((p) => { (p.material as THREE.MeshBasicMaterial).color.setScalar(e.panel); p.visible = !g; });
    if (this.envRT) this.envRT.dispose();
    this.envRT = this.pmrem.fromScene(this.envScene, 0.02, 0.1, 100); this.scene.environment = this.envRT.texture;
    const envI = g ? g.envI : e.envI;
    this.matCache.forEach((m) => { m.envMapIntensity = envI * (m.userData.envMul || 1); });
    // 船体响应(仅船体材质; 背景不受影响)
    const U = this.U; U.uGameOn.value = g ? 1 : 0;
    if (g) {
      U.uGain.value.set(...g.gain); U.uToe.value.set(...g.toe); U.uTmP.value = g.tmP; U.uTmS.value = g.tmS ?? 1; U.uSat.value = g.sat;
      U.uFogCol.value = lin(g.fogCol); U.uFogDen.value = g.fogDen; U.uMgSwap.value = g.mgSwap; U.uEmisK.value = g.emisK; U.uGameRaw.value = g.raw || 0;
      U.uAoAmt.value = g.contactAO ?? 0; U.uAoFloor.value = 0.52; U.uGoldShift.value = Math.min(1, Math.max(0, (g.goldHue ?? 0) / 25)); U.uPeriShift.value = Math.max(-1, Math.min(1, (g.periHue ?? 0) / 30)); U.uWaterOpa.value = g.waterOpa ?? 1;
    } else { U.uMgSwap.value = 0; U.uEmisK.value = 4; U.uAoAmt.value = 0; U.uGoldShift.value = 0; U.uPeriShift.value = 0; U.uWaterOpa.value = 1; }
    this.compMat.uniforms.uExposure.value = g ? g.exposure : e.exposure; this.compMat.uniforms.uVig.value = g ? 0 : e.vig;
    this.compMat.uniforms.uRaw.value = this.rawComposite() ? 1 : 0;
    this.updateBloom(); this.updateSimVis();
    this.renderer.shadowMap.needsUpdate = true; this.poke();
  }
  private updateBloom() {
    const b = this.bloom as any;
    const g = this.gameParams();
    if (this.channel === 3) { b.strength = 0.8; b.threshold = 0.3; } else if (g) { b.strength = g.bloomS; b.threshold = g.bloomT; } else { b.strength = this.pal.bloomS; b.threshold = this.pal.bloomT; }
    b.radius = 0.0;
  }
  resize() {
    const [w, h] = this.opts.fixed || [Math.max(1, this.el.clientWidth), Math.max(1, this.el.clientHeight)];
    this.renderer.setSize(w, h, !this.opts.fixed);
    const pr = this.renderer.getPixelRatio(), W = Math.round(w * pr), H = Math.round(h * pr);
    this.rtA.setSize(W, H); this.rtB.setSize(W, H); this.bloom.setSize(W, H);
    this.camera.aspect = w / h; this.camera.updateProjectionMatrix();
    this.particles.setScale(H * 0.5 / Math.tan(T.MathUtils.degToRad(this.baseFov / 2)));
    this.poke();
  }
  size(): [number, number] { return this.opts.fixed || [this.el.clientWidth, this.el.clientHeight]; }
  setFixedSize(w: number, h: number) { this.opts.fixed = [w, h]; this.resize(); }

  // ---------------------------------------------------------------- 几何
  loadGeometry(buf: ArrayBuffer) {
    const { root } = parseGLB(buf);
    if (this.ship) { this.shipFrame.remove(this.ship); this.meshes.length = 0; this.parts = {}; this.mounts = []; }
    root.updateMatrixWorld(true);
    const box = new T.Box3();
    // 特效挂点(extras.fx: HP_FX_Smoke_* / HP_FX_WakeTrace* …)不是部件, 单独收起来
    this.fxNodes = root.children.filter((o) => o.userData && o.userData.fx);
    // 顶层节点 = 部件（船体分段 / 挂点）
    root.children.forEach((top) => {
      if (top.userData && top.userData.fx) return;
      const pid = top.userData.part || top.name;
      const p: PartRec = this.parts[pid] = { id: pid, meshes: [], visible: true, verts: 0, tris: 0, lods: top.userData.lods || [{ lod: 0, extent: 1e9 }], lod: 0, center: new T.Vector3(), top };
      const pb = new T.Box3();
      top.traverse((o: any) => {
        if (!o.isMesh) return;
        o.userData.part = pid; p.meshes.push(o); this.meshes.push(o);
        if ((o.userData.lod || 0) === 0) {
          const g = o.geometry; p.verts += g.attributes.position.count; p.tris += (g.index ? g.index.count : g.attributes.position.count) / 3;
          box.expandByObject(o); pb.expandByObject(o);
        }
        o.userData.pickable = true; o.onBeforeRender = this.onMeshRender;
      });
      if (!pb.isEmpty()) pb.getCenter(p.center);
    });
    if (!box.isEmpty()) { this.BOX = box; this.fitStage(); }
    this.ship = root; this.shipFrame.add(root);
    // model animate: 由 .seq 采样的独立模型(extras.anim.frames = glTF 世界矩阵)
    this.animNodes = root.children.filter((o: any) => o.userData.anim && o.userData.anim.frames && o.userData.anim.frames.length);
    this.meshes.forEach((m) => { m.material = this.neutralMat(); this.applyMeshFlags(m, m.material as any); });
    this.computeFootprint(); this.computeFitPts();
    this.applyLod(true);
    if (this.simData) this.setupMounts();
    this.renderer.shadowMap.needsUpdate = true;
    this.preset('hero', true);
    const out: Record<string, any> = {}; let tv = 0, tt = 0;
    Object.values(this.parts).forEach((p) => { out[p.id] = { verts: p.verts, tris: p.tris, prims: p.meshes.filter((m) => !m.userData.lod).length, lods: p.lods.length }; tv += p.verts; tt += p.tris; });
    return { parts: out, verts: tv, tris: tt, prims: this.meshes.filter((m) => !m.userData.lod).length, bounds: { min: this.BOX.min.toArray(), max: this.BOX.max.toArray() } };
  }

  /** 取景用的采样点（LOD0 顶点抽样，约 6000 个）：比包围盒角点紧，斜视角不会留大片空白 */
  fitPts: THREE.Vector3[] = [];
  private computeFitPts() {
    const lod0 = this.meshes.filter((m) => !m.userData.lod); let total = 0;
    lod0.forEach((m) => { total += m.geometry.attributes.position.count; });
    const step = Math.max(1, Math.floor(total / 6000)); const pts: THREE.Vector3[] = [];
    lod0.forEach((m) => { const a = m.geometry.attributes.position; m.updateMatrixWorld(); const M = m.matrixWorld;
      for (let i = 0; i < a.count; i += step) pts.push(new T.Vector3(a.getX(i), a.getY(i), a.getZ(i)).applyMatrix4(M)); });
    this.fitPts = pts;
  }
  /** 水线处船体半宽轮廓（船体 LOD0 中 |y| 小的顶点），用于舷侧白沫与船首浪 */
  private computeFootprint() {
    const bins = 72, B = this.BOX, z0 = B.min.z, z1 = B.max.z, w = new Array(bins).fill(0); const v = new T.Vector3();
    Object.values(this.parts).filter((p) => p.id.startsWith('hull/')).forEach((p) => p.meshes.filter((m) => !m.userData.lod).forEach((m) => {
      const pos = m.geometry.attributes.position as THREE.BufferAttribute; m.updateMatrixWorld();
      for (let i = 0; i < pos.count; i += 1) {
        v.fromBufferAttribute(pos, i).applyMatrix4(m.matrixWorld);
        if (Math.abs(v.y - this.WL) > 0.06) continue;
        const k = Math.min(bins - 1, Math.max(0, Math.floor((v.z - z0) / (z1 - z0) * bins)));
        w[k] = Math.max(w[k], Math.abs(v.x));
      }
    }));
    this.footprint = w.map((x, k) => ({ z: z0 + (k + 0.5) / bins * (z1 - z0), w: x })).filter((f) => f.w > 0.02);
    this.buildSideFoam();
  }
  private buildSideFoam() {
    if (this.sideFoam) { this.scene.remove(this.sideFoam); this.sideFoam.geometry.dispose(); }
    const F = this.footprint; if (F.length < 4) return;
    const pos: number[] = [], uv: number[] = [], al: number[] = [], idx: number[] = [];
    const zMin = F[0].z, zMax = F[F.length - 1].z, len = zMax - zMin;
    for (const s of [-1, 1]) {
      const base = pos.length / 3;
      F.forEach((f, i) => {
        const t = (f.z - zMin) / len;                          // 0 = 船首, 1 = 船尾
        const wd = 0.05 + 0.10 * Math.pow(1 - t, 2) + 0.06 * t;  // 船首附近更宽
        const a = 0.45 + 0.55 * Math.pow(1 - t, 1.5);
        pos.push(s * (f.w - 0.01), 0, f.z, s * (f.w + wd), 0, f.z); uv.push(i, -1, i, 1); al.push(a, a);
        if (i > 0) { const k = base + (i - 1) * 2; idx.push(k, k + 1, k + 2, k + 1, k + 3, k + 2); }
      });
    }
    const g = new T.BufferGeometry();
    g.setAttribute('position', new T.Float32BufferAttribute(pos, 3)); g.setAttribute('aUv', new T.Float32BufferAttribute(uv, 2)); g.setAttribute('aA', new T.Float32BufferAttribute(al, 1));
    g.setIndex(idx);
    const mat = this.foamMat.clone(); mat.uniforms = { ...this.foamMat.uniforms, uK: { value: 0 } };
    this.sideFoam = new T.Mesh(g, mat); this.sideFoam.position.y = this.WL + 0.004; this.sideFoam.renderOrder = 4; this.sideFoam.frustumCulled = false;
    this.scene.add(this.sideFoam); this.updateSimVis();
  }

  private neutralMat() {
    if (!this.neutral) this.neutral = this.buildMaterial({ family: 'pbs', params: {}, textures: {} }, {}, 'neutral');
    return this.neutral;
  }
  private onMeshRender = function (this: THREE.Mesh, _r: any, _s: any, _c: any, _g: any, mat: any) {
    const V: Viewer = (this as any).__viewer;
    const p = this.userData.part; let v = 0;
    if (!V.passIsA && V.hiOn && V.channel < 3 && V.changedMfm.has(this.userData.mfm)) v = 0.16;
    if (!V.passIsA && V.flashT > 0 && V.flashSet.has(p)) v = Math.max(v, 0.1 + 0.35 * V.flashT);
    if (V.light === 'night') v *= 0.35;
    if (V.simOn) v *= 0.5;
    V.U.uHi.value = v;
    if (V.lodTint) { V.U.uLodCol.value.copy(lin(LOD_COLORS[Math.min(3, this.userData.lod || 0)])); V.U.uLodMix.value = 0.55; } else V.U.uLodMix.value = 0;
    mat.uniformsNeedUpdate = true;
  };
  private applyMeshFlags(m: THREE.Mesh, mat: any) {
    const fam = mat.userData.family;
    m.castShadow = (fam === 'pbs' || fam === 'emissive') && (m.userData.lod || 0) < 2; m.receiveShadow = fam !== 'glass';
    m.renderOrder = fam === 'glass' ? 5 : 0; (m as any).__viewer = this;
  }

  // ---------------------------------------------------------------- 材质
  private buildMaterial(def: MaterialDef, tex: Record<string, THREE.Texture>, sig: string) {
    const fam = def.family || 'pbs', p = def.params || {};
    const m = new T.MeshStandardMaterial({ color: 0xffffff, roughness: 0.6, metalness: 0.0, side: T.FrontSide });
    m.map = tex.a || FALLBACK.a;
    if (tex.n) { m.normalMap = tex.n; m.normalScale = new T.Vector2(1, 1); }
    const hasMG = !!tex.mg; if (hasMG) m.roughnessMap = tex.mg;
    const defines: Record<string, string> = {};
    if (fam === 'pbs') defines.FAM_PBS = ''; if (fam === 'emissive') defines.FAM_EMISSIVE = '';
    if (hasMG) defines.HAS_MG = ''; if (tex.ao) defines.HAS_AO = '';
    (m as any).defines = defines;
    m.userData = { family: fam, sig, envMul: 1, aoTex: tex.ao || null };
    if (fam === 'wire') { m.alphaTest = 0.5; m.side = T.DoubleSide; }
    if (fam === 'alpha') { m.alphaTest = 0.4; m.side = T.DoubleSide; m.roughness = 0.55; m.metalness = 0.3; }
    if (fam === 'glass') {
      m.transparent = true; m.depthWrite = false; m.side = T.DoubleSide; m.roughness = 0.06; m.metalness = 0.0;
      if (!tex.a) { m.map = null; m.color = lin('#2d3d4a'); m.opacity = 0.5; } else m.opacity = 0.85;
      m.userData.envMul = 1.6;
    }
    // .mfm 的 doubleSided / alphaTestEnable + alphaReference(0..255)
    if (p.doubleSided) m.side = T.DoubleSide;
    if (p.alphaTestEnable) m.alphaTest = Math.max(1, p.alphaReference != null ? +p.alphaReference : 128) / 255;
    // 发光参数（.mfm）：emissivePower（默认 2.0）、emissionAnimationMode、animEmissionPower；动画速度 animSpeed 未确认，缺省 1 Hz（近似）
    const loc = {
      uEmisPower: { value: p.emissivePower != null ? +p.emissivePower : 2.0 },
      uAnimMode: { value: p.emissionAnimationMode != null ? +p.emissionAnimationMode : 0 },
      uAnimPower: { value: p.animEmissionPower != null ? +p.animEmissionPower : (p.emissivePower != null ? +p.emissivePower : 1.0) },
      uAnimSpeed: { value: p.animSpeed != null ? +p.animSpeed : 1.0 },
      tAO: { value: tex.ao || FALLBACK.ao },
    };
    m.onBeforeCompile = (sh) => {
      Object.assign(sh.uniforms, this.U, loc);
      sh.vertexShader = sh.vertexShader.replace('#include <common>', '#include <common>\nvarying vec3 vWPos;')
        .replace('#include <project_vertex>', '#include <project_vertex>\nvWPos = (modelMatrix*vec4(transformed,1.0)).xyz;');
      sh.fragmentShader = sh.fragmentShader.replace('#include <common>', '#include <common>\n' + S.SHIP_COMMON)
        .replace('#include <map_fragment>', S.FRAG.map).replace('#include <roughnessmap_fragment>', S.FRAG.rough).replace('#include <metalnessmap_fragment>', S.FRAG.metal)
        .replace('#include <normal_fragment_maps>', S.FRAG.normal).replace('#include <emissivemap_fragment>', S.FRAG.emis).replace('#include <aomap_fragment>', S.FRAG.ao)
        .replace('#include <dithering_fragment>', '#include <dithering_fragment>\n' + S.FRAG.post);
    };
    m.customProgramCacheKey = () => 'wm2|' + fam + '|' + Object.keys(defines).join(',') + '|' + (m.normalMap ? 'n' : '') + (m.map ? 'a' : '');
    const gp = this.gameParams(); m.envMapIntensity = (gp ? gp.envI : this.pal.envI) * m.userData.envMul;
    m.wireframe = this.channel === 4;
    return m;
  }
  private static SLOTS: [string, boolean][] = [['a', true], ['n', false], ['mg', false], ['ao', false]];
  private sigOf(def: MaterialDef) { const t: any = def.textures || {}; return [def.family, JSON.stringify(def.params || {}), ...Viewer.SLOTS.map(([s]) => (t[s] && t[s].hash) || '-')].join('|'); }

  async applyManifest(side: 'A' | 'B', manifest: Manifest, onProgress?: (d: number, t: number) => void) {
    const mats = manifest.materials || {}, max = this.Q.tex;
    if (manifest.camo && manifest.camo.color0) this.U.uCamoCol.value.setRGB(...manifest.camo.color0);
    const result = { missing: [] as string[], failed: [] as string[] }; const next: Record<string, string> = {};
    let done = 0, total = 0; const pending: Promise<void>[] = [];
    Object.entries(mats).forEach(([mfm, def]) => {
      const sig = this.sigOf(def) + '|' + max; next[mfm] = sig;
      if (this.matCache.has(sig)) return;
      const tex: Record<string, THREE.Texture> = {}, defT: any = def.textures || {};
      pending.push(Promise.all(Viewer.SLOTS.map(async ([slot, srgb]) => {
        const ti = defT[slot]; if (!ti) return;
        if (ti.missing || !ti.url) { result.missing.push(ti.name || mfm + ':' + slot); return; }
        total++;
        try { tex[slot] = await getTex(ti, { srgb, max }); } catch { result.failed.push(ti.name); }
        done++; onProgress && onProgress(done, total);
      })).then(() => { if (!this.matCache.has(sig)) this.matCache.set(sig, this.buildMaterial(def, tex, sig)); }));
    });
    onProgress && onProgress(done, total);
    await Promise.all(pending);
    this.sets[side] = {}; Object.entries(next).forEach(([mfm, sig]) => { this.sets[side][mfm] = this.matCache.get(sig)!; });
    this.recomputeChanged();
    if (side === 'B') this.meshes.forEach((m) => { const mt = this.sets.B[m.userData.mfm]; if (mt) { m.material = mt; this.applyMeshFlags(m, mt); } });
    this.renderer.shadowMap.needsUpdate = true; this.poke();
    return result;
  }
  clearSide(side: 'A' | 'B') { this.sets[side] = {}; this.recomputeChanged(); this.poke(); }
  private recomputeChanged() { this.changedMfm = new Set(); Object.keys(this.sets.B).forEach((mfm) => { if (this.sets.A[mfm] && this.sets.A[mfm] !== this.sets.B[mfm]) this.changedMfm.add(mfm); }); }
  gc() {
    const used = new Set([...Object.values(this.sets.A), ...Object.values(this.sets.B)]); const keep = new Set<THREE.Texture>();
    this.matCache.forEach((m, k) => { if (used.has(m)) { [m.map, m.normalMap, m.roughnessMap, m.userData.aoTex].forEach((t) => t && keep.add(t)); } else { m.dispose(); this.matCache.delete(k); } });
    return texGC(keep);
  }
  changedParts() { const s = new Set<string>(); this.meshes.forEach((m) => { if (this.changedMfm.has(m.userData.mfm)) s.add(m.userData.part); }); return [...s]; }
  hasEmissive() { return Object.values(this.sets.B).some((m: any) => m && m.userData.family === 'emissive'); }
  emissiveAnimModes() { const s = new Set<number>(); Object.values(this.sets.B).forEach((m: any) => { if (m && m.userData.family === 'emissive') s.add(0); }); return s; }

  // ---------------------------------------------------------------- LOD
  /** 自动：相机到部件中心的距离 ≤ extent × 偏移 的最低一级；超过最后一级的 extent 就不画（与 BigWorld .model 链一致） */
  applyLod(force = false) {
    const camL = this.shipFrame.worldToLocal(this.camera.position.clone());
    let changed = false;
    for (const p of Object.values(this.parts)) {
      let lod = 0;
      if (this.lodMode === 'auto') {
        const d = camL.distanceTo(p.center); lod = -1;
        for (const l of p.lods) { if (d <= l.extent * this.lodBias) { lod = l.lod; break; } }
      } else lod = Math.min(this.lodMode as number, p.lods.length - 1);
      if (lod !== p.lod || force) { p.lod = lod; changed = true; p.meshes.forEach((m) => { m.visible = p.visible && (m.userData.lod || 0) === lod; }); }
    }
    if (changed) this.renderer.shadowMap.needsUpdate = true;
    return changed;
  }
  lodState() { const o: Record<string, number> = {}; Object.values(this.parts).forEach((p) => { o[p.id] = p.lod; }); return o; }
  setLodMode(m: 'auto' | number) { this.lodMode = m; this.applyLod(true); this.poke(); }
  setLodBias(b: number) { this.lodBias = b; this.applyLod(); this.poke(); }
  setLodTint(on: boolean) { this.lodTint = on; this.poke(); }

  // ---------------------------------------------------------------- 视图状态
  setMode(m: Mode) { if (m === 'overlay' && !this.Q.overlay) m = 'split'; this.mode = m; this.compMat.uniforms.uMode.value = { single: 0, split: 1, side: 2, overlay: 3 }[m]; this.poke(); return m; }
  setSplit(x: number) { this.split = x; this.poke(); }
  setOverlay(x: number) { this.compMat.uniforms.uOverlay.value = x; this.poke(); }
  setChannel(c: number) {
    this.channel = c; this.U.uChannel.value = c; this.compMat.uniforms.uRaw.value = this.rawComposite() ? 1 : 0;
    this.matCache.forEach((m) => { m.wireframe = c === 4; }); if (this.neutral) this.neutral.wireframe = c === 4; this.updateBloom(); this.poke();
  }
  /** 合成时不做 Neutral 色调映射: 原始通道视图, 或游戏外观(船体已在着色器里按港口曲线映射, 背景按标称色显示) */
  private rawComposite() { const c = this.channel; return c === 1 || c === 2 || c === 4 || c === 5 || this.look === 'game'; }
  setEnv(e: EnvName) { this.env = e; this.applyEnv(); }
  setLight(l: LightName) { this.light = l; this.applyEnv(); }
  setLook(l: LookName) { this.look = l === 'studio' ? 'studio' : 'game'; this.applyEnv(); }
  /** 临时覆盖游戏外观参数(拟合脚本 / 出图 gl= 参数用); 传 {} 还原 */
  setLookOverride(o: Partial<GameLook>) { this.lookOv = o || {}; this.applyEnv(); }
  setWaterline(on: boolean) { this.wlOn = on; this.applyEnv(); }
  setWaterMode(m: WaterMode) { this.waterMode = m; this.applyEnv(); }
  setGlowGain(g: number) { this.glowGain = g; this.U.uGlowGain.value = g; this.poke(); }
  setCamo(on: boolean) { this.camoOn = on; this.U.uCamoOn.value = on ? 1 : 0; this.poke(); }
  setHighlight(on: boolean) { this.hiOn = on; this.poke(); }
  setEmisOverride(mode: number, speed = 1) { this.emisOverride = mode; this.U.uAnimOverride.value = mode; this.U.uAnimOvSpeed.value = speed; this.poke(); }
  setPartVisible(id: string, v: boolean) { const p = this.parts[id]; if (!p) return; p.visible = v; this.applyLod(true); this.poke(); }
  flash(ids: string[]) { this.flashSet = new Set(ids); this.flashT = 1.0; this.poke(); }
  onPick(cb: (id: string | null) => void) { this.pickCb = cb; }
  onFrame(cb: () => void) { this.frameCb = cb; }
  stats() {
    const r = this.renderer;
    return { fps: this.fps, cpuMs: this.cpuMs, idle: this.idle, draws: this.drawCalls, tris: this.tris, quality: this.qName, gpuTexMB: texGpuBytes() / 1048576, pr: r.getPixelRatio(),
      gl: (() => { try { const gl = r.getContext(); const ext = gl.getExtension('WEBGL_debug_renderer_info'); return ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER); } catch { return '?'; } })() };
  }
  poke() { this.dirty = Math.max(this.dirty, 2); this.lastInput = performance.now(); }
  pause(p: boolean) { this.paused = p; }
  time0() { return this.time; }
  setTime(t: number) { this.time = t; this.U.uTime.value = t; }

  // ---------------------------------------------------------------- 相机
  static PRESETS: Record<string, { dir?: number[]; pos?: number[]; tgt: number[]; fill?: number }> = {
    hero: { dir: [0.86, 0.17, -0.47], tgt: [0, 0.6, -0.2], fill: 0.86 },
    side: { dir: [1, 0.04, 0], tgt: [0, 0.55, 0.05], fill: 0.92 },
    bow: { dir: [0.62, 0.22, -0.75], tgt: [0, 0.6, -0.6], fill: 0.88 },
    stern: { dir: [-0.62, 0.24, 0.75], tgt: [0, 0.6, 0.8], fill: 0.88 },
    top: { dir: [-0.02, 1, 0.0005], tgt: [0, 0, 0.05], fill: 0.92 },
    port: { pos: [5.0, 0.55, -13.6], tgt: [0.2, 1.3, -3.4] },
    funnel: { pos: [3.9, 2.55, 4.9], tgt: [0, 1.5, 1.95] },
  };
  private setCamFor(cam: THREE.PerspectiveCamera, aspect: number) {
    cam.aspect = aspect;
    cam.fov = aspect < this.baseAspect * 0.75 ? 2 * Math.atan(Math.tan(T.MathUtils.degToRad(this.baseFov) / 2) * 1.35) * 180 / Math.PI : this.baseFov;
    cam.updateProjectionMatrix();
  }
  private fitDistance(dir: THREE.Vector3, tgt: THREE.Vector3, fill: number, box = this.BOX) {
    const cam = this.camera.clone(); const [w, h] = this.size(); const full = w / Math.max(1, h); this.baseAspect = full;
    this.setCamFor(cam, this.mode === 'side' && Object.keys(this.sets.A).length ? full / 2 : full);
    let pts: THREE.Vector3[] = [];
    if (box === this.BOX && this.fitPts.length) pts = this.fitPts;
    else for (let i = 0; i < 8; i++) pts.push(new T.Vector3(i & 1 ? box.max.x : box.min.x, i & 2 ? box.max.y : box.min.y, i & 4 ? box.max.z : box.min.z));
    let lo = 0.3, hi = 400;
    const q = new T.Vector3();
    for (let k = 0; k < 24; k++) {
      const d = (lo + hi) / 2; cam.position.copy(tgt).addScaledVector(dir, d); cam.lookAt(tgt); cam.updateMatrixWorld(); cam.matrixWorldInverse.copy(cam.matrixWorld).invert();
      const ok = pts.every((p) => { q.copy(p).project(cam); return q.z < 1 && Math.abs(q.x) <= fill && Math.abs(q.y) <= fill; });
      if (ok) hi = d; else lo = d;
    }
    return hi;
  }
  presetPose(name: string) {
    const p = Viewer.PRESETS[name]; if (!p) return null;
    const tgt = new T.Vector3(...p.tgt as [number, number, number]);
    if (p.pos) return { pos: new T.Vector3(...p.pos as [number, number, number]), tgt };
    const dir = new T.Vector3(...p.dir as [number, number, number]).normalize();
    return this.fitCentered(dir, tgt, p.fill || 0.9);
  }
  /** 沿 dir 取景并把船在画面里居中（两次迭代：先求距离，再按投影包围框平移目标点） */
  fitCentered(dir: THREE.Vector3, tgt0: THREE.Vector3, fill: number) {
    const tgt = tgt0.clone(); let d = this.fitDistance(dir, tgt, fill);
    if (this.fitPts.length) for (let it = 0; it < 2; it++) {
      const cam = this.camera.clone(); const [w, h] = this.size(); const full = w / Math.max(1, h);
      this.setCamFor(cam, this.mode === 'side' && Object.keys(this.sets.A).length ? full / 2 : full);
      cam.position.copy(tgt).addScaledVector(dir, d); cam.lookAt(tgt); cam.updateMatrixWorld();
      let x0 = 1e9, x1 = -1e9, y0 = 1e9, y1 = -1e9; const q = new T.Vector3();
      for (const p of this.fitPts) { q.copy(p).project(cam); if (q.z >= 1) continue; x0 = Math.min(x0, q.x); x1 = Math.max(x1, q.x); y0 = Math.min(y0, q.y); y1 = Math.max(y1, q.y); }
      const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2;
      const hh = Math.tan(T.MathUtils.degToRad(cam.fov) / 2) * d, hw = hh * cam.aspect;
      const right = new T.Vector3().setFromMatrixColumn(cam.matrixWorld, 0), up = new T.Vector3().setFromMatrixColumn(cam.matrixWorld, 1);
      tgt.addScaledVector(right, cx * hw).addScaledVector(up, cy * hh);
      d = this.fitDistance(dir, tgt, fill);
    }
    return { pos: tgt.clone().addScaledVector(dir, d), tgt };
  }
  private goTo(to: { pos: THREE.Vector3; tgt: THREE.Vector3 }, instant?: boolean) {
    if (instant) { this.camera.position.copy(to.pos); this.controls.target.copy(to.tgt); this.controls.update(); this.camTween = null; this.syncFreeCam(); this.poke(); return; }
    this.camTween = { from: { pos: this.camera.position.clone(), tgt: this.controls.target.clone() }, to, t: 0 }; this.poke();
  }
  preset(name: string, instant?: boolean) { const to = this.presetPose(name); if (to) this.goTo(to, instant); }
  /** 方位角（度，0 = 船首方向，正 = 右舷）、俯仰（度）、距离（模型单位；省略 = 自动取景）、目标点 */
  orbit(az: number, el: number, dist?: number, tgt?: number[], instant = true) {
    const a = T.MathUtils.degToRad(az), e = T.MathUtils.degToRad(el);
    const dir = new T.Vector3(Math.sin(a) * Math.cos(e), Math.sin(e), -Math.cos(a) * Math.cos(e));
    const t = new T.Vector3(...((tgt || [0, 0.6, 0]) as [number, number, number]));
    if (!dist && !tgt) { this.goTo(this.fitCentered(dir, t, 0.88), instant); return; }
    const d = dist || this.fitDistance(dir, t, 0.9);
    this.goTo({ pos: t.clone().addScaledVector(dir, d), tgt: t }, instant);
  }
  partsBox(ids: string[]) {
    const b = new T.Box3();
    ids.forEach((id) => { const p = this.parts[id]; if (p && p.visible) p.meshes.filter((m) => m.visible || !m.userData.lod).forEach((m) => { if (!m.userData.lod) b.expandByObject(m); }); });
    return b;
  }
  /** 把一组部件框进画面（保持当前观察方向，或给定方向） */
  focus(ids: string[], instant?: boolean, dir?: THREE.Vector3, fill = 0.82) {
    const b = this.partsBox(ids); if (b.isEmpty()) return false;
    const c = b.getCenter(new T.Vector3());
    const d0 = dir ? dir.clone().normalize() : this.camera.position.clone().sub(this.controls.target).normalize();
    const d = Math.max(0.6, this.fitDistance(d0, c, fill, b));
    this.goTo({ pos: c.clone().addScaledVector(d0, d), tgt: c }, instant);
    return true;
  }
  setView(pos: number[], tgt: number[]) { this.camera.position.set(pos[0], pos[1], pos[2]); this.controls.target.set(tgt[0], tgt[1], tgt[2]); this.controls.update(); this.camTween = null; this.syncFreeCam(); this.poke(); }
  getView() { return { pos: this.camera.position.toArray().map((v) => +v.toFixed(3)), tgt: this.controls.target.toArray().map((v) => +v.toFixed(3)) }; }
  camDistance() { return this.camera.position.distanceTo(this.controls.target); }
  /** 距离预设（LOD 检查）：沿当前方向把相机放到给定距离 */
  setDistance(d: number, instant = false) {
    const dir = this.camera.position.clone().sub(this.controls.target).normalize();
    const tgt = new T.Vector3(0, 0.6, 0);
    this.goTo({ pos: tgt.clone().addScaledVector(dir, d), tgt }, instant);
  }
  partScreen(id: string) {
    const p = this.parts[id]; if (!p) return null;
    const c = p.center.clone().applyMatrix4(this.shipFrame.matrixWorld).project(this.camera);
    return { x: (c.x + 1) / 2, y: (1 - c.y) / 2, z: c.z };
  }
  /** 船体坐标系中的点 → 屏幕（0..1） */
  project(v: THREE.Vector3) { const c = v.clone().project(this.camera); return { x: (c.x + 1) / 2, y: (1 - c.y) / 2, z: c.z }; }

  // ---------------------------------------------------------------- 模拟
  aimBearing: number | null = null; aimDistKm = 12; aimMarkDist = 40; pendingShots: { m: Mount; t: number }[] = [];
  setSimData(d: SimData) {
    this.simData = d; this.motion = new ShipMotion(d.ship);
    this.motion.m2u = (this.BOX.max.z - this.BOX.min.z) / (d.ship.size_m?.[0] || 250);
    if (this.ship) this.setupMounts();
  }
  m2u() { return this.motion ? this.motion.m2u : 0.067; }
  private setupMounts() {
    if (!this.simData || !this.ship) return;
    if (this.motion) this.motion.m2u = (this.BOX.max.z - this.BOX.min.z) / (this.simData.ship.size_m?.[0] || 250);
    this.mounts = []; let i = 0;
    for (const [id, spec] of Object.entries(this.simData.mounts)) {
      const p = this.parts[id] || Object.values(this.parts).find((q) => q.id.startsWith(id + '|')); if (!p) continue;
      if (spec.bones && spec.bones.includes('yaw')) { const m = makeMount(id, spec, p.top, i++); if (m.yawN || m.muzzles.length) { this.mounts.push(m); continue; } }
      // 没有骨骼的几何(盒子/CI 装配): 静态炮位 + 由网格前端估计炮口, 只用于开火特效预览(炮塔不转)
      const n = spec.numBarrels || spec.muzzles || 0; if (!n) continue;
      const m = makeMount(id, spec, p.top, i++); this.syntheticMuzzles(m, n); if (m.muzzles.length) this.mounts.push(m);
    }
  }
  /** 估计炮口: 炮位局部坐标里最靠前(-z)的一段顶点, 按 x 均分 n 管 */
  private syntheticMuzzles(m: Mount, n: number) {
    const top = m.top; top.updateMatrixWorld(true); const inv = top.matrixWorld.clone().invert(); const v = new T.Vector3(); const pts: THREE.Vector3[] = [];
    top.traverse((o: any) => { if (!o.isMesh || o.userData.lod) return; const a = o.geometry.attributes.position; const M = inv.clone().multiply(o.matrixWorld);
      const st = Math.max(1, Math.floor(a.count / 4000)); for (let k = 0; k < a.count; k += st) pts.push(v.fromBufferAttribute(a, k).applyMatrix4(M).clone()); });
    if (pts.length < 8) return;
    let zmin = Infinity, zmax = -Infinity; pts.forEach((q) => { zmin = Math.min(zmin, q.z); zmax = Math.max(zmax, q.z); });
    const tip = pts.filter((q) => q.z < zmin + 0.08 * (zmax - zmin)); if (!tip.length) return;
    let x0 = Infinity, x1 = -Infinity, y = 0; tip.forEach((q) => { x0 = Math.min(x0, q.x); x1 = Math.max(x1, q.x); y += q.y; }); y /= tip.length;
    for (let k = 0; k < n; k++) { const o = new T.Object3D(); o.position.set(n === 1 ? (x0 + x1) / 2 : x0 + (x1 - x0) * (k + 0.5) / n, y, zmin); o.userData.muzzle = k; o.userData.synthetic = true; top.add(o); m.muzzles.push(o); }
  }
  simEnable(on: boolean) {
    this.simOn = on && !!this.motion;
    if (!this.simOn) { this.mounts.forEach((m) => { m.yaw = m.pitch = 0; m.shotT = 99; poseMount(m, this.m2u()); }); this.shipFrame.position.set(0, 0, 0); this.shipFrame.rotation.set(0, 0, 0); this.particles.clear(); }
    if (!this.simOn && this.camMode === 'game') this.setCamMode('orbit');
    this.updateSimVis(); this.renderer.shadowMap.needsUpdate = true; this.poke();
  }
  simReset() {
    if (!this.simData) return;
    this.motion = new ShipMotion(this.simData.ship); this.motion.m2u = (this.BOX.max.z - this.BOX.min.z) / (this.simData.ship.size_m?.[0] || 250);
    this.wake.clear(); this.bowL.clear(); this.bowR.clear(); this.particles.clear(); this.rng = mulberry32(7); this.pendingShots = []; this.emitAcc = 0; this.funnelAcc = 0;
    this.mounts.forEach((m) => { m.yaw = m.pitch = 0; m.reload = 0; m.shotT = 99; });
    this.WU.uShipP.value.set(0, 0); this.WU.uShipPsi.value = 0;
  }
  private updateSimVis() {
    if (!this.foamMat) return;
    const on = this.simOn, water = this.env === 'water' && this.waterMode !== 'off';
    [this.wake.mesh, this.bowL.mesh, this.bowR.mesh].forEach((m) => { m.visible = on && water; });
    if (this.sideFoam) this.sideFoam.visible = on && water;
    (this.floor.material as any).uniforms.uLineA.value = on ? 1 : 0.0;
    this.blob.visible = this.env !== 'water' && !on; this.wlSheet.visible = this.wlOn && !water && !on && this.env !== 'water' || (this.wlOn && this.env === 'water' && this.waterMode === 'off' && !on);
    this.aimMarker.visible = on && this.combat && this.aimBearing !== null;
  }
  setTelegraph(t: Telegraph) { if (this.motion) this.motion.tel = t; this.poke(); }
  setRudder(r: number) { if (this.motion) this.motion.rudderCmd = Math.max(-1, Math.min(1, r)); this.poke(); }
  setWaves(w: number) { if (this.motion) this.motion.waves = w; }
  setCombat(on: boolean) { this.combat = on; this.updateSimVis(); this.poke(); }
  setReloadReal(on: boolean) { this.reloadReal = on; }
  setAimDistKm(km: number) { this.aimDistKm = km; }
  /** az：相对船首的方位（度，正 = 右舷）；distKm：用于估算抬炮角 */
  aimAt(az: number, distKm?: number) {
    if (!this.motion) return;
    this.aimBearing = this.motion.psi - az * Math.PI / 180;
    if (distKm) this.aimDistKm = distKm;
    this.updateSimVis(); this.poke();
  }
  clearAim() { this.aimBearing = null; this.updateSimVis(); }
  aimAz() { return this.motion && this.aimBearing !== null ? T.MathUtils.radToDeg(this.motion.psi - this.aimBearing) : null; }
  /** 屏幕坐标（0..1）→ 水平面上的目标方位 */
  aimFromScreen(nx: number, ny: number) {
    if (!this.motion) return;
    const rc = new T.Raycaster(); rc.setFromCamera(new T.Vector2(nx * 2 - 1, -(ny * 2 - 1)), this.camera);
    const pl = new T.Plane(new T.Vector3(0, 1, 0), -this.WL); const hit = new T.Vector3();
    let x: number, z: number;
    if (rc.ray.intersectPlane(pl, hit) && hit.distanceTo(this.camera.position) < 600) { x = hit.x; z = hit.z; this.aimMarkDist = Math.max(4, Math.min(160, Math.hypot(x, z))); }
    else { const d = rc.ray.direction; x = d.x * 1000; z = d.z * 1000; this.aimMarkDist = 120; }
    const az = Math.atan2(x, -z) * 180 / Math.PI;
    this.aimAt(az);
  }
  setCamMode(m: CamMode) {
    this.camMode = m; const c = this.controls;
    if (m === 'game') {
      c.enablePan = false; c.minPolarAngle = Math.PI * 0.36; c.maxPolarAngle = Math.PI * 0.485; c.minDistance = 9; c.maxDistance = 34;
      const az = this.aimAz() ?? 0; const a = az * Math.PI / 180;
      this.goTo({ pos: new T.Vector3(-Math.sin(a) * 19, 5.2, Math.cos(a) * 19), tgt: new T.Vector3(0, 1.6, 0) });
    } else {
      c.enablePan = true; c.minPolarAngle = 0; c.maxPolarAngle = Math.PI * 0.62; c.minDistance = 1.2; c.maxDistance = 260;
    }
    this.freeCamWorld = null; this.syncFreeCam(); this.poke();
  }
  private syncFreeCam() {
    if (this.camMode !== 'free' || !this.motion) { this.freeCamWorld = null; return; }
    const m = this.motion, p = this.camera.position, t = this.controls.target;
    const a = m.toWorld(p.x, p.z), b = m.toWorld(t.x, t.z);
    this.freeCamWorld = { pos: new T.Vector3(a.x, p.y, a.y), tgt: new T.Vector3(b.x, t.y, b.y) };
  }
  private applyFreeCam() {
    if (this.camMode !== 'free' || !this.motion || !this.freeCamWorld) return;
    const m = this.motion, W = this.freeCamWorld;
    const a = m.toLocal(W.pos.x, W.pos.z), b = m.toLocal(W.tgt.x, W.tgt.z);
    this.camera.position.set(a.x, W.pos.y, a.y); this.controls.target.set(b.x, W.tgt.y, b.y);
  }
  readyMounts(group: string) { return this.mounts.filter((m) => m.spec.group === group && m.muzzles.length && m.reload <= 0 && (!this.combat ? false : m.inSector && Math.abs(m.wantYaw - m.yaw) < 3)); }
  /** 开火：salvo = 该组所有就绪炮塔（按序号错开 40 ms）；single = 下一座就绪炮塔 */
  fire(group: 'main' | 'secondary', how: 'salvo' | 'single', force = false) {
    if (!this.simOn) return 0;
    let ready = force ? this.mounts.filter((m) => m.spec.group === group && m.muzzles.length) : this.readyMounts(group);
    if (!ready.length) return 0;
    if (how === 'single') { const k = (this.singleIdx[group] || 0) % ready.length; ready = [ready[k]]; this.singleIdx[group] = k + 1; }
    ready.forEach((m, i) => this.pendingShots.push({ m, t: how === 'salvo' ? i * 0.04 : 0 }));
    ready.forEach((m) => { m.reload = this.reloadReal ? (m.spec.shotDelay || 5) : Math.min(m.spec.shotDelay || 2, 2); });
    this.poke(); return ready.length;
  }
  private shoot(m: Mount) {
    const mo = this.motion!; m.shotT = 0; poseMount(m, mo.m2u); m.top.updateMatrixWorld(true);
    const D = m.spec.barrelDiameter || 0.15, k = mo.m2u, night = this.light === 'night', dusk = this.light === 'dusk';
    const flashS = Math.max(0.08, D * 22 * k), mean = new T.Vector3();
    const fx = this.fx;
    const smokeCol = fxTint(night ? new T.Color(0.12, 0.12, 0.13) : dusk ? new T.Color(0.42, 0.38, 0.36) : new T.Color(0.62, 0.61, 0.6), fx?.shotSmoke);
    const coreCol = fxTint(new T.Color(6.0, 4.6, 2.6), fx?.muzzle, 1), outerCol = fxTint(new T.Color(5.2, 2.5, 0.8), fx?.muzzle, 0.3);
    const shipV = new T.Vector3(-Math.sin(mo.psi) * mo.speedU(), 0, -Math.cos(mo.psi) * mo.speedU());
    for (const mz of m.muzzles) {
      const p = mz.getWorldPosition(new T.Vector3()); mean.add(p);
      const q = mz.getWorldQuaternion(new T.Quaternion()); const dir = new T.Vector3(0, 0, -1).applyQuaternion(q);
      const toW = (v: THREE.Vector3) => { const w = mo.toWorld(v.x, v.z); return new T.Vector3(w.x, v.y, w.y); };
      const dirW = (v: THREE.Vector3) => { const c = Math.cos(mo.psi), s = Math.sin(mo.psi); return new T.Vector3(c * v.x + s * v.z, v.y, -s * v.x + c * v.z); };
      const pw = toW(p), dw = dirW(dir);
      // 炮口焰：沿炮管方向一串亮点（加色，HDR，供 Bloom）
      for (let i = 0; i < 7; i++) {
        const off = (0.15 + i * 0.4) * flashS, core = i < 2;
        this.particles.spawn({ p: pw.clone().addScaledVector(dw, off), v: dw.clone().multiplyScalar(flashS * (3 + 2 * i)).add(shipV), life: 0.07 + 0.025 * i,
          s0: flashS * (1.3 - i * 0.13), s1: flashS * (1.7 - i * 0.16), kind: 1, col: core ? coreCol : outerCol, a: fxA(fx?.muzzle), seed: this.rng() });
      }
      // 硝烟
      const n = D > 0.2 ? 10 : 4;
      for (let i = 0; i < n; i++) {
        const r = () => this.rng() * 2 - 1;
        const v = dw.clone().multiplyScalar(flashS * (6 + 10 * this.rng())).add(new T.Vector3(r(), r() * 0.5 + 0.3, r()).multiplyScalar(flashS * 1.2));
        this.particles.spawn({ p: pw.clone().addScaledVector(dw, flashS * (0.3 + 1.2 * this.rng())), v, life: 3.5 + 3 * this.rng(),
          s0: flashS * 0.7, s1: flashS * (2.6 + 1.6 * this.rng()), kind: 0, col: smokeCol, a: (night ? 0.26 : 0.34) * fxA(fx?.shotSmoke), seed: this.rng() });
      }
    }
    mean.multiplyScalar(1 / Math.max(1, m.muzzles.length));
    this.flashLight.position.copy(mean); this.flashLight.intensity = D > 0.2 ? 20 : 6; this.flashLight.distance = D > 0.2 ? 4.5 : 2;
  }
  /** 烟囱烟: 从 HP_FX_Smoke_* 挂点持续冒(世界坐标, 船开走自然拖尾); 无挂点则不画(不猜位置) */
  private emitFunnel(dt: number) {
    const mo = this.motion!; const hps = this.fxNodes.filter((o) => /^HP_FX_Smoke/i.test(o.userData.fx));
    if (!hps.length || this.env !== 'water') return;
    this.funnelAcc += dt; const every = 0.06; if (this.funnelAcc < every) return; this.funnelAcc -= every;
    const k = mo.m2u, night = this.light === 'night', dusk = this.light === 'dusk', sp = Math.abs(mo.speedKn) / mo.maxKn;
    const base = night ? new T.Color(0.07, 0.075, 0.085) : dusk ? new T.Color(0.3, 0.28, 0.27) : new T.Color(0.46, 0.46, 0.47);
    const col = fxTint(base, this.fx?.funnel), a = (night ? 0.2 : 0.26) * (0.55 + 0.45 * sp) * fxA(this.fx?.funnel);
    const shipV = new T.Vector3(-Math.sin(mo.psi) * mo.speedU(), 0, -Math.cos(mo.psi) * mo.speedU());
    for (const h of hps) {
      const p = h.getWorldPosition(new T.Vector3()); const w = mo.toWorld(p.x, p.z); const r = () => this.rng() * 2 - 1;
      const v = new T.Vector3(r() * 0.6 + 1.2, 3.5 + this.rng() * 1.5, r() * 0.6).multiplyScalar(k).addScaledVector(shipV, 0.15);
      this.particles.spawn({ p: new T.Vector3(w.x + r() * 1.5 * k, p.y + 1.0 * k, w.y + r() * 1.5 * k), v, life: 5 + 2 * this.rng(),
        s0: 8 * k, s1: (22 + 10 * this.rng()) * k, kind: 0, col, a: a * 0.7, seed: this.rng() });
    }
  }
  /** 设特效主题(null = 原版) */
  setFx(t: FxTheme | null) { this.fx = t; this.applyEnv(); this.poke(); }
  /** 推进模拟 dt 秒（确定性） */
  stepSim(dt: number) {
    const mo = this.motion; if (!mo || !this.simOn) return;
    mo.step(dt);
    this.WU.uShipP.value.copy(mo.P); this.WU.uShipPsi.value = mo.psi;
    const D2R = Math.PI / 180;
    this.shipFrame.position.set(0, mo.heave, 0); this.shipFrame.rotation.set(mo.pitch * D2R, 0, mo.roll * D2R, 'XZY'); this.shipFrame.updateMatrixWorld(true);
    // 尾迹
    const sp = Math.abs(mo.speedKn) / mo.maxKn, F = this.footprint;
    this.emitAcc += dt;
    if (F.length && this.emitAcc >= 0.1) {
      this.emitAcc = 0;
      if (sp > 0.02) {
        const st = F[F.length - 1], bw = F[Math.min(F.length - 1, 3)];
        const w = mo.toWorld(0, st.z + 0.15); this.wake.push(w.x, w.y, 0, 0, Math.min(1, sp * 1.2));
        if (mo.speedKn > 0) for (const s of [-1, 1]) {
          const b = mo.toWorld(s * bw.w * 1.02, bw.z + 0.1);
          const c = Math.cos(mo.psi), sn = Math.sin(mo.psi), lx = s * 0.95, lz = 0.3, v = mo.speedU() * 0.32;
          (s < 0 ? this.bowL : this.bowR).push(b.x, b.y, (c * lx + sn * lz) * v, (-sn * lx + c * lz) * v, Math.min(1, sp * 1.1));
        }
      }
    }
    this.wake.step(dt); this.bowL.step(dt); this.bowR.step(dt);
    this.emitFunnel(dt);
    // 炮塔
    const tl = this.aimBearing !== null ? new T.Vector3() : null;
    if (tl) { const far = 1000; const wx = mo.P.x - Math.sin(this.aimBearing!) * far, wz = mo.P.y - Math.cos(this.aimBearing!) * far; const l = mo.toLocal(wx, wz); tl.set(l.x, this.WL, l.y); }
    for (const m of this.mounts) {
      let loc: THREE.Vector3 | null = null;
      if (tl) loc = m.top.worldToLocal(tl.clone());
      stepMount(m, dt, loc, this.aimDistKm * 1000, this.combat);
      poseMount(m, mo.m2u);
    }
    for (let i = this.pendingShots.length - 1; i >= 0; i--) { const s = this.pendingShots[i]; s.t -= dt; if (s.t <= 0) { this.pendingShots.splice(i, 1); this.shoot(s.m); } }
    this.particles.step(dt);
    this.flashLight.intensity *= Math.exp(-dt * 30);
    if (this.aimBearing !== null) { const az = mo.psi - this.aimBearing; this.aimMarker.position.set(Math.sin(az) * this.aimMarkDist, this.WL + 0.01, -Math.cos(az) * this.aimMarkDist); this.aimMarker.scale.setScalar(Math.max(1, this.aimMarkDist / 25)); }
  }
  private buildSimGeometry() {
    const mo = this.motion; if (!mo || !this.simOn) return;
    const F = this.footprint, sp = Math.abs(mo.speedKn) / mo.maxKn, y = this.WL + 0.003;
    if (F.length) {
      const st = F[F.length - 1], bw = F[Math.min(F.length - 1, 3)];
      this.wake.build(mo, new T.Vector2(0, st.z + 0.1), Math.min(1, sp * 1.2), y);
      this.bowL.build(mo, mo.speedKn > 0 ? new T.Vector2(-bw.w, bw.z) : null, sp, y);
      this.bowR.build(mo, mo.speedKn > 0 ? new T.Vector2(bw.w, bw.z) : null, sp, y);
    }
    if (this.sideFoam) (this.sideFoam.material as any).uniforms.uK.value = Math.min(1, sp * 1.15) * (this.env === 'water' ? 1 : 0.85);
    this.particles.build(mo);
  }
  simSnapshot(): SimSnapshot | null {
    const mo = this.motion; if (!mo) return null;
    const az = this.aimAz();
    return { speedKn: mo.speedKn, tel: mo.tel, rudder: mo.rudder, rudderCmd: mo.rudderCmd, heading: ((-mo.psi * 180 / Math.PI) % 360 + 360) % 360, heel: mo.heel, roll: mo.roll, pitch: mo.pitch,
      mounts: this.mounts.map((m) => ({ id: m.id, group: m.spec.group, yaw: m.yaw, pitch: m.pitch, wantYaw: m.wantYaw, aimed: m.aimed, inSector: m.inSector, reload: m.reload, reloadMax: this.reloadReal ? (m.spec.shotDelay || 5) : Math.min(m.spec.shotDelay || 2, 2) })),
      lod: this.lodState(), camDist: this.camDistance(), aim: az === null ? null : { az: ((az + 540) % 360) - 180, distM: this.aimDistKm * 1000 } };
  }
  /** 确定性预热：用固定步长推进 seconds 秒（无头出图时让尾迹/姿态进入稳态） */
  simWarm(seconds: number, dt = 1 / 30) { const n = Math.round(seconds / dt); for (let i = 0; i < n; i++) { this.stepSim(dt); this.time += dt; } this.U.uTime.value = this.time; }
  /** 直接设定炮塔姿态（无头出图） */
  snapTurrets() { for (const m of this.mounts) { m.yaw = m.wantYaw; m.pitch = m.wantPitch; poseMount(m, this.m2u()); } }

  // ---------------------------------------------------------------- 输入
  private bindPointer() {
    const el = this.renderer.domElement; let down: number[] | null = null;
    el.addEventListener('pointerdown', (e) => { down = [e.clientX, e.clientY]; this.poke(); });
    el.addEventListener('wheel', () => this.poke(), { passive: true });
    el.addEventListener('pointermove', (e) => {
      if (!this.simOn || !this.combat || this.camMode === 'game' || (e.buttons & 1) || !this.aimFollowMouse) return;
      const r = el.getBoundingClientRect(); this.aimFromScreen((e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height);
    });
    el.addEventListener('pointerup', (e) => {
      if (!down || Math.hypot(e.clientX - down[0], e.clientY - down[1]) > 4 || !this.ship) return;
      const r = el.getBoundingClientRect();
      if (this.simOn && this.combat && this.camMode !== 'game') { this.aimFromScreen((e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height); this.aimCb && this.aimCb(null); return; }
      this.pickCb && this.pickCb(this.pickAt(e.clientX, e.clientY));
    });
  }
  aimFollowMouse = false;
  onAim(cb: (p: THREE.Vector3 | null) => void) { this.aimCb = cb; }
  private pickAt(cx: number, cy: number) {
    const r = this.renderer.domElement.getBoundingClientRect();
    let nx = (cx - r.left) / r.width; const ny = (cy - r.top) / r.height;
    const cam = this.camera.clone();
    if (this.mode === 'side' && Object.keys(this.sets.A).length) { this.setCamFor(cam, this.camera.aspect / 2); nx = nx < 0.5 ? nx * 2 : nx * 2 - 1; }
    const rc = new T.Raycaster(); rc.setFromCamera(new T.Vector2(nx * 2 - 1, -(ny * 2 - 1)), cam);
    const hit = rc.intersectObjects(this.meshes.filter((m) => m.visible && m.userData.pickable), false)[0];
    return hit ? (hit.object.userData.part as string) : null;
  }

  // ---------------------------------------------------------------- 渲染
  private setSide(side: 'A' | 'B') {
    this.passIsA = side === 'A'; const set = this.sets[side]; if (!Object.keys(set).length) return;
    for (const m of this.meshes) { const mt = set[m.userData.mfm]; if (mt && m.material !== mt) m.material = mt; }
  }
  private drawScene(rt: THREE.WebGLRenderTarget, vp: number[], side: 'A' | 'B') {
    this.setSide(side);
    rt.viewport.set(vp[0], vp[1], vp[2], vp[3]); rt.scissor.set(vp[0], vp[1], vp[2], vp[3]); rt.scissorTest = true;
    this.renderer.setRenderTarget(rt); this.renderer.render(this.scene, this.camera);
    this.drawCalls += this.renderer.info.render.calls; this.tris += this.renderer.info.render.triangles;
  }
  private fullVP(rt: THREE.WebGLRenderTarget) { rt.viewport.set(0, 0, rt.width, rt.height); rt.scissor.set(0, 0, rt.width, rt.height); rt.scissorTest = false; }
  private bloomOn() { return this.Q.bloom && (this.light === 'night' || this.channel === 3 || this.hasEmissive() || (this.simOn && this.flashLight.intensity > 0.5) || this.particles.n > 0); }

  private loop(now: number) {
    if (this.lost || this.paused) return;
    const animating = this.bench || !!this.camTween || this.flashT > 0 || this.env === 'water' || this.simOn || this.hasAnimatedEmission() || this.animNodes.length > 0;
    const t0 = performance.now();
    if (!animating && this.dirty <= 0 && t0 - this.lastInput > 600) { this.idle = true; this.clock.getDelta(); return; }
    this.idle = false;
    const dt = Math.min(this.clock.getDelta(), 0.1);
    this.advance(dt); this.frame();
    this.cpuMs = this.cpuMs * 0.9 + (performance.now() - t0) * 0.1;
    this.frameTimes.push(now); while (this.frameTimes.length && now - this.frameTimes[0] > 1000) this.frameTimes.shift();
    this.fps = this.frameTimes.length > 1 ? (this.frameTimes.length - 1) * 1000 / (now - this.frameTimes[0]) : 0;
    if (this.dirty > 0) this.dirty--;
  }
  animNodes: THREE.Object3D[] = [];
  private tickAnim() {
    for (const n of this.animNodes as any[]) {
      const a = n.userData.anim, F = a.frames, L = a.span || a.duration || 1;
      const x = (this.time % L) / (a.step || L), k = Math.min(F.length - 1, Math.floor(x)), k2 = Math.min(F.length - 1, k + 1), w = Math.min(1, x - k);
      const A = n.userData._am || (n.userData._am = [new T.Matrix4(), new T.Vector3(), new T.Quaternion(), new T.Vector3()]);
      n.matrix.fromArray(F[k].m).decompose(n.position, n.quaternion, n.scale);
      A[0].fromArray(F[k2].m).decompose(A[1], A[2], A[3]);
      n.position.lerp(A[1], w); n.quaternion.slerp(A[2], w); n.scale.lerp(A[3], w); n.visible = F[k].v !== false;
    }
  }
  hasAnimatedEmission() {
    if (this.emisOverride > 0) return this.hasEmissive();
    return Object.values(this.sets.B).some((m: any) => m && m.userData.family === 'emissive' && m.userData.sig && /emissionAnimationMode":[1-9]/.test(m.userData.sig));
  }
  /** 推进时间（相机补间、模拟、动画） */
  advance(dt: number) {
    this.time += dt; this.U.uTime.value = this.time;
    this.tickAnim();
    if (this.camTween) {
      const tw = this.camTween; tw.t = Math.min(1, tw.t + dt / 0.9); const k = 1 - Math.pow(1 - tw.t, 3);
      this.camera.position.lerpVectors(tw.from.pos, tw.to.pos, k); this.controls.target.lerpVectors(tw.from.tgt, tw.to.tgt, k);
      if (tw.t >= 1) { this.camTween = null; this.syncFreeCam(); }
    }
    if (this.simOn) { this.stepSim(dt); if (!this.camTween) this.applyFreeCam(); }
    if (this.bench && !this.simOn && !this.camTween) { // ?bench：匀速绕船旋转（12°/s），用于测帧率
      const tg = this.controls.target, c = this.camera.position, a = dt * 12 * Math.PI / 180;
      const x = c.x - tg.x, z = c.z - tg.z; c.x = tg.x + x * Math.cos(a) - z * Math.sin(a); c.z = tg.z + x * Math.sin(a) + z * Math.cos(a);
    }
    this.controls.update();
    if (this.simOn && !this.camTween) this.syncFreeCam();
    if (this.simOn && this.camMode === 'game') { this.aimFromScreen(0.5, 0.47); }
    const cx = this.camera.position.x, cz = this.camera.position.z, l = Math.hypot(cx, cz) || 1;
    this.U.uCutDir.value.set(cx / l, cz / l); this.cutWall.rotation.y = Math.atan2(cx / l, cz / l);
    if (this.flashT > 0) this.flashT = Math.max(0, this.flashT - dt * 0.8);
  }
  /** 画一帧（不推进时间） */
  frame() {
    if (this.lodMode === 'auto') this.applyLod();
    this.buildSimGeometry();
    if (this.simOn && this.Q.simShadowEvery && (++this.shadowTick % this.Q.simShadowEvery === 0)) this.renderer.shadowMap.needsUpdate = true;
    const [w, h] = this.size(); const asp = w / Math.max(1, h); this.baseAspect = asp;
    const W = this.rtB.width, H = this.rtB.height; this.drawCalls = 0; this.tris = 0;
    const doBloom = this.bloomOn(); const m = Object.keys(this.sets.A).length ? this.mode : 'single';
    const r = this.renderer;
    if (m === 'single') { this.setCamFor(this.camera, asp); this.drawScene(this.rtB, [0, 0, W, H], 'B'); }
    else if (m === 'split') {
      this.setCamFor(this.camera, asp); const sx = Math.round(W * this.split);
      const clip = (x0: number, x1: number, side: 'A' | 'B') => {
        if (x1 <= x0) return; this.setSide(side);
        this.rtB.viewport.set(0, 0, W, H); this.rtB.scissor.set(x0, 0, x1 - x0, H); this.rtB.scissorTest = true;
        r.setRenderTarget(this.rtB); r.render(this.scene, this.camera); this.drawCalls += r.info.render.calls; this.tris += r.info.render.triangles;
      };
      clip(0, sx, 'A'); clip(sx, W, 'B');
    } else if (m === 'side') {
      this.setCamFor(this.camera, asp / 2); const hw = Math.floor(W / 2);
      this.drawScene(this.rtB, [0, 0, hw, H], 'A'); this.drawScene(this.rtB, [hw, 0, W - hw, H], 'B');
    } else {
      this.setCamFor(this.camera, asp); this.drawScene(this.rtA, [0, 0, W, H], 'A'); this.drawScene(this.rtB, [0, 0, W, H], 'B');
      this.fullVP(this.rtA); if (doBloom) this.bloom.render(r, null as any, this.rtA, 0, false);
    }
    r.shadowMap.needsUpdate = false;
    this.fullVP(this.rtB); if (doBloom) this.bloom.render(r, null as any, this.rtB, 0, false);
    this.setSide('B'); this.passIsA = false; this.setCamFor(this.camera, asp);
    r.setRenderTarget(null); r.render(this.compScene, this.compCam);
    this.frameCb && this.frameCb();
  }
  renderOnce(dt = 0) { const t0 = performance.now(); this.advance(dt); this.frame(); return performance.now() - t0; }
  snapshot(type = 'image/png') { this.frame(); return this.renderer.domElement.toDataURL(type); }
  canvas() { return this.renderer.domElement; }
  setDither(on: boolean) { this.compMat.uniforms.uDither.value = on ? 1 : 0; }
  dispose() { this.renderer.setAnimationLoop(null); this.renderer.dispose(); this.el.removeChild(this.renderer.domElement); }
}
