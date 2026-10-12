/* 无头出图模式（wowsmod render 用）。
 *   URL:  index.html?render&view=fq&rev=head&compare=split&base=base&w=1600&h=900 ...
 *   JS:   await window.__render({ view: 'fq', compare: 'split' })  → RenderResult
 * 完成信号：window.__renderDone = true，并在控制台打印一行 `WM_RENDER_DONE {json}`（失败为 `WM_RENDER_ERROR {json}`）。
 * 输出：页面上只有一个 <canvas id="out">（尺寸 = 请求尺寸），CDP 直接截它；或取 __renderResult.dataURL（data=1）。
 * 确定性：不跑动画循环；所有时间由参数给定（t / warm / fireT），随机数固定种子；同一台机器同样参数 → 同样像素。 */
import { Viewer, CHANNELS, type Quality, type FxTheme } from '../engine/viewer';
import { api, disableLive } from '../lib/api';
import { setLang, t, type Lang } from '../lib/i18n';
import type { Manifest, Project, SimData, Step } from '../lib/types';
import type { Telegraph } from '../engine/sim';
import { TEL_FRAC } from '../engine/sim';

export interface RenderParams {
  w?: number; h?: number; quality?: Quality; lang?: Lang;
  rev?: string; base?: string; compare?: 'single' | 'side' | 'split'; splitPos?: number;
  view?: string; az?: number; el?: number; dist?: number | string; focus?: string; fill?: number; fov?: number; tgt?: string;
  channel?: string; env?: 'studio' | 'water' | 'dark'; water?: 'full' | 'cut' | 'off'; light?: 'day' | 'dusk' | 'night';
  /** 外观: game(默认, 港口对齐) / studio(影棚展台) / game-day / game-night(连同光照一起指定) */
  look?: string; /** 游戏外观参数临时覆盖(JSON, 拟合脚本用) */ gl?: string;
  /** 特效主题: project(默认, 项目 fx.json) | vanilla(原版) | JSON(FxTheme, 临时试色) */ fx?: string;
  hl?: boolean; wm?: boolean; waterline?: boolean; camo?: boolean; glow?: number;
  sim?: boolean; speed?: number | string; rudder?: number; waves?: number; warm?: number; turret?: number | null; aimKm?: number;
  fire?: 'main' | 'secondary' | 'all' | ''; fireT?: number; t?: number; emis?: number; emisSpeed?: number;
  lod?: 'auto' | number; lodBias?: number; lodTint?: boolean;
  sheet?: boolean; views?: string; revs?: string; cellW?: number; cellH?: number; title?: string;
  data?: boolean;
}
export interface RenderResult {
  ok: boolean; width: number; height: number; ms: number; dataURL?: string; error?: { code: string; message: string };
  meta: { ship: string; rev: string; step: number | null; base: string | null; view: string; channel: string; env: string; light: string; look?: string; fx?: string | null; compare: string; lod?: Record<string, number>; sim?: unknown; cells?: { view: string; rev: string; x: number; y: number; w: number; h: number }[] };
}

const VIEW_ALIAS: Record<string, string> = { fq: 'bow', 'front-quarter': 'bow', front: 'bow', rq: 'stern', 'rear-quarter': 'stern', rear: 'stern', side: 'side', top: 'top', port: 'port', hero: 'hero', funnel: 'funnel', bow: 'bow', stern: 'stern', custom: 'custom' };
const DIST: Record<string, number> = { mid: 30, far: 110 };
const bool = (v: string | null) => v !== null && v !== '0' && v !== 'false' && v !== 'no';
/** fx 参数 → 特效主题 */
function fxOf(P: RenderParams): FxTheme | null {
  const f = (P.fx || 'project').trim();
  if (f === 'project') return project?.fx?.preview ? { ...project.fx.preview, name: project.fx.name || 'project' } : null;
  if (f === 'vanilla' || f === 'off' || f === 'none') return null;
  try { return JSON.parse(f) as FxTheme; } catch { throw Object.assign(new Error(`fx= must be project | vanilla | JSON`), { code: 'bad_param' }); }
}
/** look 参数: game(默认) | studio | game-day | game-night */
export function lookOf(P: RenderParams): { look: 'game' | 'studio'; light?: 'day' | 'night' } {
  const l = (P.look || 'game').toLowerCase();
  if (l === 'studio') return { look: 'studio' };
  if (l === 'game-day' || l === 'day') return { look: 'game', light: 'day' };
  if (l === 'game-night' || l === 'night') return { look: 'game', light: 'night' };
  if (l === 'game') return { look: 'game' };
  throw Object.assign(new Error(`unknown look '${P.look}' (game | game-day | game-night | studio)`), { code: 'bad_param' });
}

