/* 应用状态（zustand）。3D 视口实例是模块级单例（不进 React 状态），动作里直接调用。 */
import { create } from 'zustand';
import { api, ApiError, on as apiOn, startLive, type ConnState } from './lib/api';
import type { Project, Step, Part, TexInfo, Manifest, SimData, Job } from './lib/types';
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
  job: Job | null; toasts: Toast[]; dialog: null | { kind: 'uninstall' | 'gameRunning' };
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
  project: null, steps: [], head: null, viewRev: null, baseSel: 'base', parts: [], textures: [], sim: null,
  selected: null, hidden: {}, onlyChanged: false, filter: '', selTex: null, texCh: 'rgb',
  leftTab: 'outline', rightTab: 'history',
  mode: 'single', channel: 0, look: savedLook, env: 'studio', light: 'day', waterMode: 'full', waterline: true, highlight: true, camo: true, glow: 1, overlay: 0.5, quality: savedQ,
  simOn: false, tel: 'stop', rudder: 0, camMode: 'orbit', combat: false, aimMouse: true, aimKm: 12, reloadReal: false, waves: 1,
  emisMode: -1, emisSpeed: 1, lodMode: 'auto', lodBias: 1, lodTint: false, distPreset: null, snap: null,
  stats: null, job: null, toasts: [], dialog: null,
}));
const set = useStore.setState, get = useStore.getState;

