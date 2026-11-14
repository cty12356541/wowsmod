/* 应用状态（zustand）。3D 视口实例是模块级单例（不进 React 状态），动作里直接调用。 */
import { create } from 'zustand';
import { api, ApiError, on as apiOn, startLive, disposeRequests, projectScope, type ConnState } from './lib/api';
import type { Project, Step, Part, TexInfo, Manifest, SimData, Job, ApplyPlan, Capability } from './lib/types';
import { ProjectCache } from './lib/scope';
import type { Viewer, Mode, WaterMode, Quality, CamMode, SimSnapshot } from './engine/viewer';
import type { EnvName, LightName, LookName } from './engine/env';
import type { Telegraph } from './engine/sim';
import { setLang, t, type Lang } from './lib/i18n';

export let viewer: Viewer | null = null;
export const setViewer = (v: Viewer | null) => { viewer = v; };

export type Workspace = 'review' | 'sim';
export type BaseSel = 'base' | 'prev';
export interface Toast { id: number; kind: 'info' | 'error' | 'ok'; title: string; desc?: string; ms?: number }

export interface State {
  lang: Lang; workspace: Workspace;
  conn: ConnState; offlineSince: number | null;
  phase: 'boot' | 'loading' | 'ready' | 'fatal'; fatal: { code: string; message: string } | null;
  loadMsg: string; loadFrac: number | null; texProg: { d: number; n: number } | null;
  compareStatus: 'idle' | 'loading' | 'ready' | 'error'; compareError: string; compareProg: { d: number; n: number } | null;
  project: Project | null; steps: Step[]; head: string | null; viewRev: string | null; baseSel: BaseSel;
  parts: Part[]; textures: TexInfo[]; sim: SimData | null;
  selected: string | null; hidden: Record<string, boolean>; onlyChanged: boolean; filter: string;
  selTex: string | null; texCh: string;
  leftTab: 'outline' | 'textures'; rightTab: 'history' | 'inspect' | 'sim';
  mode: Mode; channel: number; look: LookName; env: EnvName; light: LightName; waterMode: WaterMode; waterline: boolean; highlight: boolean; camo: boolean; glow: number; overlay: number; quality: Quality;
  // 模拟
  simOn: boolean; tel: Telegraph; rudder: number; camMode: CamMode; combat: boolean; aimMouse: boolean; aimKm: number; reloadReal: boolean; waves: number;
  emisMode: number; emisSpeed: number; lodMode: 'auto' | number; lodBias: number; lodTint: boolean; distPreset: 'near' | 'mid' | 'far' | null;
  snap: SimSnapshot | null;
  stats: { fps: number; draws: number; tris: number; idle: boolean; gl: string; gpuTexMB: number } | null;
  job: Job | null; toasts: Toast[]; dialog: null | { kind: 'uninstall' | 'gameRunning' | 'apply' };
  applyPlan: ApplyPlan | null; applyBusy: boolean; applyError: string; applyMessage: string;
  catalogEpoch: number;
}

const initialLang = ((): Lang => {
  const q = new URLSearchParams(location.search).get('lang'); const s = q || localStorage.getItem('wm.lang') || navigator.language || 'zh';
  return s.startsWith('ru') ? 'ru' : s.startsWith('en') ? 'en' : 'zh';
})();
setLang(initialLang);
/** 画质：URL ?q= > 用户上次选择 > 按显卡自动（软件渲染 → 低，集显 → 中，其它 → 高） */
function autoQuality(): Quality {
  try {
    const gl = document.createElement('canvas').getContext('webgl2'); if (!gl) return 'low';
    const ext = gl.getExtension('WEBGL_debug_renderer_info');
    const name = String(ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER));
    gl.getExtension('WEBGL_lose_context')?.loseContext();
    if (/swiftshader|llvmpipe|software|basic render/i.test(name)) return 'low';
    if (/intel|radeon\(tm\) graphics|vega \d+ graphics|apple m\d|mali|adreno/i.test(name) && !/arc/i.test(name)) return 'mid';
    return 'high';
  } catch { return 'mid'; }
}
const isQ = (x: string | null): x is Quality => x === 'high' || x === 'mid' || x === 'low';
const savedQ: Quality = ((): Quality => {
  const u = new URLSearchParams(location.search).get('q'); if (isQ(u)) return u;
  const l = localStorage.getItem('wm.quality'); if (isQ(l)) return l;
  return autoQuality();
})();

