/* wowsmod 预览接口客户端（API v1，见 API.md）
 * 实时更新：优先 SSE（/events），断开后退回 2 秒一次的 /state 轮询并持续尝试恢复 SSE；25 秒无消息判定假死。 */
import { projectPrefix } from './scope';
import type { LibrarySummary, LibraryStatus, Manifest } from './types';
const ORIGIN = ((window as any).WM_API_BASE || '');
const ROOT = ORIGIN + '/api/v1';
const BASE = ORIGIN + projectPrefix(location.search);
export const projectScope = BASE;

export class ApiError extends Error {
  code: string; status: number; detail: unknown;
  constructor(code: string, message: string, status: number, detail?: unknown) { super(message || code); this.code = code; this.status = status; this.detail = detail; }
}
export type ConnState = 'connecting' | 'online' | 'offline';
type Handler = (d: any) => void;

let conn: ConnState = 'connecting', lastMsg = 0, lastEtag: number | null = null;
let es: EventSource | null = null, pollT: number | null = null;
const handlers: Record<string, Handler[]> = {};
const emit = (ev: string, d: any) => (handlers[ev] || []).forEach((fn) => { try { fn(d); } catch (e) { console.error(e); } });
export const on = (ev: 'connection' | 'event' | 'state', fn: Handler) => { (handlers[ev] = handlers[ev] || []).push(fn); return () => { handlers[ev] = handlers[ev].filter((f) => f !== fn); }; };
function setConn(c: ConnState) { if (c !== conn) { conn = c; emit('connection', c); } }
let liveEnabled = false;
let watchdog: ReturnType<typeof setInterval> | null = null;
const requests = new Set<AbortController>();

async function request<T = any>(method: string, path: string, body?: unknown, opt: { timeout?: number; root?: boolean } = {}): Promise<T> {
  const ctl = new AbortController(); const to = setTimeout(() => ctl.abort(), opt.timeout || 20000);
  requests.add(ctl);
  let r: Response;
  try {
    r = await fetch((opt.root ? ROOT : BASE) + path, { method, signal: ctl.signal, cache: 'no-store', headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined, body: body !== undefined ? JSON.stringify(body) : undefined });
  } catch (e: any) { noteFailure(); throw new ApiError(e.name === 'AbortError' ? 'timeout' : 'network', e.message, 0); }
  finally { clearTimeout(to); requests.delete(ctl); }
  if (r.status === 503) noteFailure(); else noteOk();
  let data: any = null;
  if ((r.headers.get('content-type') || '').includes('json')) { try { data = await r.json(); } catch { data = null; } }
  if (!r.ok || (data && data.ok === false)) { const e = (data && data.error) || {}; throw new ApiError(e.code || 'http_' + r.status, e.message || 'HTTP ' + r.status, r.status, e.detail); }
  return data as T;
}
function noteFailure() { setConn('offline'); if (liveEnabled && !pollT) startPoll(); }
function noteOk() { lastMsg = Date.now(); setConn('online'); }

async function binary(url: string, onProgress?: (f: number) => void): Promise<ArrayBuffer> {
  const ctl = new AbortController(); requests.add(ctl);
  try {
  let r: Response;
  try { r = await fetch(url, { cache: 'force-cache', signal: ctl.signal }); } catch (e: any) { noteFailure(); throw new ApiError('network', e.message, 0); }
  if (!r.ok) { let d: any = null; try { d = await r.json(); } catch { /* */ } const e = (d && d.error) || {}; throw new ApiError(e.code || 'http_' + r.status, e.message || 'HTTP ' + r.status, r.status); }
  noteOk();
  const total = +(r.headers.get('content-length') || 0);
  if (!r.body || !total || r.headers.get('content-encoding')) return await r.arrayBuffer();
  const reader = r.body.getReader(); const out = new Uint8Array(total); let got = 0;
  for (;;) {
    const { done, value } = await reader.read(); if (done) break;
    if (got + value.length > out.length) return new Blob([out.subarray(0, got), value]).arrayBuffer();
    out.set(value, got); got += value.length; onProgress && onProgress(got / total);
  }
  return out.buffer;
  } finally { requests.delete(ctl); }
}