/** URL 参数 → RenderParams（未给的不出现，好让默认值生效） */
export function parseQuery(q: URLSearchParams): RenderParams {
  const p: any = {}; const num = (k: string) => { const v = q.get(k); if (v !== null && v !== '' && !isNaN(+v)) p[k] = +v; };
  ['w', 'h', 'splitPos', 'az', 'el', 'fill', 'fov', 'glow', 'rudder', 'waves', 'warm', 'turret', 'aimKm', 'fireT', 't', 'emis', 'emisSpeed', 'lodBias', 'cellW', 'cellH'].forEach(num);
  ['quality', 'lang', 'rev', 'base', 'compare', 'view', 'focus', 'channel', 'env', 'water', 'light', 'look', 'gl', 'fx', 'tgt', 'fire', 'views', 'revs', 'title'].forEach((k) => { const v = q.get(k); if (v !== null) p[k] = v; });
  ['hl', 'wm', 'waterline', 'camo', 'sim', 'lodTint', 'sheet', 'data'].forEach((k) => { if (q.has(k)) p[k] = bool(q.get(k)) || q.get(k) === ''; });
  const sp = q.get('speed'); if (sp !== null) p.speed = isNaN(+sp) ? sp : +sp;
  const d = q.get('dist'); if (d !== null) p.dist = isNaN(+d) ? d : +d;
  const l = q.get('lod'); if (l !== null) p.lod = l === 'auto' ? 'auto' : +l;
  return p;
}

let viewer: Viewer | null = null; let host: HTMLDivElement; let out: HTMLCanvasElement;
let project: Project | null = null; let steps: Step[] = []; let head: string | null = null; let sim: SimData | null = null;
const manifests = new Map<string, Manifest>();
let booted: Promise<void> | null = null; let curQ: Quality | null = null; let geomHash: string | null = null;

async function ensure(q: Quality) {
  if (booted && curQ === q) return booted;
  curQ = q;
  booted = (async () => {
    disableLive();
    if (!host) {
      host = document.createElement('div'); host.style.cssText = 'position:fixed;left:0;top:0;width:16px;height:16px;overflow:hidden;opacity:0;pointer-events:none';
      document.body.appendChild(host);
    }
    if (viewer) { viewer.setQuality(q); return; }
    viewer = new Viewer(host, { quality: q, preserve: true, fixed: [1600, 900], deterministic: true });
    viewer.setDither(false);
    (window as any).__viewer = viewer;
    const [p, st] = await Promise.all([api.project(), api.steps()]); project = p; steps = st.steps; head = st.head;
    if (!head) throw Object.assign(new Error('project has no steps'), { code: 'no_model' });
    const m = await manifest(head);
    viewer.loadGeometry(await api.geometry(m.geometry.url)); geomHash = m.geometry.hash;
    try { const sd = await api.sim(); sim = sd; viewer.setSimData(sd); } catch { sim = null; }
  })();
  booted.catch(() => { booted = null; });
  return booted;
}
async function manifest(rev: string) { if (!manifests.has(rev)) manifests.set(rev, await api.model(rev)); return manifests.get(rev)!; }

/** rev 参数：head | base | s2 | 2（序号） | prev（相对 rev 的上一步，仅 base 用） */
function resolveRev(x: string | undefined, rel?: string): string {
  const act = steps.filter((s) => s.state === 'active');
  if (!x || x === 'head' || x === 'HEAD') return head!;
  if (x === 'base' || x === 'original') return 'base';
  if (x === 'prev') { const i = act.findIndex((s) => s.id === rel); return i > 0 ? act[i - 1].id : 'base'; }
  if (/^\d+$/.test(x)) { const s = steps.find((s) => s.index === +x); if (s) return s.id; }
  if (steps.some((s) => s.id === x)) return x;
  throw Object.assign(new Error(`unknown rev '${x}'`), { code: 'bad_param' });
}
const stepOf = (rev: string) => steps.find((s) => s.id === rev) || null;
const revLabel = (rev: string) => (rev === 'base' ? t('original') : `${t('stepN', { n: stepOf(rev)?.index ?? '?' })} · ${stepOf(rev)?.title ?? ''}`);
const revShort = (rev: string) => (rev === 'base' ? t('original') : t('stepN', { n: stepOf(rev)?.index ?? '?' }));
const viewLabel = (v: string) => t('v_' + (VIEW_ALIAS[v] || v));