/** 外观: URL ?look= > 上次选择 > 游戏内(默认) */
const savedLook: LookName = ((): LookName => {
  const u = new URLSearchParams(location.search).get('look') || localStorage.getItem('wm.look');
  return u === 'studio' ? 'studio' : 'game';
})();

export const useStore = create<State>(() => ({
  lang: initialLang, workspace: 'review', conn: 'connecting', offlineSince: null,
  phase: 'boot', fatal: null, loadMsg: '', loadFrac: null, texProg: null,
  compareStatus: 'idle', compareError: '', compareProg: null,
  project: null, steps: [], head: null, viewRev: null, baseSel: 'base', parts: [], textures: [], sim: null,
  selected: null, hidden: {}, onlyChanged: false, filter: '', selTex: null, texCh: 'rgb',
  leftTab: 'outline', rightTab: 'history',
  mode: 'single', channel: 0, look: savedLook, env: 'studio', light: 'day', waterMode: 'full', waterline: true, highlight: true, camo: true, glow: 1, overlay: 0.5, quality: savedQ,
  simOn: false, tel: 'stop', rudder: 0, camMode: 'orbit', combat: false, aimMouse: true, aimKm: 12, reloadReal: false, waves: 1,
  emisMode: -1, emisSpeed: 1, lodMode: 'auto', lodBias: 1, lodTint: false, distPreset: null, snap: null,
  stats: null, job: null, toasts: [], dialog: null,
  applyPlan: null, applyBusy: false, applyError: '', applyMessage: '',
  catalogEpoch: 0,
}));
const set = useStore.setState, get = useStore.getState;
let active = false;
const timers = new Set<ReturnType<typeof setTimeout>>();
function later(fn: () => void, ms: number) { const timer = setTimeout(() => { timers.delete(timer); if (active) fn(); }, ms); timers.add(timer); }
export const capabilityReason = (capability: Capability) => get().project?.capabilities?.reasons?.[capability] || t('libCapabilityUnavailable');
export const supports = (capability: Capability) => get().project?.capabilities?.[capability] !== false;

// ---------------------------------------------------------------- 提示
let toastId = 0;
export function toast(kind: Toast['kind'], title: string, desc?: string, ms = 4200) {
  const id = ++toastId; set({ toasts: [...get().toasts, { id, kind, title, desc, ms }] });
  if (ms > 0) later(() => set({ toasts: get().toasts.filter((x) => x.id !== id) }), ms);
}
export const dismissToast = (id: number) => set({ toasts: get().toasts.filter((x) => x.id !== id) });

// ---------------------------------------------------------------- 版本与对比
export function baseRev(s = get()): string {
  if (s.baseSel === 'base') return 'base';
  const act = s.steps.filter((x) => x.state === 'active'); const cur = s.viewRev || s.head;
  const i = act.findIndex((x) => x.id === cur);
  return i > 0 ? act[i - 1].id : 'base';
}
export function changedPartIds() { return get().parts.filter((p) => p.changed).map((p) => p.id); }

const manifestCache = new ProjectCache<Manifest>();
let manifestEpoch = 0;
function clearManifests() { manifestEpoch++; manifestCache.clear(); }
async function manifest(rev: string) { const epoch = manifestEpoch; const hit = manifestCache.get(projectScope, rev); if (hit) return hit; const value = await api.model(rev); if (epoch === manifestEpoch) manifestCache.set(projectScope, rev, value); return value; }
const geometryKey = (m: Manifest) => JSON.stringify([projectScope, m.geometry.hash, m.geometry.url]);
let loadedGeometry: { target: Viewer; key: string } | null = null;
const simulationKey = (project: Project) => JSON.stringify([project.ship.param || project.ship.code, project.capabilities?.simulation]);
let loadedSimulationKey: string | null = null;
let simulationEpoch = 0, attemptedSimulationEpoch = -1;

function clearSimulation() {
  viewer?.setSimData(null);
  set({ sim: null, simOn: false, workspace: 'review', rightTab: 'history', snap: null, tel: 'stop', rudder: 0, combat: false, camMode: get().camMode === 'game' ? 'orbit' : get().camMode });
}