function handleEtag(etag: number, d?: any) { if (lastEtag !== null && etag !== lastEtag) emit('event', { type: 'resync', data: d || {} }); lastEtag = etag; }
export function startLive() {
  liveEnabled = true; stopLive();
  if (!watchdog) watchdog = setInterval(() => { if (es && !pollT && lastMsg && Date.now() - lastMsg > 25000) { stopLive(); startPoll(); } }, 5000);
  if ((window as any).EventSource && !(window as any).WM_NO_SSE) {
    try {
      es = new EventSource(BASE + '/events');
      es.addEventListener('hello', (e: MessageEvent) => { noteOk(); const d = JSON.parse(e.data); handleEtag(d.etag, d); stopPoll(); });
      ['steps', 'project', 'activity', 'job', 'game'].forEach((t) => es!.addEventListener(t, (e: MessageEvent) => { noteOk(); const d = JSON.parse(e.data); lastEtag = d.etag; emit('event', { type: t, data: d }); }));
      es.addEventListener('ping', (e: MessageEvent) => { noteOk(); try { handleEtag(JSON.parse(e.data).etag); } catch { /* */ } });
      es.onerror = () => { if (es && es.readyState === 2) es = null; startPoll(); };
    } catch { startPoll(); }
  } else startPoll();
}
function startPoll() {
  if (pollT || !liveEnabled) return;
  const tick = async () => {
    try {
      const s: any = await request('GET', '/state', undefined, { timeout: 5000 });
      handleEtag(s.etag, s); emit('state', s);
      if (!es && liveEnabled) startLive();
    } catch { /* noteFailure 已处理 */ }
  };
  pollT = window.setInterval(tick, 2000); tick();
}
function stopPoll() { if (pollT) { clearInterval(pollT); pollT = null; } }
export function stopLive() { if (es) { es.close(); es = null; } }
export function disableLive() { liveEnabled = false; stopLive(); stopPoll(); if (watchdog) { clearInterval(watchdog); watchdog = null; } }
export function disposeRequests() { disableLive(); requests.forEach((ctl) => ctl.abort()); requests.clear(); }

function resourceUrl(url: string): string {
  // Older direct-preview servers return root-relative resources. Keep these scoped too.
  if (url.startsWith('/api/v1/') && !url.startsWith('/api/v1/projects/')) return BASE + url.slice('/api/v1'.length);
  if (ORIGIN && url.startsWith('/')) return ORIGIN + url;
  return url;
}
function scopedManifest(m: Manifest): Manifest {
  m.geometry.url = resourceUrl(m.geometry.url);
  Object.values(m.materials).forEach((mat) => Object.values(mat.textures).forEach((tex) => { if (tex?.url) tex.url = resourceUrl(tex.url); }));
  return m;
}

const enc = encodeURIComponent;
export const api = {
  conn: () => conn,
  health: () => request('GET', '/health', undefined, { timeout: 4000, root: true }),
  project: () => request('GET', '/project'),
  steps: () => request('GET', '/steps'),
  parts: (rev: string, base: string) => request('GET', `/parts?rev=${enc(rev || 'head')}&base=${enc(base || 'base')}`),
  model: async (rev: string) => scopedManifest(await request<Manifest>('GET', `/model?rev=${enc(rev || 'head')}`)),
  sim: () => request('GET', '/sim'),
  textures: async (rev: string, base: string) => { const result = await request('GET', `/textures?rev=${enc(rev || 'head')}&base=${enc(base || 'base')}`); result.textures?.forEach((tex: any) => { if (tex.url) tex.url = resourceUrl(tex.url); if (tex.base_url) tex.base_url = resourceUrl(tex.base_url); }); return result; },
  resourceUrl,
  blobUrl: (hash: string, opt: { max?: number; ch?: string } = {}) => `${BASE}/blob/${hash}.png` + ((opt.max || (opt.ch && opt.ch !== 'rgba')) ? '?' + [opt.max ? 'max=' + opt.max : '', opt.ch && opt.ch !== 'rgba' ? 'ch=' + opt.ch : ''].filter(Boolean).join('&') : ''),
  geometry: (url: string, onProgress?: (f: number) => void) => binary(resourceUrl(url), onProgress),
  revert: (id: string) => request('POST', `/steps/${enc(id)}/revert`, {}),
  restore: (id: string) => request('POST', `/steps/${enc(id)}/restore`, {}),
  install: () => request('POST', '/install', {}),
  apply: (dryRun: boolean) => request('POST', '/apply', { dry_run: dryRun }),
  uninstall: () => request('POST', '/uninstall', {}),
  job: (id: string) => request('GET', `/jobs/${enc(id)}`),
};

const libraryRequest = (path: string, body?: unknown) => request(body === undefined ? 'GET' : 'POST', '/library' + path, body, { root: true, timeout: path === '/scan' ? 120000 : 20000 });
export const libraryApi = {
  list: () => libraryRequest('') as Promise<LibrarySummary>,
  add: (path: string) => libraryRequest('/add', { path }),
  scan: (root: string) => libraryRequest('/scan', { root }),
  update: (id: string, patch: { display_name?: string; tags?: string[]; favorite?: boolean; status?: LibraryStatus }) => libraryRequest(`/projects/${enc(id)}/update`, patch),
  relocate: (id: string, path: string) => libraryRequest(`/projects/${enc(id)}/relocate`, { path }),
  remove: (id: string) => libraryRequest(`/projects/${enc(id)}/remove`, {}),
  open: (id: string) => libraryRequest(`/projects/${enc(id)}/open`, {}),
};