// ---------------------------------------------------------------- 提示
let toastId = 0;
export function toast(kind: Toast['kind'], title: string, desc?: string, ms = 4200) {
  const id = ++toastId; set({ toasts: [...get().toasts, { id, kind, title, desc, ms }] });
  if (ms > 0) setTimeout(() => set({ toasts: get().toasts.filter((x) => x.id !== id) }), ms);
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

let manifestCache = new Map<string, Manifest>();
async function manifest(rev: string) { if (!manifestCache.has(rev)) manifestCache.set(rev, await api.model(rev)); return manifestCache.get(rev)!; }

let loadSeq = 0;
/** 载入 viewRev（B）与对比基准（A）的材质 + 部件树 + 贴图列表 */
export async function loadRevs() {
  const s = get(); const rev = s.viewRev || s.head; if (!rev || !viewer) return;
  const base = baseRev(); const seq = ++loadSeq;
  const [mB, mA, parts, tex] = await Promise.all([manifest(rev), manifest(base), api.parts(rev, base), api.textures(rev, base)]);
  if (seq !== loadSeq) return;
  set({ parts: parts.parts, textures: tex.textures });
  const prog = (d: number, n: number) => set({ texProg: n && d < n ? { d, n } : null });
  const rB = await viewer.applyManifest('B', mB, prog);
  if (seq !== loadSeq) return;
  await viewer.applyManifest('A', mA);
  set({ texProg: null });
  viewer.gc();
  const failed = rB.failed.length;
  if (failed) toast('error', t('texFailTitle', { n: failed }), rB.failed.slice(0, 4).join(', '));
}

// ---------------------------------------------------------------- 启动
export async function boot() {
  set({ phase: 'loading', loadMsg: t('ldConnect'), fatal: null });
  try {
    await api.health();
    set({ loadMsg: t('ldProject') });
    const [project, steps] = await Promise.all([api.project(), api.steps()]);
    set({ project, steps: steps.steps, head: steps.head, viewRev: steps.head });
    document.title = t('docTitle', { p: project.project.name });
    if (!steps.head) throw new ApiError('no_model', '', 404);
    const m = await api.model(steps.head); manifestCache.set(steps.head, m);
    set({ loadMsg: t('ldModel'), loadFrac: 0 });
    const buf = await api.geometry(m.geometry.url, (f) => set({ loadFrac: f }));
    set({ loadMsg: t('ldParse'), loadFrac: null });
    await new Promise((r) => setTimeout(r, 0));
    viewer!.loadGeometry(buf);
    viewer!.setFx(project.fx?.preview ?? null);
    try { const sim = await api.sim(); set({ sim }); viewer!.setSimData(sim); } catch { /* 后端没有模拟数据：模拟模式不可用 */ }
    await loadRevs();
    set({ phase: 'ready' });
    startLive();
  } catch (e: any) {
    const code = e && e.code ? e.code : 'internal';
    set({ phase: 'fatal', fatal: { code, message: e && e.message ? e.message : String(e) } });
    if (code === 'network' || code === 'timeout' || code === 'backend_unavailable') startLive();
  }
}

apiOn('connection', (c: ConnState) => {
  const prev = get().conn; set({ conn: c, offlineSince: c === 'offline' ? get().offlineSince || Date.now() : null });
  if (c === 'online' && prev === 'offline') {
    if (get().phase === 'fatal') boot(); else { resync(); toast('ok', t('reconnected'), t('reconnectedDesc')); }
  }
});
apiOn('event', async (ev: { type: string; data: any }) => {
  if (ev.type === 'steps' || ev.type === 'resync') await resync(ev.data);
  else if (ev.type === 'project' || ev.type === 'game' || ev.type === 'activity') { try { set({ project: await api.project() }); } catch { /* */ } }
  else if (ev.type === 'job') set({ job: ev.data.job });
});

async function resync(d?: any) {
  try {
    const [project, steps] = await Promise.all([api.project(), api.steps()]);
    const s = get(); const wasAtHead = !s.viewRev || s.viewRev === s.head;
    const exists = steps.steps.some((x: Step) => x.id === s.viewRev);
    manifestCache = new Map();
    viewer?.setFx(project.fx?.preview ?? null);
    set({ project, steps: steps.steps, head: steps.head, viewRev: wasAtHead || !exists ? steps.head : s.viewRev });
    await loadRevs();
    if (d && d.reason === 'new_step' && wasAtHead) {
      const st = steps.steps.find((x: Step) => x.id === steps.head);
      if (st) { toast('info', t('newStep', { n: st.index }), st.title); viewer?.flash(st.parts.slice(0, 12)); }
    }
  } catch { /* 断线由连接状态处理 */ }
}

// ---------------------------------------------------------------- 动作
export const actions = {
  setLang(l: Lang) { setLang(l); localStorage.setItem('wm.lang', l); set({ lang: l }); const p = get().project; if (p) document.title = t('docTitle', { p: p.project.name }); },
  async view(rev: string) { set({ viewRev: rev }); await loadRevs(); const st = get().steps.find((x) => x.id === rev); if (st && st.parts.length && st.parts.length <= 12) viewer?.flash(st.parts); },
  async setBase(b: BaseSel) { set({ baseSel: b }); await loadRevs(); },
  setMode(m: Mode) { const r = viewer ? viewer.setMode(m) : m; const prev = get().mode; set({ mode: r }); if ((r === 'side') !== (prev === 'side')) viewer?.preset('hero'); },
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
  setQuality(q: Quality) { if (viewer?.setQuality(q)) { localStorage.setItem('wm.quality', q); set({ quality: q, mode: viewer.mode }); loadRevs(); } },
  select(id: string | null, focus = false) { set({ selected: id, rightTab: id && get().workspace === 'review' ? 'inspect' : get().rightTab }); if (id && focus) viewer?.focus([id]); if (id) viewer?.flash([id]); },
  toggleHidden(id: string) { const h = { ...get().hidden, [id]: !get().hidden[id] }; set({ hidden: h }); viewer?.setPartVisible(id, !h[id]); },
  solo(id: string) {
    const h: Record<string, boolean> = {}; const already = get().parts.every((p) => p.id === id || get().hidden[p.id]);
    get().parts.forEach((p) => { h[p.id] = already ? false : p.id !== id; viewer?.setPartVisible(p.id, !h[p.id]); }); set({ hidden: h });
  },
  showAll() { get().parts.forEach((p) => viewer?.setPartVisible(p.id, true)); set({ hidden: {} }); },
  async revert(id: string) {
    try { const r = await api.revert(id); set({ steps: r.steps, head: r.head, viewRev: r.head }); manifestCache = new Map(); await loadRevs(); toast('ok', t('revertedTo', { n: r.steps.find((x: Step) => x.id === r.head)?.index ?? 0 })); }
    catch (e: any) { toast('error', t('opFailed'), e.message); }
  },
  async restore(id: string) {
    try { const r = await api.restore(id); set({ steps: r.steps, head: r.head, viewRev: r.head }); manifestCache = new Map(); await loadRevs(); toast('ok', t('restored')); }
    catch (e: any) { toast('error', t('opFailed'), e.message); }
  },
  async install(kind: 'install' | 'uninstall') {
    const p = get().project; if (p && p.game.running) { set({ dialog: { kind: 'gameRunning' } }); return; }
    try {
      const r = kind === 'install' ? await api.install() : await api.uninstall(); set({ job: r.job, dialog: null });
      const poll = async () => {
        const j = (await api.job(r.job.id)) as Job; set({ job: j });
        if (j.state === 'running') { setTimeout(poll, 400); return; }
        set({ project: await api.project() });
        if (j.state === 'done') toast('ok', kind === 'install' ? t('installedTitle') : t('uninstalledTitle'), j.message);
        else toast('error', t('jobFailed'), j.error?.message);
        setTimeout(() => set({ job: null }), 600);
      };
      setTimeout(poll, 300);
    } catch (e: any) {
      if (e.code === 'game_running') set({ dialog: { kind: 'gameRunning' } }); else toast('error', t('jobFailed'), e.message);
    }
  },
  // ---------- 工作区 / 模拟
  setWorkspace(w: Workspace) {
    const s = get(); if (w === 'sim' && !s.sim) { toast('error', t('simUnavailable')); return; }
    set({ workspace: w, rightTab: w === 'sim' ? 'sim' : 'history', simOn: w === 'sim' });
    if (!viewer) return;
    viewer.simEnable(w === 'sim');
    viewer.setLodMode(w === 'sim' ? s.lodMode : 0); viewer.setLodTint(w === 'sim' && s.lodTint);
    if (w === 'sim') { viewer.setCombat(s.combat); viewer.setTelegraph(s.tel); viewer.setRudder(s.rudder); viewer.setReloadReal(s.reloadReal); viewer.setAimDistKm(s.aimKm); viewer.aimFollowMouse = s.aimMouse; viewer.setEmisOverride(s.emisMode, s.emisSpeed); }
    else { viewer.setEmisOverride(-1); if (s.mode !== 'single') viewer.setMode(s.mode); }
    if (w === 'sim' && s.mode !== 'single') { viewer.setMode('single'); }
    else if (w === 'review') viewer.setMode(s.mode);
  },
  setTel(t2: Telegraph) { viewer?.setTelegraph(t2); set({ tel: t2 }); },
  setRudder(r: number) { viewer?.setRudder(r); set({ rudder: r }); },
  setCamMode(m: CamMode) { viewer?.setCamMode(m); set({ camMode: m }); },
  setCombat(on: boolean) { viewer?.setCombat(on); if (on && viewer && viewer.aimBearing === null) viewer.aimAt(60); set({ combat: on }); },
  setAimMouse(on: boolean) { if (viewer) viewer.aimFollowMouse = on; set({ aimMouse: on }); },
  setAimKm(km: number) { viewer?.setAimDistKm(km); set({ aimKm: km }); },
  setReloadReal(on: boolean) { viewer?.setReloadReal(on); set({ reloadReal: on }); },
  setWaves(w: number) { viewer?.setWaves(w); set({ waves: w }); },
  fire(group: 'main' | 'secondary', how: 'salvo' | 'single') {
    if (!get().combat) actions.setCombat(true);
    const n = viewer ? viewer.fire(group, how) : 0;
    if (!n) toast('info', t('noReady'), undefined, 1800);
  },
  setEmis(mode: number, speed?: number) { const sp = speed ?? get().emisSpeed; viewer?.setEmisOverride(mode, sp); set({ emisMode: mode, emisSpeed: sp }); },
  setLodMode(m: 'auto' | number) { viewer?.setLodMode(m); set({ lodMode: m }); },
  setLodBias(b: number) { viewer?.setLodBias(b); set({ lodBias: b }); },
  setLodTint(on: boolean) { viewer?.setLodTint(on); set({ lodTint: on }); },
  setDist(p: 'near' | 'mid' | 'far') { if (!viewer) return; if (p === 'near') viewer.preset('hero'); else viewer.setDistance(p === 'mid' ? 30 : 110); set({ distPreset: p }); },
  simReset() { viewer?.simReset(); set({ tel: 'stop', rudder: 0 }); viewer?.setTelegraph('stop'); viewer?.setRudder(0); },
};