function applySimulationSettings(target: Viewer, s = get()) {
  target.setWaves(s.waves); target.setCombat(s.combat);
  target.setTelegraph(s.tel); target.setRudder(s.rudder); target.setReloadReal(s.reloadReal);
  target.setAimDistKm(s.aimKm); target.aimFollowMouse = s.aimMouse;
  if (s.simOn) {
    target.setCamMode(s.camMode);
    const lodMode = target.setLodMode(s.lodMode) ?? s.lodMode;
    target.setLodBias(s.lodBias); target.setLodTint(s.lodTint);
    target.setEmisOverride(s.emisMode, s.emisSpeed);
    set({ lodMode });
  }
}

function syncGeometryLod(target: Viewer) {
  const s = get();
  const lodMode = target.resolveLodMode?.(s.lodMode) ?? s.lodMode;
  if (lodMode !== s.lodMode) set({ lodMode });
}

async function loadSimulation(target: Viewer) {
  const epoch = simulationEpoch, boot = bootSeq;
  if (!active || viewer !== target || attemptedSimulationEpoch === epoch) return;
  // Stamp only an actual request, so an overlapping refresh can still take
  // ownership if the first refresh was superseded while loading materials.
  attemptedSimulationEpoch = epoch;
  try {
    const sim: SimData = await api.sim();
    if (!active || epoch !== simulationEpoch || boot !== bootSeq || viewer !== target) return;
    target.setSimData(sim);
    if (get().simOn) target.simEnable(true);
    applySimulationSettings(target);
    set({ sim });
  } catch { /* Missing simulation is represented as unavailable. */ }
}

