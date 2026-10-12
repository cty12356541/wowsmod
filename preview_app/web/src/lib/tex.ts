/* 贴图缓存：按 (内容哈希, 尺寸上限, 色彩空间) 缓存；PNG 为原始数据（不翻转 Y、不预乘 alpha）；最多 6 个并发下载。 */
import * as THREE from 'three';
import type { TexRef } from './types';

interface Entry { tex: THREE.Texture | null; promise: Promise<THREE.Texture>; bytes: number }
const cache = new Map<string, Entry>();
let aniso = 4, inflight = 0;
const queue: (() => Promise<void>)[] = [];
export const texStats = { requested: 0, loaded: 0, failed: 0, bytes: 0 };
let onStats: ((s: typeof texStats) => void) | null = null;
export const onTexStats = (cb: (s: typeof texStats) => void) => { onStats = cb; };

function pump() { while (inflight < 6 && queue.length) { const job = queue.shift()!; inflight++; job().finally(() => { inflight--; pump(); }); } }
function enqueue<T>(fn: () => Promise<T>): Promise<T> { return new Promise((ok, bad) => { queue.push(() => fn().then(ok, bad)); pump(); }); }

async function decode(blob: Blob): Promise<ImageBitmap | HTMLImageElement> {
  if ((window as any).createImageBitmap) {
    try { return await createImageBitmap(blob, { premultiplyAlpha: 'none', colorSpaceConversion: 'none', imageOrientation: 'none' } as any); } catch { /* 回落 */ }
  }
  const url = URL.createObjectURL(blob);
  try { const img = new Image(); img.decoding = 'async'; img.src = url; await img.decode(); return img; } finally { setTimeout(() => URL.revokeObjectURL(url), 1000); }
}
function withMax(url: string, max: number, w?: number, h?: number) {
  if (!max || (w && h && Math.max(w, h) <= max)) return url;
  return url + (url.includes('?') ? '&' : '?') + 'max=' + max;
}
export function getTex(info: TexRef, opt: { srgb: boolean; max: number }): Promise<THREE.Texture> {
  const key = info.hash + '|' + opt.max + '|' + (opt.srgb ? 's' : 'l');
  const hit = cache.get(key); if (hit) return hit.promise;
  texStats.requested++; onStats && onStats(texStats);
  const e: Entry = { tex: null, bytes: 0, promise: null as any };
  e.promise = enqueue(async () => {
    const r = await fetch(withMax(info.url!, opt.max, info.width, info.height), { cache: 'force-cache' });
    if (!r.ok) throw Object.assign(new Error('HTTP ' + r.status + ' ' + info.url), { code: 'texture_http', status: r.status });
    const blob = await r.blob(); const img = await decode(blob);
    const tex = new THREE.Texture(img as any);
    tex.flipY = false; tex.premultiplyAlpha = false; tex.wrapS = tex.wrapT = THREE.RepeatWrapping;
    tex.encoding = opt.srgb ? THREE.sRGBEncoding : THREE.LinearEncoding;
    tex.anisotropy = aniso; tex.generateMipmaps = true; tex.minFilter = THREE.LinearMipmapLinearFilter;
    tex.needsUpdate = true; tex.userData = { hash: info.hash, w: img.width, h: img.height };
    e.tex = tex; e.bytes = img.width * img.height * 4 * 1.33;
    texStats.loaded++; texStats.bytes += blob.size; onStats && onStats(texStats);
    return tex;
  }).catch((err) => { texStats.failed++; cache.delete(key); onStats && onStats(texStats); throw err; });
  cache.set(key, e);
  return e.promise;
}
const solid = (r: number, g: number, b: number, a: number, srgb = false) => {
  const t = new THREE.DataTexture(new Uint8Array([r, g, b, a]), 1, 1, THREE.RGBAFormat);
  t.encoding = srgb ? THREE.sRGBEncoding : THREE.LinearEncoding; t.needsUpdate = true; return t;
};
export const FALLBACK = { a: solid(150, 156, 160, 255, true), n: solid(128, 128, 255, 255), mg: solid(110, 0, 0, 255), ao: solid(255, 255, 255, 255) };
export function setAniso(a: number) { aniso = a; cache.forEach((e) => { if (e.tex && e.tex.anisotropy !== a) { e.tex.anisotropy = a; e.tex.needsUpdate = true; } }); }
export function texGC(keep: Set<THREE.Texture>) { let n = 0; cache.forEach((e, k) => { if (e.tex && !keep.has(e.tex)) { e.tex.dispose(); cache.delete(k); n++; } }); return n; }
export function texGpuBytes() { let b = 0; cache.forEach((e) => { b += e.bytes; }); return b; }