/** 把一套参数施加到 viewer 上并画一帧（结果在 viewer 的画布里） */
async function setup(P: Required<Pick<RenderParams, 'w' | 'h'>> & RenderParams, rev: string, base: string | null, view: string) {
  const v = viewer!;
  v.setFixedSize(P.w, P.h);
  // 材质
  v.clearSide('A');
  await v.applyManifest('B', await manifest(rev));
  if (base && base !== rev && P.compare !== 'single') await v.applyManifest('A', await manifest(base));
  v.setMode(base && P.compare && P.compare !== 'single' ? P.compare : 'single');
  v.setSplit(P.splitPos ?? 0.5);
  // 外观
  const ch = CHANNELS.indexOf((P.channel || 'final') as any); v.setChannel(ch < 0 ? 0 : ch);
  const env = P.env || 'studio';
  const look = lookOf(P);
  v.look = look.look; try { v.lookOv = P.gl ? JSON.parse(P.gl) : {}; } catch { throw Object.assign(new Error('gl= must be JSON'), { code: 'bad_param' }); }
  v.setEnv(env === 'water' ? 'water' : 'studio'); v.setLight(look.light || (env === 'dark' ? 'night' : (P.light || 'day'))); v.setWaterMode(P.water || 'full');
  v.setHighlight(!!P.hl); v.setWaterline(P.waterline ?? true); v.setCamo(P.camo ?? true); v.setGlowGain(P.glow ?? 1);
  v.setEmisOverride(P.emis ?? -1, P.emisSpeed ?? 1);
  v.flashT = 0;
  v.setFx(fxOf(P));
  // 模拟
  const wantSim = !!(P.sim || P.speed !== undefined || P.turret !== undefined || P.fire || P.rudder);
  v.simEnable(false);
  if (wantSim && sim) {
    v.simReset(); v.simEnable(true); v.setReloadReal(true);
    const mo = v.motion!; mo.waves = P.waves ?? 1;
    let kn = 0;
    const TEL_ALIAS: Record<string, string> = { quarter: 'q1', '1/4': 'q1', half: 'q2', '1/2': 'q2', '3/4': 'q3', reverse: 'rev', back: 'rev' };
    if (typeof P.speed === 'string') P.speed = TEL_ALIAS[P.speed] || P.speed;
    if (typeof P.speed === 'string' && !(P.speed in TEL_FRAC)) throw Object.assign(new Error(`unknown speed '${P.speed}'`), { code: 'bad_param' });
    if (typeof P.speed === 'string') { const f = (TEL_FRAC as any)[P.speed]; kn = f === undefined ? 0 : f >= 0 ? f * mo.maxKn : -0.25 * mo.maxKn; mo.tel = (P.speed as Telegraph); }
    else if (typeof P.speed === 'number') kn = P.speed;
    mo.fixedKn = kn; mo.speedKn = kn; mo.rudderCmd = mo.rudder = Math.max(-1, Math.min(1, P.rudder || 0));
    const combat = P.turret !== undefined && P.turret !== null || !!P.fire;
    v.setCombat(combat);
    if (combat) v.aimAt(P.turret ?? 0, P.aimKm ?? 12);
    const warm = P.warm ?? (Math.abs(kn) > 0.1 ? 14 : 2);
    v.simWarm(warm); v.snapTurrets();
    if (P.fire) {
      const groups = P.fire === 'all' ? ['main', 'secondary'] : [P.fire];
      groups.forEach((g) => v.fire(g as any, 'salvo', true));
      v.simWarm(P.fireT ?? 0.07, 1 / 120);
    }
  }
  v.setTime(P.t ?? 0);
  // 相机（在模拟之后：船体姿态已定）
  const vname = VIEW_ALIAS[view] || 'hero';
  v.baseFov = P.fov && P.fov > 5 && P.fov < 120 ? P.fov : 34;
  let tgt: number[] | undefined;
  if (P.tgt) { tgt = P.tgt.split(',').map(Number); if (tgt.length !== 3 || tgt.some((x) => !isFinite(x))) throw Object.assign(new Error('tgt= must be x,y,z'), { code: 'bad_param' }); }
  if (P.az !== undefined || P.el !== undefined || vname === 'custom' || tgt) v.orbit(P.az ?? 35, P.el ?? 12, typeof P.dist === 'number' ? P.dist : undefined, tgt, true);
  else v.preset(vname, true);
  if (P.focus) {
    const ids = P.focus.split(',').map((s) => s.trim()).filter(Boolean);
    const dir = v.camera.position.clone().sub(v.controls.target).normalize();
    if (!v.focus(ids, true, dir, P.fill ?? 0.7)) throw Object.assign(new Error(`unknown part '${P.focus}'`), { code: 'bad_param' });
  }
  if (typeof P.dist === 'string' && DIST[P.dist]) v.setDistance(DIST[P.dist], true);
  else if (typeof P.dist === 'number' && P.az === undefined && P.el === undefined) v.setDistance(P.dist, true);
  v.controls.update();
  v.setLodBias(P.lodBias ?? 1); v.setLodTint(!!P.lodTint);
  v.setLodMode(P.lod ?? (P.dist !== undefined ? 'auto' : 0));
  v.frame();
}