let loadSeq = 0;
interface RevisionLoad { seq: number; target: Viewer; base: Manifest; failedB: string[]; ready: boolean; pending?: Promise<void>; stopWaiting?: () => void }
interface RevisionTask { result: Promise<void>; superseded: Promise<void>; supersede: () => void }
let revisionTask: RevisionTask | null = null;
let loadedRevision: RevisionLoad | null = null;
const comparisonWanted = () => get().mode !== 'single' && get().workspace === 'review';
const currentRevision = (load: RevisionLoad) => active && load.seq === loadSeq && load.target === viewer && loadedRevision === load;
function displayMode(target: Viewer) {
  const mode = comparisonWanted() && get().compareStatus === 'ready' ? get().mode : 'single';
  target.setMode?.(mode);
}
function invalidateRevision() {
  revisionTask?.supersede(); revisionTask = null;
  loadSeq++; loadedRevision?.stopWaiting?.(); loadedRevision = null;
  set({ compareStatus: comparisonWanted() ? 'loading' : 'idle', compareError: '', compareProg: null, texProg: null });
  if (viewer) { displayMode(viewer); viewer.clearSide?.('A'); }
}
async function loadComparison(load = loadedRevision): Promise<void> {
  if (!load || !currentRevision(load)) return;
  if (load.failedB.length) {
    set({ compareStatus: 'error', compareError: t('texFailTitle', { n: load.failedB.length }) + ': ' + load.failedB.slice(0, 4).join(', '), compareProg: null });
    displayMode(load.target); return;
  }
  if (load.ready) { set({ compareStatus: 'ready' }); displayMode(load.target); return; }
  if (load.pending) return load.pending;
  const isCurrent = () => currentRevision(load);
  set({ compareStatus: 'loading', compareError: '', compareProg: null });
  load.pending = (async () => {
    try {
      const result = await load.target.applyManifest('A', load.base, (d, n) => {
        if (isCurrent()) set({ compareProg: n && d < n ? { d, n } : null });
      }, isCurrent);
      if (!isCurrent()) return;
      if (result.failed.length) throw new Error(t('texFailTitle', { n: result.failed.length }) + ': ' + result.failed.slice(0, 4).join(', '));
      load.ready = true;
      set({ compareStatus: 'ready', compareError: '', compareProg: null });
      displayMode(load.target); load.target.gc();
    } catch (error) {
      if (!isCurrent()) return;
      set({ compareStatus: 'error', compareError: displayError(error, t('opFailed')), compareProg: null });
      displayMode(load.target);
    } finally { load.pending = undefined; }
  })();
  return load.pending;
}
/** 完整载入 B；基准清单用于高亮，A 的贴图仅在实际请求对比时载入。 */
export function loadRevs(): Promise<void> {
  // Even an unavailable snapshot supersedes an outstanding geometry download.
  invalidateRevision();
  let supersede!: () => void;
  const superseded = new Promise<void>((resolve) => { supersede = resolve; });
  const task: RevisionTask = { result: loadRevision(loadSeq), superseded, supersede };
  revisionTask = task;
  return waitForLatestRevision(task);
}
async function waitForLatestRevision(task: RevisionTask): Promise<void> {
  for (;;) {
    try { await Promise.race([task.result, task.superseded]); }
    catch (error) { if (task === revisionTask) throw error; }
    // A replaced load is not a completed preview. Boot/recovery callers follow
    // the latest task even when history, base or quality actions initiated it.
    if (!active || !revisionTask || task === revisionTask) return;
    task = revisionTask;
  }
}
async function loadRevision(seq: number) {
  const s = get(); const rev = s.viewRev || s.head; if (!rev || !viewer || s.project?.capabilities?.preview === false) return;
  const target = viewer;
  try {
  const base = baseRev();
  const [mB, mA, parts, tex] = await Promise.all([manifest(rev), manifest(base), api.parts(rev, base), api.textures(rev, base)]);
  if (seq !== loadSeq || target !== viewer || !active) return;
  const key = geometryKey(mB);
  if (loadedGeometry?.target !== target || loadedGeometry.key !== key) {
    const geometry = await api.geometry(mB.geometry.url);
    if (seq !== loadSeq || target !== viewer || !active) return;
    target.loadGeometry(geometry, { preserveCamera: loadedGeometry?.target === target });
    syncGeometryLod(target);
    loadedGeometry = { target, key };
    Object.entries(get().hidden).forEach(([id, hidden]) => target.setPartVisible(id, !hidden));
  }
  const current = get(); const ids = new Set<string>(parts.parts.map((part: Part) => part.id));
  set({ parts: parts.parts, textures: tex.textures, selected: current.selected && ids.has(current.selected) ? current.selected : null, hidden: Object.fromEntries(Object.entries(current.hidden).filter(([id]) => ids.has(id))) });
  const isCurrent = () => active && seq === loadSeq && target === viewer;
  const prog = (d: number, n: number) => { if (isCurrent()) set({ texProg: n && d < n ? { d, n } : null }); };
  target.setBaselineManifest?.(mA);
  const rB = await target.applyManifest('B', mB, prog, isCurrent);
  if (!isCurrent()) return;
  const load: RevisionLoad = { seq, target, base: mA, failedB: rB.failed, ready: false };
  loadedRevision = load;
  set({ texProg: null });
  target.gc();
  const failed = rB.failed.length;
  if (failed) toast('error', t('texFailTitle', { n: failed }), rB.failed.slice(0, 4).join(', '));
  if (comparisonWanted()) {
    // Switching back to single can publish the already complete B immediately.
    const noLongerNeeded = new Promise<void>((resolve) => { load.stopWaiting = resolve; });
    await Promise.race([loadComparison(load), noLongerNeeded]);
    load.stopWaiting = undefined;
  }
  } catch (error) {
    if (!active || seq !== loadSeq || target !== viewer) return;
    set({ texProg: null, compareStatus: 'error', compareError: displayError(error, t('opFailed')), compareProg: null });
    throw error;
  }
}

