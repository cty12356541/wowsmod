import { useEffect, useRef, useState } from 'react';
import { Camera, SlidersHorizontal, Layers, Sun, Sunset, Moon, RotateCcw, Info } from 'lucide-react';
import { useStore, actions, setViewer, viewer, baseRev, boot } from '../store';
import { Viewer, CHANNELS } from '../engine/viewer';
import { t } from '../lib/i18n';
import { IconBtn, Menu, Seg, SelBtn, Slider, Switch, Tip } from './ui';
import * as Popover from '@radix-ui/react-dialog';

export function Viewport() {
  const host = useRef<HTMLDivElement>(null);
  const s = useStore();
  useEffect(() => {
    const v = new Viewer(host.current!, { quality: useStore.getState().quality, onContextLost: () => useStore.setState({ phase: 'fatal', fatal: { code: 'context_lost', message: t('ctxLost') } }) });
    if (useStore.getState().look !== v.look) v.setLook(useStore.getState().look);
    setViewer(v); (window as any).__viewer = v;
    v.onPick((id) => actions.select(id));
    let last = 0;
    v.onFrame(() => {
      const now = performance.now(); if (now - last < 120) return; last = now;
      const st = v.stats();
      useStore.setState({ stats: { fps: st.fps, draws: st.draws, tris: st.tris, idle: st.idle, gl: st.gl, gpuTexMB: st.gpuTexMB }, snap: v.simOn ? v.simSnapshot() : useStore.getState().snap });
    });
    const idleT = setInterval(() => { if (v.idle) useStore.setState({ stats: { ...(useStore.getState().stats || {} as any), idle: true } }); }, 1000);
    boot();
    return () => { clearInterval(idleT); };
  }, []);
  useEffect(() => keyboard(), []);
  const dark = s.light === 'night' || (s.light === 'dusk' && s.env === 'water');
  const hasA = s.mode !== 'single' && s.workspace === 'review';
  const cur = s.steps.find((x) => x.id === (s.viewRev || s.head));
  const b = baseRev(); const baseStep = s.steps.find((x) => x.id === b);
  const baseName = b === 'base' ? t('original') : t('stepN', { n: baseStep?.index ?? '' });
  const curName = cur ? t('stepN', { n: cur.index }) + ' · ' + cur.title : '';
  const reviewing = s.viewRev && s.head && s.viewRev !== s.head;
  return (
    <div className={'viewport' + (dark ? ' dark' : '')}>
      <ViewBar />
      <div className={'vcanvas' + (s.conn === 'offline' ? ' offline' : '')} ref={host}>
        {hasA && s.mode === 'split' && <Splitter />}
        {hasA && s.mode === 'split' && <><span className="vlabel" style={{ left: 8 }}>{baseName}</span><span className="vlabel" style={{ right: 8 }}>{curName}</span></>}
        {hasA && s.mode === 'side' && <><span className="vlabel" style={{ left: 8 }}>{baseName}</span><span className="vlabel" style={{ left: 'calc(50% + 8px)' }}>{curName}</span></>}
        {hasA && s.mode === 'overlay' && <span className="vlabel" style={{ left: 8 }}>{t('overlayLabel', { a: baseName, p: Math.round(s.overlay * 100) })}</span>}
        {reviewing && s.workspace === 'review' && (
          <div className="reviewbar"><span>{t('reviewing', { n: cur?.index ?? '', h: s.steps.find((x) => x.id === s.head)?.index ?? '' })}</span>
            <button className="btn ghost" onClick={() => actions.view(s.head!)}>{t('latest')}</button></div>
        )}
        {s.workspace === 'sim' && <SimHud />}
        {s.workspace === 'sim' && s.camMode === 'game' && <div className="crosshair" />}
        {s.workspace === 'review' && s.phase === 'ready' && !hasA && <div className="vfoot">{t('vpHint')}</div>}
        {(s.phase === 'loading' || s.phase === 'boot') && (
          <div className="loading"><div className="box"><div>{s.loadMsg}{s.loadFrac !== null ? ` ${Math.round(s.loadFrac * 100)}%` : ''}</div>
            <div className={'bar' + (s.loadFrac === null ? ' ind' : '')}><i style={{ width: s.loadFrac !== null ? s.loadFrac * 100 + '%' : undefined }} /></div></div></div>
        )}
        {s.phase === 'fatal' && <Fatal />}
      </div>
    </div>
  );
}