function drawWatermark(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, lines: string[], corner: 'bl' | 'tl' = 'bl') {
  const fs = Math.max(11, Math.round(Math.min(w, h * 1.78) / 110));
  g.save(); g.font = `500 ${fs}px "Inter","Segoe UI","Microsoft YaHei","PingFang SC",sans-serif`; g.textBaseline = 'middle';
  const pad = Math.round(fs * 0.55), lh = Math.round(fs * 1.5);
  const tw = Math.max(...lines.map((l) => g.measureText(l).width)) + pad * 2, th = lines.length * lh + pad * 0.6;
  const bx = x + pad, by = corner === 'bl' ? y + h - th - pad : y + pad;
  g.fillStyle = 'rgba(16,17,19,0.62)'; g.fillRect(bx, by, tw, th);
  g.fillStyle = 'rgba(236,238,241,0.92)'; lines.forEach((l, i) => g.fillText(l, bx + pad, by + pad * 0.3 + lh * (i + 0.5)));
  g.restore();
}

/** 角标：左上 / 右上的小标签 */
function tag(g: CanvasRenderingContext2D, x: number, y: number, text: string, align: 'l' | 'r', W: number) {
  const fs = Math.max(11, Math.round(W / 110)), pad = Math.round(fs * 0.55);
  g.save(); g.font = `500 ${fs}px "Inter","Segoe UI","Microsoft YaHei","PingFang SC",sans-serif`; g.textBaseline = 'middle';
  const tw = g.measureText(text).width + pad * 2, th = Math.round(fs * 1.7);
  const bx = align === 'l' ? x + pad : x - pad - tw, by = y + pad;
  g.fillStyle = 'rgba(16,17,19,0.62)'; g.fillRect(bx, by, tw, th); g.fillStyle = 'rgba(236,238,241,0.92)'; g.fillText(text, bx + pad, by + th / 2);
  g.restore();
}

async function renderOne(P: RenderParams & { w: number; h: number }): Promise<RenderResult> {
  if (!P.compare) P.compare = P.base ? 'split' : 'single';
  const rev = resolveRev(P.rev); const base = P.compare && P.compare !== 'single' ? resolveRev(P.base || 'base', rev) : null;
  const view = P.view || (P.az !== undefined ? 'custom' : 'hero');
  await setup(P, rev, base, view);
  const v = viewer!; const g = out.getContext('2d')!;
  out.width = P.w; out.height = P.h; g.drawImage(v.canvas(), 0, 0, P.w, P.h);
  if (P.wm !== false) {
    const ship = project!.ship.name[P.lang || 'zh'] || project!.ship.name.en;
    const parts = [ship, revLabel(rev), viewLabel(view), t('ch_' + (P.channel || 'final'))];
    if (P.env === 'dark') parts.push(t('darkStudio')); else if (P.env === 'water') parts.push(t('ocean')); 
    if (v.light !== 'day' && P.env !== 'dark') parts.push(t('light_' + v.light));
    parts.push(v.look === 'game' ? t('lookGame') : t('lookStudio'));
    drawWatermark(g, 0, 0, P.w, P.h, [parts.join(' · ')]);
    if (base) {
      const a = revShort(base), b = revShort(rev);
      if (P.compare === 'split') { const sx = Math.round(P.w * (P.splitPos ?? 0.5)); tag(g, 0, 0, a, 'l', P.w); tag(g, P.w, 0, b, 'r', P.w); void sx; }
      else { tag(g, 0, 0, a, 'l', P.w); tag(g, P.w / 2, 0, b, 'l', P.w); }
    }
  }
  return {
    ok: true, width: P.w, height: P.h, ms: 0,
    meta: { ship: project!.ship.code, rev, step: stepOf(rev)?.index ?? null, base, view, channel: P.channel || 'final', env: P.env || 'studio', light: v.light, look: v.look === 'game' ? 'game-' + (v.light === 'night' ? 'night' : 'day') : 'studio', compare: base ? P.compare! : 'single', fx: v.fx ? (v.fx.name || 'custom') : null, lod: v.lodState(), sim: v.simOn ? v.simSnapshot() : undefined },
  };
}