// ---------------------------------------------------------------- 启动
let bootSeq = 0;
let viewerFactory: (() => Viewer) | null = null;
let previewRecoveryPending = false;
function completePreview(target: Viewer) {
  const load = loadedRevision;
  if (!load || !currentRevision(load) || load.target !== target) return false;
  if (load.failedB.length) throw new ApiError('texture_failed', t('texFailTitle', { n: load.failedB.length }) + ': ' + load.failedB.slice(0, 4).join(', '), 0);
  previewRecoveryPending = false;
  set({ phase: 'ready', fatal: null });
  return true;
}
export async function boot(factory?: () => Viewer) {
  if (factory) viewerFactory = factory;
  active = true; previewRecoveryPending = false; const seq = ++bootSeq;
  set({ phase: 'loading', loadMsg: t('ldConnect'), fatal: null });
  invalidateRevision(); clearManifests();
  try {
    await api.health();
    if (!active || seq !== bootSeq) return;
    set({ loadMsg: t('ldProject') });
    const [project, steps] = await Promise.all([api.project(), api.steps()]);
    if (!active || seq !== bootSeq) return;
    const s = get();
    const nextSimulationKey = simulationKey(project);
    if ((loadedSimulationKey !== nextSimulationKey || project.capabilities?.simulation === false) && (s.sim || s.simOn || s.workspace === 'sim')) clearSimulation();
    loadedSimulationKey = nextSimulationKey;
    simulationEpoch++;
    set({ project, steps: steps.steps, head: steps.head, viewRev: steps.head });
    document.title = t('docTitle', { p: project.project.name });
    if (project.capabilities?.preview === false) { set({ phase: 'ready' }); startLive(); return; }
    if (!steps.head) throw new ApiError('no_model', '', 404);
    if (!viewer) { const v = viewerFactory?.(); if (v) setViewer(v); }
    if (!viewer) throw new ApiError('no_viewer', t('libPreviewUnavailable'), 0);
    const m = await api.model(steps.head); if (!active || seq !== bootSeq) return; manifestCache.set(projectScope, steps.head, m);
    set({ loadMsg: t('ldModel'), loadFrac: 0 });
    const buf = await api.geometry(m.geometry.url, (f) => set({ loadFrac: f }));
    if (!active || seq !== bootSeq) return;
    set({ loadMsg: t('ldParse'), loadFrac: null });
    await new Promise((r) => setTimeout(r, 0));
    if (!active || seq !== bootSeq) return;
    const target = viewer!;
    target.loadGeometry(buf);
    syncGeometryLod(target);
    loadedGeometry = { target, key: geometryKey(m) };
    target.setFx(project.fx?.preview ?? null);
    await loadRevs();
    if (!active || seq !== bootSeq || viewer !== target) return;
    if (!completePreview(target)) return;
    startLive();
    if (get().project?.capabilities?.simulation !== false) void loadSimulation(target);
  } catch (e: any) {
    if (!active || seq !== bootSeq) return;
    previewRecoveryPending = true;
    const code = e && e.code ? e.code : 'internal';
    set({ phase: 'fatal', fatal: { code, message: e && e.message ? e.message : String(e) } });
    if (code === 'network' || code === 'timeout' || code === 'backend_unavailable') startLive();
  }
}
export function disposeWorkbench() {
  active = false; bootSeq++; loadSeq++; syncSeq++; viewerFactory = null; previewRecoveryPending = false; disposeRequests();
  revisionTask?.supersede(); revisionTask = null;
  loadedRevision?.stopWaiting?.(); loadedRevision = null;
  timers.forEach(clearTimeout); timers.clear(); clearManifests(); loadedGeometry = null; loadedSimulationKey = null; simulationEpoch++; attemptedSimulationEpoch = -1; setViewer(null);
}

apiOn('connection', (c: ConnState) => {
  if (!active) return;
  const prev = get().conn; set({ conn: c, offlineSince: c === 'offline' ? get().offlineSince || Date.now() : null });
  if (c === 'online' && prev === 'offline') {
    if (get().phase === 'fatal') boot(); else { resync(); toast('ok', t('reconnected'), t('reconnectedDesc')); }
  }
});
apiOn('event', async (ev: { type: string; data: any }) => {
  if (!active) return;
  if (ev.type === 'steps' || ev.type === 'resync' || ev.type === 'project') await resync(ev.data);
  else if (ev.type === 'game' || ev.type === 'activity') { try { const project = await api.project(); if (active) { set({ project }); document.title = t('docTitle', { p: project.project.name }); } } catch { /* */ } }
  else if (ev.type === 'job') set({ job: ev.data.job });
});

