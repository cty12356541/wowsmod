import { createRoot } from 'react-dom/client';
import './styles/app.css';
import App from './App';
import { startRenderMode } from './render/render';
import { useStore, actions, viewer } from './store';

// ?render → 无头出图模式（无 UI，见 render/render.ts 与 API.md §12）；否则正常界面
if (new URLSearchParams(location.search).has('render')) startRenderMode();
else {
  createRoot(document.getElementById('root')!).render(<App />);
  // 自动化/调试钩子（截图脚本用；不属于 API）
  const w = window as any;
  w.WM = {
    store: useStore, actions, get V() { return viewer; },
    /** 截图用：按固定步长推进模拟到指定状态，然后暂停并画一帧（与无头出图同一套确定性逻辑） */
    simPose(o: { tel?: string; rudder?: number; warm?: number; turret?: number; km?: number; fire?: 'main' | 'secondary' | 'all'; fireT?: number; t?: number }) {
      const V = viewer!; V.simReset();
      if (o.tel) actions.setTel(o.tel as any);
      const mo = V.motion!; const f = ({ rev: -0.25, stop: 0, q1: 0.25, q2: 0.5, q3: 0.75, full: 1 } as any)[o.tel || 'stop'] ?? 0;
      mo.speedKn = f * mo.maxKn; if (o.rudder !== undefined) { actions.setRudder(o.rudder); mo.rudder = o.rudder; }
      if (o.turret !== undefined) { actions.setCombat(true); V.aimAt(o.turret, o.km); }
      V.simWarm(o.warm ?? 20); V.snapTurrets();
      if (o.fire) { (o.fire === 'all' ? ['main', 'secondary'] : [o.fire]).forEach((g) => V.fire(g as any, 'salvo', true)); V.simWarm(o.fireT ?? 0.06, 1 / 120); }
      if (o.t !== undefined) V.setTime(o.t);
      V.pause(true); V.frame(); useStore.setState({ snap: V.simSnapshot() });
      return V.simSnapshot();
    },
    /** 测帧率：rAF 实际帧率（受显示器刷新率限制）+ 不限帧的单帧耗时（frame + 1 像素回读强制同步 = CPU+GPU 真实成本） */
    async bench(o: { seconds?: number; frames?: number } = {}) {
      const V = viewer!; V.pause(false); V.bench = true;
      const sec = o.seconds ?? 8; const ts: number[] = [];
      await new Promise<void>((res) => { const t0 = performance.now(); const f = (n: number) => { ts.push(n); if (n - t0 < sec * 1000) requestAnimationFrame(f); else res(); }; requestAnimationFrame(f); });
      const iv = ts.slice(1).map((x, i) => x - ts[i]).sort((a, b) => a - b);
      V.pause(true);
      const gl = V.renderer.getContext(); const px = new Uint8Array(4); const N = o.frames ?? 120; const ms: number[] = [];
      for (let i = 0; i < N; i++) { const a = performance.now(); V.advance(1 / 60); V.frame(); gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px); ms.push(performance.now() - a); }
      ms.sort((a, b) => a - b); V.bench = false; V.pause(false);
      const q = (arr: number[], p: number) => arr[Math.min(arr.length - 1, Math.floor(arr.length * p))];
      const st = V.stats();
      return { rafFps: +(1000 / (iv.reduce((a, b) => a + b, 0) / iv.length)).toFixed(1), rafP99ms: +q(iv, 0.99).toFixed(1),
        frameMsMedian: +q(ms, 0.5).toFixed(2), frameMsP95: +q(ms, 0.95).toFixed(2), uncappedFps: +(1000 / q(ms, 0.5)).toFixed(0),
        draws: st.draws, tris: st.tris, quality: st.quality, pr: st.pr, gl: st.gl, gpuTexMB: +st.gpuTexMB.toFixed(0), size: [V.renderer.domElement.width, V.renderer.domElement.height] };
    },
    still() { viewer!.pause(true); viewer!.frame(); },
    resume() { viewer!.pause(false); },
  };
  const benchQ = new URLSearchParams(location.search).has('bench');
  useStore.subscribe((s) => { w.__ready = s.phase === 'ready' && !s.texProg; if (benchQ && w.__ready && viewer && !viewer.bench) viewer.bench = true; });
}