function Fatal() {
  const f = useStore((s) => s.fatal)!;
  const net = f.code === 'network' || f.code === 'timeout' || f.code === 'backend_unavailable';
  const noModel = f.code === 'no_model';
  return (
    <div className="fatal" role="alert"><div className="box">
      <Info size={20} className="muted" />
      <h2>{net ? t('fatalNetTitle') : noModel ? t('fatalNoModelTitle') : t('fatalTitle')}</h2>
      <p dangerouslySetInnerHTML={{ __html: net ? t('fatalNetDesc') : noModel ? t('fatalNoModelDesc') : f.message }} />
      <div className="acts"><button className="btn primary" onClick={() => boot()}>{t('retry')}</button></div>
      {!noModel && <div className="err">{f.code}{f.message && !net ? '' : ' · ' + f.message}</div>}
    </div></div>
  );
}

function Splitter() {
  const split = useStore((s) => s.overlay); void split;
  const [x, setX] = useState(0.5);
  return (
    <div className="splitter" style={{ left: x * 100 + '%' }} role="separator" aria-orientation="vertical"
      onPointerDown={(e) => {
        const el = (e.currentTarget.parentElement as HTMLElement).getBoundingClientRect(); (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        const mv = (ev: PointerEvent) => { const f = Math.min(0.98, Math.max(0.02, (ev.clientX - el.left) / el.width)); setX(f); viewer?.setSplit(f); };
        const up = () => { window.removeEventListener('pointermove', mv); window.removeEventListener('pointerup', up); };
        window.addEventListener('pointermove', mv); window.addEventListener('pointerup', up); e.stopPropagation();
      }}><div className="grip" /></div>
  );
}

function ViewBar() {
  const s = useStore(); const review = s.workspace === 'review';
  const chans = CHANNELS.map((c, i) => ({ c, i }));
  return (
    <div className="vbar">
      {review && <>
        <Seg value={s.mode} onChange={(m) => actions.setMode(m)} ariaLabel={t('compare')}
          opts={[{ v: 'single', label: t('single'), kbd: '1' }, { v: 'split', label: t('split'), kbd: '2' }, { v: 'side', label: t('side'), kbd: '3' }, { v: 'overlay', label: t('overlay'), disabled: s.quality === 'low', tip: s.quality === 'low' ? t('overlayLowQ') : undefined, kbd: '4' }]} />
        <Menu trigger={<SelBtn>{s.baseSel === 'base' ? t('baseOrig') : t('basePrev')}</SelBtn>}
          items={[{ heading: t('compareWith') }, { label: t('baseOrig'), checked: s.baseSel === 'base', onSelect: () => actions.setBase('base') }, { label: t('basePrev'), checked: s.baseSel === 'prev', onSelect: () => actions.setBase('prev') }]} />
        {s.mode === 'overlay' && <div style={{ width: 80, margin: '0 4px' }}><Slider value={s.overlay} min={0} max={1} step={0.01} onChange={(v) => actions.setOverlay(v)} label={t('opacity')} /></div>}
        <div className="vsep" />
      </>}
      <Menu trigger={<SelBtn><Layers size={14} />{t('ch_' + CHANNELS[s.channel])}</SelBtn>}
        items={[{ heading: t('channel') }, ...chans.map(({ c, i }) => ({ label: t('ch_' + c), checked: s.channel === i, onSelect: () => actions.setChannel(i), kbd: i ? String.fromCharCode(65 + 0) && ['', 'A', 'N', 'E', 'W', 'M'][i] : 'F' }))]} />
      <div className="grow" />
      <Seg value={s.look} onChange={(l) => actions.setLook(l)} ariaLabel={t('look')} opts={[{ v: 'game', label: t('lookGame'), tip: t('lookGameTip') }, { v: 'studio', label: t('lookStudio'), tip: t('lookStudioTip') }]} />
      <div className="vsep" />
      <Seg value={s.env} onChange={(e) => actions.setEnv(e)} ariaLabel={t('environment')} opts={[{ v: 'studio', label: t('studio'), tip: t('studioTip') }, { v: 'water', label: t('ocean'), tip: t('oceanTip') }]} />
      {s.env === 'water' && <Seg value={s.waterMode} onChange={(m) => actions.setWaterMode(m)} ariaLabel={t('waterMode')} opts={[{ v: 'full', label: t('wmFull'), tip: t('wmFullTip') }, { v: 'cut', label: t('wmCut'), tip: t('wmCutTip') }, { v: 'off', label: t('wmOff'), tip: t('wmOffTip') }]} />}
      <Seg value={s.light} onChange={(l) => actions.setLight(l)} ariaLabel={t('lighting')}
        opts={[{ v: 'day', label: <Sun size={14} />, tip: t('lightDay') }, ...(s.look === 'studio' ? [{ v: 'dusk' as const, label: <Sunset size={14} />, tip: t('lightDusk') }] : []), { v: 'night', label: <Moon size={14} />, tip: t('lightNight') }]} />
      <div className="vsep" />
      <Menu align="end" trigger={<IconBtn icon={<Camera size={16} />} label={t('views')} />}
        items={[{ heading: t('views') }, ...['hero', 'side', 'bow', 'stern', 'top', 'port', 'funnel'].map((v, i) => ({ label: t('v_' + v), kbd: String(i + 5 <= 9 ? '' : ''), onSelect: () => viewer?.preset(v) })),
          { sep: true }, { label: t('focusSel'), disabled: !s.selected, onSelect: () => s.selected && viewer?.focus([s.selected]), kbd: 'F' }]} />
      <DisplayMenu />
    </div>
  );
}

function DisplayMenu() {
  const s = useStore(); const [open, setOpen] = useState(false);
  return (
    <Popover.Root open={open} onOpenChange={setOpen} modal={false}>
      <Popover.Trigger asChild><IconBtn icon={<SlidersHorizontal size={16} />} label={t('display')} on={open} /></Popover.Trigger>
      <Popover.Portal>
        <Popover.Content className="menu" style={{ position: 'fixed', right: 12, top: 76, width: 248, padding: '8px 12px' }} onOpenAutoFocus={(e) => e.preventDefault()} aria-describedby={undefined}>
          <Popover.Title className="menu-label" style={{ padding: '0 0 6px' }}>{t('display')}</Popover.Title>
          <Row label={t('highlight')}><Switch on={s.highlight} onChange={actions.setHighlight} label={t('highlight')} /></Row>
          <Row label={t('waterline')}><Switch on={s.waterline} onChange={actions.setWaterline} label={t('waterline')} /></Row>
          <Row label={t('camo')} tip={t('camoTip')}><Switch on={s.camo} onChange={actions.setCamo} label={t('camo')} /></Row>
          <Row label={t('glowGain')} tip={t('glowGainTip')}><div style={{ width: 96 }}><Slider value={s.glow} min={0} max={3} step={0.05} onChange={actions.setGlow} label={t('glowGain')} /></div><span className="num faint" style={{ width: 28, textAlign: 'right' }}>{s.glow.toFixed(1)}×</span></Row>
          <div className="menu-sep" />
          <button className="btn ghost" style={{ width: '100%', justifyContent: 'flex-start' }} onClick={() => { actions.setHighlight(true); actions.setWaterline(true); actions.setCamo(true); actions.setGlow(1); }}><RotateCcw size={14} />{t('resetDisplay')}</button>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
function Row({ label, tip, children }: { label: string; tip?: string; children: React.ReactNode }) {
  return <div style={{ display: 'flex', alignItems: 'center', gap: 8, height: 28, fontSize: 12 }}><Tip label={tip}><span style={{ flex: 1, color: 'var(--g8)' }}>{label}</span></Tip>{children}</div>;
}

function SimHud() {
  const sn = useStore((s) => s.snap); const sim = useStore((s) => s.sim);
  const [open, setOpen] = useState(() => localStorage.getItem('wm.hud') !== '0');
  if (!sn || !sim) return null;
  const hullLod = sn.lod['hull/MidFront'] ?? 0; const mainLod = sn.lod['HP_FGM_1'] ?? 0;
  const toggle = () => { const v = !open; setOpen(v); localStorage.setItem('wm.hud', v ? '1' : '0'); };
  // 单行紧凑 HUD, 贴视口左下角; 可收起成一个小标签。“部分近似”并入行尾的 ≈ 标记(悬停看说明), 不再单独占右下角
  return (
    <div className={'hud num' + (open ? '' : ' min')}>
      <button className="hud-t" onClick={toggle} aria-label={open ? t('hudHide') : t('hudShow')} aria-expanded={open}>
        <span>{sn.speedKn.toFixed(1)} {t('kn')}</span>
      </button>
      {open && <>
        <span className="k">{t('heading')}</span><span>{sn.heading.toFixed(0)}°</span>
        <span className="k">{t('rudderAngle')}</span><span>{(sn.rudder * (sim.ship.maxRudderAngle_deg || 30)).toFixed(0)}°</span>
        <span className="k">LOD</span><span>{t('hudLod', { h: hullLod < 0 ? '–' : hullLod, m: mainLod < 0 ? '–' : mainLod })}</span>
      </>}
      <Tip label={<>{t('approxShort')}：{t('approxNote')}</>} side="top"><span className="approx-tag" tabIndex={0} aria-label={t('approxShort')}>≈</span></Tip>
    </div>
  );
}

// ---------------------------------------------------------------- 键盘
function keyboard() {
  const h = (e: KeyboardEvent) => {
    const tg = e.target as HTMLElement; if (tg && (tg.tagName === 'INPUT' || tg.tagName === 'TEXTAREA' || tg.isContentEditable)) return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const s = useStore.getState(); const k = e.key.toLowerCase();
    if (k === 'tab') { e.preventDefault(); if (s.sim) actions.setWorkspace(s.workspace === 'review' ? 'sim' : 'review'); return; }
    if (s.workspace === 'review') {
      const m = { '1': 'single', '2': 'split', '3': 'side', '4': 'overlay' }[k]; if (m) { actions.setMode(m as any); return; }
      if (k === 'f' && s.selected) { viewer?.focus([s.selected]); return; }
      const ch = { a: 1, n: 2, e: 3, w: 4, m: 5 }[k]; if (ch !== undefined) { actions.setChannel(s.channel === ch ? 0 : ch); return; }
      if (k === 'escape') { actions.select(null); return; }
      if (k === 'arrowup' || k === 'arrowdown') {
        const act = s.steps; const i = act.findIndex((x) => x.id === (s.viewRev || s.head)); const j = i + (k === 'arrowup' ? -1 : 1);
        if (act[j]) { e.preventDefault(); actions.view(act[j].id); }
      }
      return;
    }
    // 模拟
    const tels = ['rev', 'stop', 'q1', 'q2', 'q3', 'full'] as const; const ti = tels.indexOf(s.tel as any);
    if (k === 'w') actions.setTel(tels[Math.min(5, ti + 1)]);
    else if (k === 's') actions.setTel(tels[Math.max(0, ti - 1)]);
    else if (k === 'a') actions.setRudder(Math.max(-1, s.rudder - 0.5));
    else if (k === 'd') actions.setRudder(Math.min(1, s.rudder + 0.5));
    else if (k === ' ') { e.preventDefault(); actions.fire('main', 'salvo'); }
    else if (k === 'f') actions.fire('main', 'single');
    else if (k === 'r') actions.fire('secondary', 'salvo');
    else if (k === 'c') actions.setCombat(!s.combat);
    else if (k === '1' || k === '2' || k === '3') actions.setDist((['near', 'mid', 'far'] as const)[+k - 1]);
  };
  window.addEventListener('keydown', h); return () => window.removeEventListener('keydown', h);
}