let syncSeq = 0;
export async function resync(d?: any) {
  const seq = ++syncSeq;
  let initializingPreview = false;
  try {
    const [project, steps] = await Promise.all([api.project(), api.steps()]);
    const s = get(); const wasAtHead = !s.viewRev || s.viewRev === s.head;
    const exists = steps.steps.some((x: Step) => x.id === s.viewRev);
    if (!active || seq !== syncSeq) return;
    const nextSimulationKey = simulationKey(project);
    const refreshSimulation = loadedSimulationKey !== nextSimulationKey;
    loadedSimulationKey = nextSimulationKey;
    if (refreshSimulation) simulationEpoch++;
    if (refreshSimulation || (project.capabilities?.simulation === false && (s.sim || s.simOn || s.workspace === 'sim'))) clearSimulation();
    clearManifests();
    set({ project, steps: steps.steps, head: steps.head, viewRev: wasAtHead || !exists ? steps.head : s.viewRev });
    document.title = t('docTitle', { p: project.project.name });
    if (project.capabilities?.preview !== false && (previewRecoveryPending || !viewer || loadedGeometry?.target !== viewer)) {
      // Recovery includes materials: a newer resync must finish it even when
      // an earlier resync has already committed geometry or failed afterward.
      previewRecoveryPending = true;
      initializingPreview = true;
      set({ phase: 'loading', loadMsg: t('ldModel'), loadFrac: null, fatal: null });
      if (!steps.head) throw new ApiError('no_model', '', 404);
      if (!viewer) { const created = viewerFactory?.(); if (created) setViewer(created); }
      if (!viewer) throw new ApiError('no_viewer', t('libPreviewUnavailable'), 0);
    }
    const target = viewer;
    target?.setFx(project.fx?.preview ?? null);
    await loadRevs();
    if (!active || seq !== syncSeq || viewer !== target) return;
    if (project.capabilities?.preview === false) { previewRecoveryPending = false; set({ phase: 'ready', fatal: null }); }
    else if (initializingPreview && target && !completePreview(target)) return;
    if (get().phase === 'ready' && project.capabilities?.simulation === true && target) {
      await loadSimulation(target);
      if (!active || seq !== syncSeq || viewer !== target) return;
    }
    if (d && d.reason === 'new_step' && wasAtHead) {
      const st = steps.steps.find((x: Step) => x.id === steps.head);
      if (st) { toast('info', t('newStep', { n: st.index }), st.title); viewer?.flash(st.parts.slice(0, 12)); }
    }
  } catch (e: any) {
    if (initializingPreview && active && seq === syncSeq) set({ phase: 'fatal', fatal: { code: e?.code || 'internal', message: displayError(e, t('libPreviewUnavailable')) } });
    // Ordinary live refresh disconnections remain owned by connection state.
  }
}