/** 报告图：行 = 视角，列 = 版本（默认 原始 | 当前），带小标签 */
async function renderSheet(P: RenderParams): Promise<RenderResult> {
  const views = (P.views || 'side,fq,rq,top').split(',').map((s) => s.trim()).filter(Boolean);
  const h0 = resolveRev(P.rev);
  const revs = (P.revs || `base,${h0}`).split(',').map((s) => resolveRev(s.trim()));
  const cw = Math.min(1600, P.cellW || 720), ch = Math.min(1200, P.cellH || 405);
  const gap = 8, head = 40, colHead = 28, rowHead = 0, margin = 16;
  const W = margin * 2 + revs.length * cw + (revs.length - 1) * gap + rowHead;
  const H = margin + head + colHead + views.length * ch + (views.length - 1) * gap + margin;
  out.width = W; out.height = H; const g = out.getContext('2d')!;
  g.fillStyle = '#16171a'; g.fillRect(0, 0, W, H);
  const font = (wt: number, px: number) => `${wt} ${px}px "Inter","Segoe UI","Microsoft YaHei","PingFang SC",sans-serif`;
  const ship = project!.ship.name[P.lang || 'zh'] || project!.ship.name.en;
  g.fillStyle = '#eceef1'; g.font = font(600, 16); g.textBaseline = 'middle';
  g.fillText(P.title || `${ship} · ${project!.project.name}`, margin, margin + 14);
  g.fillStyle = '#8b9099'; g.font = font(400, 12);
  const envTxt = [t('ch_' + (P.channel || 'final')), P.env === 'dark' ? t('darkStudio') : P.env === 'water' ? t('ocean') : t('studio'), P.env !== 'dark' && P.light && P.light !== 'day' ? t('light_' + P.light) : ''].filter(Boolean).join(' · ');
  g.textAlign = 'right'; g.fillText(`${envTxt} · ${project!.game.client} ${project!.game.version}`, W - margin, margin + 14); g.textAlign = 'left';
  const cells: NonNullable<RenderResult['meta']['cells']> = [];
  for (let c = 0; c < revs.length; c++) {
    const x = margin + c * (cw + gap);
    g.fillStyle = '#c9ccd2'; g.font = font(600, 12); g.fillText(revLabel(revs[c]), x, margin + head + colHead / 2 - 4);
  }
  for (let r = 0; r < views.length; r++) {
    for (let c = 0; c < revs.length; c++) {
      const x = margin + c * (cw + gap), y = margin + head + colHead + r * (ch + gap);
      await setup({ ...P, w: cw, h: ch, compare: 'single' }, revs[c], null, views[r]);
      g.drawImage(viewer!.canvas(), x, y, cw, ch);
      // 小标签：左上 视角
      g.font = font(500, 11); const lab = viewLabel(views[r]); const tw = g.measureText(lab).width;
      g.fillStyle = 'rgba(16,17,19,0.62)'; g.fillRect(x + 6, y + 6, tw + 12, 20); g.fillStyle = 'rgba(236,238,241,0.92)'; g.fillText(lab, x + 12, y + 16);
      g.strokeStyle = 'rgba(255,255,255,0.06)'; g.lineWidth = 1; g.strokeRect(x + 0.5, y + 0.5, cw - 1, ch - 1);
      cells.push({ view: views[r], rev: revs[c], x, y, w: cw, h: ch });
    }
  }
  return { ok: true, width: W, height: H, ms: 0, meta: { ship: project!.ship.code, rev: h0, step: stepOf(h0)?.index ?? null, base: revs[0], view: views.join(','), channel: P.channel || 'final', env: P.env || 'studio', light: viewer!.light, look: viewer!.look === 'game' ? 'game-' + (viewer!.light === 'night' ? 'night' : 'day') : 'studio', compare: 'sheet', cells } };
}