// ---------------------------------------------------------------- 动作
function displayError(value: unknown, fallback: string): string {
  if (typeof value === 'string') return value || fallback;
  if (value && typeof value === 'object' && 'message' in value) return displayError(value.message, fallback);
  return fallback;
}
async function trackResult(response: any, kind: 'apply' | 'uninstall') {
  const id = response.job_id || response.job?.id;
  async function finish(job?: Job) {
    if (!active) return;
    if (job?.state === 'failed') {
      const message = displayError(job.error, displayError(job.message, t('jobFailed')));
      set({ applyMessage: message }); toast('error', t('jobFailed'), message); return;
    }
    const project = await api.project(); if (!active) return;
    const message = kind === 'apply' ? t('libApplyDone') : t('uninstalledTitle');
    set({ project, catalogEpoch: get().catalogEpoch + 1, applyMessage: message }); toast('ok', message, job?.message);
  }
  if (!id) { await finish(); return; }
  set({ job: { id, kind, state: 'running', progress: 0, message: response.message || t('libWorking') }, applyMessage: response.message || t('libWorking') });
  const poll = async () => {
    try {
      const result = await api.job(id); if (!active) return;
      const job: Job = { ...result, id, kind }; set({ job, applyMessage: job.message || t('libWorking') });
      if (job.state === 'running') later(poll, 500); else await finish(job);
    } catch (e: any) {
      if (!active) return;
      set({ job: { id, kind, state: 'failed', progress: 0, message: e.message }, applyMessage: e.message });
      toast('error', t('jobFailed'), e.message);
    }
  };
  later(poll, 300);
}
export const actions = {
  setLang(l: Lang) { setLang(l); localStorage.setItem('wm.lang', l); set({ lang: l }); const p = get().project; if (p) document.title = t('docTitle', { p: p.project.name }); },
  async view(rev: string) { set({ viewRev: rev }); await loadRevs(); const st = get().steps.find((x) => x.id === rev); if (st && st.parts.length && st.parts.length <= 12) viewer?.flash(st.parts); },
  async setBase(b: BaseSel) { set({ baseSel: b }); await loadRevs(); },
  async setMode(m: Mode) {
    const mode = m === 'overlay' && viewer?.Q?.overlay === false ? 'split' : m;
    const previous = get().mode;
    set({ mode });
    if (viewer) displayMode(viewer);
    if ((mode === 'side') !== (previous === 'side')) viewer?.preset('hero');
    if (comparisonWanted()) await loadComparison(); else loadedRevision?.stopWaiting?.();
  },
  async retryComparison() {
    if (!comparisonWanted()) return;
    if (!loadedRevision || loadedRevision.failedB.length) {
      const recovering = previewRecoveryPending, target = viewer, boot = bootSeq;
      if (recovering) set({ phase: 'loading', fatal: null, loadMsg: t('ldModel'), loadFrac: null });
      try {
        await loadRevs();
        if (recovering && active && boot === bootSeq && target && viewer === target && completePreview(target)) {
          startLive();
          if (get().project?.capabilities?.simulation !== false) void loadSimulation(target);
        }
      } catch (error: any) {
        if (recovering && active && boot === bootSeq && target === viewer) set({ phase: 'fatal', fatal: { code: error?.code || 'internal', message: displayError(error, t('libPreviewUnavailable')) } });
        // loadRevs keeps the current comparison error visible for retry.
      }
    } else await loadComparison();
  },
  setChannel(c: number) { viewer?.setChannel(c); set({ channel: c }); },
  setEnv(e: EnvName) { viewer?.setEnv(e); set({ env: e }); },
  setLight(l: LightName) { viewer?.setLight(l); set({ light: l }); },
  setLook(l: LookName) { viewer?.setLook(l); localStorage.setItem('wm.look', l); const st: Partial<State> = { look: l }; if (l === 'game' && get().light === 'dusk') { viewer?.setLight('day'); st.light = 'day'; } set(st); },
  setWaterMode(m: WaterMode) { viewer?.setWaterMode(m); set({ waterMode: m }); },
  setWaterline(on: boolean) { viewer?.setWaterline(on); set({ waterline: on }); },
  setHighlight(on: boolean) { viewer?.setHighlight(on); set({ highlight: on }); },
  setCamo(on: boolean) { viewer?.setCamo(on); set({ camo: on }); },
  setGlow(g: number) { viewer?.setGlowGain(g); set({ glow: g }); },
  setOverlay(x: number) { viewer?.setOverlay(x); set({ overlay: x }); },
  setQuality(q: Quality) { if (viewer?.setQuality(q)) { localStorage.setItem('wm.quality', q); set({ quality: q, mode: get().mode === 'overlay' && !viewer.Q.overlay ? 'split' : get().mode }); loadRevs(); } },
  select(id: string | null, focus = false) { set({ selected: id, rightTab: id && get().workspace === 'review' ? 'inspect' : get().rightTab }); if (id && focus) viewer?.focus([id]); if (id) viewer?.flash([id]); },
  toggleHidden(id: string) { const h = { ...get().hidden, [id]: !get().hidden[id] }; set({ hidden: h }); viewer?.setPartVisible(id, !h[id]); },
  solo(id: string) {
    const h: Record<string, boolean> = {}; const already = get().parts.every((p) => p.id === id || get().hidden[p.id]);
    get().parts.forEach((p) => { h[p.id] = already ? false : p.id !== id; viewer?.setPartVisible(p.id, !h[p.id]); }); set({ hidden: h });
  },
  showAll() { get().parts.forEach((p) => viewer?.setPartVisible(p.id, true)); set({ hidden: {} }); },
  async revert(id: string) {
    if (!supports('undo')) { toast('error', capabilityReason('undo')); return; }
    try { const r = await api.revert(id); set({ steps: r.steps, head: r.head, viewRev: r.head }); clearManifests(); await loadRevs(); toast('ok', t('revertedTo', { n: r.steps.find((x: Step) => x.id === r.head)?.index ?? 0 })); }
    catch (e: any) { toast('error', t('opFailed'), e.message); }
  },
  async restore(id: string) {
    if (!supports('restore')) { toast('error', capabilityReason('restore')); return; }
    try { const r = await api.restore(id); set({ steps: r.steps, head: r.head, viewRev: r.head }); clearManifests(); await loadRevs(); toast('ok', t('restored')); }
    catch (e: any) { toast('error', t('opFailed'), e.message); }
  },
  async install(kind: 'install' | 'uninstall') {
    if (kind === 'install') { await actions.prepareApply(); return; }
    if (!supports('apply')) { set({ applyMessage: capabilityReason('apply') }); return; }
    const p = get().project; if (p && p.game.running) { set({ dialog: { kind: 'gameRunning' } }); return; }
    try {
      const r = await api.uninstall(); set({ dialog: null });
      await trackResult(r, 'uninstall');
    } catch (e: any) {
      set({ applyMessage: e.message }); if (e.code === 'game_running') set({ dialog: { kind: 'gameRunning' } }); else toast('error', t('jobFailed'), e.message);
    }
  },
  async prepareApply() {
    if (!supports('apply')) { set({ applyMessage: capabilityReason('apply') }); return; }
    if (get().applyBusy || get().job?.state === 'running') return;
    set({ dialog: { kind: 'apply' }, applyPlan: null, applyBusy: true, applyError: '', applyMessage: '' });
    try { const result = await api.apply(true); if (active) set({ applyPlan: result.result || result }); }
    catch (e: any) { if (active) set({ applyError: e.message, applyMessage: e.message }); }
    finally { if (active) set({ applyBusy: false }); }
  },
  async confirmApply() {
    if (!get().applyPlan || get().applyBusy || !supports('apply')) return;
    set({ applyBusy: true, applyError: '' });
    try { const result = await api.apply(false); if (!active) return; set({ dialog: null }); await trackResult(result, 'apply'); }
    catch (e: any) { if (active) set({ applyError: e.message, applyMessage: e.message }); }
    finally { if (active) set({ applyBusy: false }); }
  },
  // ---------- 工作区 / 模拟
  setWorkspace(w: Workspace) {
    const s = get(); if (w === 'sim' && (!s.sim || !supports('simulation'))) { toast('error', s.project?.capabilities?.reasons.simulation || t('simUnavailable')); return; }
    const on = w === 'sim';
    set({ workspace: w, rightTab: on ? 'sim' : 'history', simOn: on, camMode: !on && s.camMode === 'game' ? 'orbit' : s.camMode });
    if (!viewer) return;
    viewer.simEnable(on);
    if (on) applySimulationSettings(viewer);
    else { viewer.setLodMode(0); viewer.setLodTint(false); viewer.setEmisOverride(-1); }
    displayMode(viewer);
    if (on) loadedRevision?.stopWaiting?.(); else if (comparisonWanted()) void loadComparison();
  },
  setTel(t2: Telegraph) { viewer?.setTelegraph(t2); set({ tel: t2 }); },
  setRudder(r: number) { viewer?.setRudder(r); set({ rudder: r }); },
  setCamMode(m: CamMode) { viewer?.setCamMode(m); set({ camMode: m }); },
  setCombat(on: boolean) { viewer?.setCombat(on); set({ combat: on }); },
  setAimMouse(on: boolean) { if (viewer) viewer.aimFollowMouse = on; set({ aimMouse: on }); },
  setAimKm(km: number) { viewer?.setAimDistKm(km); set({ aimKm: km }); },
  setReloadReal(on: boolean) { viewer?.setReloadReal(on); set({ reloadReal: on }); },
  setWaves(w: number) { viewer?.setWaves(w); set({ waves: w }); },
  fire(group: 'main' | 'secondary', how: 'salvo' | 'single') {
    const n = viewer ? viewer.fire(group, how) : 0;
    if (!n) toast('info', t('noReady'), undefined, 1800);
  },
  setEmis(mode: number, speed?: number) { const sp = speed ?? get().emisSpeed; viewer?.setEmisOverride(mode, sp); set({ emisMode: mode, emisSpeed: sp }); },
  setLodMode(m: 'auto' | number) { set({ lodMode: viewer?.setLodMode(m) ?? m }); },
  setLodBias(b: number) { viewer?.setLodBias(b); set({ lodBias: b }); },
  setLodTint(on: boolean) { viewer?.setLodTint(on); set({ lodTint: on }); },
  setDist(p: 'near' | 'mid' | 'far') { if (!viewer) return; if (p === 'near') viewer.preset('hero'); else viewer.setDistance(p === 'mid' ? 30 : 110); set({ distPreset: p }); },
  simReset() { viewer?.simReset(); set({ tel: 'stop', rudder: 0 }); if (viewer) applySimulationSettings(viewer); },
};