function errCode(e: any): string {
  if (e?.code && typeof e.code === 'string' && !/^http_/.test(e.code)) return e.code;
  const m = String(e?.message || e);
  if (/webgl|context/i.test(m)) return 'webgl';
  if (e instanceof TypeError && /fetch|network/i.test(m) || /^http_/.test(e?.code || '')) return 'network';
  return 'internal';
}

let busy: Promise<unknown> = Promise.resolve();
export function render(params: RenderParams = {}): Promise<RenderResult> {
  const job = busy.then(async () => {
    const w = window as any; w.__renderDone = false; w.__renderResult = null;
    const t0 = performance.now();
    const P = { w: 1600, h: 900, ...params } as RenderParams & { w: number; h: number };
    P.w = Math.max(64, Math.min(4096, Math.round(P.w))); P.h = Math.max(64, Math.min(4096, Math.round(P.h)));
    if (P.lang) setLang(P.lang);
    let res: RenderResult;
    try {
      await ensure(P.quality || 'high');
      res = P.sheet ? await renderSheet(P) : await renderOne(P);
      res.ms = Math.round(performance.now() - t0);
      if (P.data) res.dataURL = out.toDataURL('image/png');
      document.body.style.width = res.width + 'px'; document.body.style.height = res.height + 'px';
      w.__renderResult = res; w.__renderDone = true;
      console.log('WM_RENDER_DONE ' + JSON.stringify({ ...res, dataURL: undefined }));
    } catch (e: any) {
      res = { ok: false, width: 0, height: 0, ms: Math.round(performance.now() - t0), error: { code: errCode(e), message: e?.message || String(e) }, meta: {} as any };
      w.__renderResult = res; w.__renderDone = true;
      console.log('WM_RENDER_ERROR ' + JSON.stringify(res.error));
    }
    return res;
  });
  busy = job.catch(() => undefined);
  return job;
}

/** 常驻出图(wowsmod render --warm)用: 项目改动后增量重载。
 *  重新取 project / steps / manifest; 几何按哈希比较, 变了才重载; 贴图本来就按内容哈希缓存, 只会拉新哈希。
 *  与 __render 共用队列, 串行执行。返回 { ok, booted, geometry(是否重载了几何), head, ms }。 */
export function reload(): Promise<any> {
  const job = busy.then(async () => {
    const t0 = performance.now();
    if (!booted) return { ok: true, booted: false, geometry: false, head: null, ms: 0 };
    try { await booted; } catch { booted = null; return { ok: true, booted: false, geometry: false, head: null, ms: 0 }; }
    try {
      const [p, st] = await Promise.all([api.project(), api.steps()]); project = p; steps = st.steps; head = st.head;
      manifests.clear();
      if (!head) return { ok: false, error: { code: 'no_model', message: 'project has no steps' }, ms: Math.round(performance.now() - t0) };
      const m = await manifest(head); let geometry = false;
      if (m.geometry.hash !== geomHash) { viewer!.loadGeometry(await api.geometry(m.geometry.url)); geomHash = m.geometry.hash; geometry = true; }
      try { const sd = await api.sim(); sim = sd; viewer!.setSimData(sd); } catch { /* 没有模拟数据 */ }
      return { ok: true, booted: true, geometry, head, ms: Math.round(performance.now() - t0) };
    } catch (e: any) {
      booted = null; // 下次 __render 会整体重新初始化
      return { ok: false, error: { code: errCode(e), message: e?.message || String(e) }, ms: Math.round(performance.now() - t0) };
    }
  });
  busy = job.catch(() => undefined);
  return job;
}

export function startRenderMode() {
  document.documentElement.classList.add('render-mode');
  out = document.createElement('canvas'); out.id = 'out'; out.width = 16; out.height = 16; document.body.appendChild(out);
  const w = window as any; w.__renderDone = false; w.__render = render; w.__renderParse = parseQuery; w.__renderReload = reload; w.__renderApi = 2;
  const q = new URLSearchParams(location.search);
  if (q.get('render') !== 'wait') render(parseQuery(q));
  else console.log('WM_RENDER_READY');
}
