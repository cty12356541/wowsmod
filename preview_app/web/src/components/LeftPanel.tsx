import { useState } from 'react';
import { ChevronDown, Eye, EyeOff, Search, Crosshair, Diff, ScanEye } from 'lucide-react';
import { useStore, actions } from '../store';
import { api } from '../lib/api';
import { t, partLabel, partMeta } from '../lib/i18n';
import type { Part, TexInfo } from '../lib/types';
import { IconBtn, Seg, Tip } from './ui';

const GROUPS = ['hull', 'main', 'secondary', 'aa', 'director', 'radar'] as const;

export function LeftPanel() {
  const tab = useStore((s) => s.leftTab); const parts = useStore((s) => s.parts); const tex = useStore((s) => s.textures);
  const nCh = parts.filter((p) => p.changed).length, tCh = tex.filter((x) => x.changed).length;
  return (
    <div className="panel left">
      <div className="tabs" role="tablist">
        <button className="tab" role="tab" data-state={tab === 'outline' ? 'active' : ''} onClick={() => useStore.setState({ leftTab: 'outline' })}>{t('outline')}<span className="count num">{nCh ? t('nChanged', { n: nCh }) : parts.length || ''}</span></button>
        <button className="tab" role="tab" data-state={tab === 'textures' ? 'active' : ''} onClick={() => useStore.setState({ leftTab: 'textures' })}>{t('textures')}<span className="count num">{tCh ? t('nChanged', { n: tCh }) : tex.length || ''}</span></button>
      </div>
      <div className="tabpane" hidden={tab !== 'outline'}><Outliner /></div>
      <div className="tabpane" hidden={tab !== 'textures'}><Textures /></div>
    </div>
  );
}

function Outliner() {
  const s = useStore(); const [closed, setClosed] = useState<Record<string, boolean>>({ aa: true, director: true, radar: true });
  const q = s.filter.trim().toLowerCase();
  const list = s.parts.filter((p) => (!s.onlyChanged || p.changed) && (!q || p.id.toLowerCase().includes(q) || (p.model || '').toLowerCase().includes(q) || partLabel(p).toLowerCase().includes(q)));
  if (s.phase !== 'ready' && !s.parts.length) return <div className="tree">{[70, 50, 60, 45, 66, 52].map((w, i) => <div key={i} style={{ padding: '7px 12px' }}><div className="sk" style={{ width: w + '%' }} /></div>)}</div>;
  return (
    <>
      <div className="toolrow">
        <div className="input-wrap"><Search size={14} /><input className="input" placeholder={t('filterParts')} value={s.filter} onChange={(e) => useStore.setState({ filter: e.target.value })} /></div>
        <IconBtn icon={<Diff size={16} />} label={t('onlyChanged')} on={s.onlyChanged} onClick={() => useStore.setState({ onlyChanged: !s.onlyChanged })} />
        <IconBtn icon={<ScanEye size={16} />} label={t('showAll')} onClick={() => actions.showAll()} disabled={!Object.values(s.hidden).some(Boolean)} />
      </div>
      <div className="scroll">
        <div className="tree" role="tree">
          {GROUPS.map((g) => {
            const items = list.filter((p) => p.group === g); if (!items.length) return null;
            const all = s.parts.filter((p) => p.group === g); const ch = all.filter((p) => p.changed).length;
            const open = q || s.onlyChanged ? true : !closed[g];
            return (
              <div key={g} role="group">
                <div className="grp" data-open={open} onClick={() => setClosed({ ...closed, [g]: open })}>
                  <span className="tw"><ChevronDown size={14} /></span>{t('group_' + g)}
                  <span className="cnt num">{ch ? t('nOfChanged', { n: all.length, c: ch }) : all.length}</span>
                </div>
                {open && items.map((p) => <Row key={p.id} p={p} />)}
              </div>
            );
          })}
          {!list.length && <div className="empty"><div className="t">{s.onlyChanged ? t('noChangedParts') : t('noMatch')}</div></div>}
        </div>
      </div>
    </>
  );
}

function Row({ p }: { p: Part }) {
  const sel = useStore((s) => s.selected === p.id); const hidden = useStore((s) => !!s.hidden[p.id]);
  return (
    <div className="row" role="treeitem" aria-selected={sel} data-sel={sel} data-hidden={hidden} onClick={() => actions.select(sel ? null : p.id)} onDoubleClick={() => actions.select(p.id, true)}>
      <span className="lbl">{partLabel(p)}</span>
      <span className="meta">{partMeta(p)}</span>
      <Tip label={p.changed ? t('changedVsBase') : undefined}><span className="m">{p.changed ? 'M' : ''}</span></Tip>
      <IconBtn sm className="eye" icon={hidden ? <EyeOff size={14} /> : <Eye size={14} />} label={t('toggleVis')} onClick={(e) => { e.stopPropagation(); actions.toggleHidden(p.id); }} />
      <IconBtn sm className="eye" icon={<Crosshair size={14} />} label={t('focus')} onClick={(e) => { e.stopPropagation(); actions.select(p.id, true); }} />
    </div>
  );
}

const CH: Record<string, string[]> = { a: ['rgb', 'a'], n: ['r', 'g'], mg: ['r', 'g', 'b'], ao: ['r'] };
function Textures() {
  const s = useStore(); const [only, setOnly] = useState(true);
  const list = only ? s.textures.filter((x) => x.changed) : s.textures;
  const sel = s.textures.find((x) => x.name === s.selTex) || null;
  if (s.phase !== 'ready' && !s.textures.length) return <div className="tree">{[60, 72, 50].map((w, i) => <div key={i} style={{ padding: '14px 12px' }}><div className="sk" style={{ width: w + '%' }} /></div>)}</div>;
  return (
    <>
      <div className="toolrow">
        <Seg size="xs" value={only ? 'ch' : 'all'} onChange={(v) => setOnly(v === 'ch')} opts={[{ v: 'ch', label: t('texChangedN', { n: s.textures.filter((x) => x.changed).length }) }, { v: 'all', label: t('texAllN', { n: s.textures.length }) }]} />
      </div>
      <div className="scroll">
        <div className="texlist">
          {list.map((x) => <TexRow key={x.name} x={x} sel={x.name === s.selTex} />)}
          {!list.length && <div className="empty"><div className="t">{t('noChangedTex')}</div></div>}
        </div>
      </div>
      {sel && <TexCompare x={sel} ch={CH[sel.slot]?.includes(s.texCh) ? s.texCh : CH[sel.slot]?.[0] || 'rgb'} />}
    </>
  );
}
function TexRow({ x, sel }: { x: TexInfo; sel: boolean }) {
  return (
    <div className="texrow" data-sel={sel} onClick={() => useStore.setState({ selTex: sel ? null : x.name })}>
      <div className="thumb" style={{ backgroundImage: `url(${api.blobUrl(x.hash, { max: 96, ch: x.slot === 'a' || x.slot === 'n' ? 'rgb' : 'r' })})` }} />
      <div style={{ minWidth: 0 }}>
        <div className="nm">{shortTex(x.name)}</div>
        <div className="sub num">{t('slotShort_' + x.slot)} · {x.width}×{x.height} · {t('usedByN', { n: x.parts })}</div>
      </div>
      <span className="m">{x.changed ? (x.base_hash ? 'M' : 'A') : ''}</span>
    </div>
  );
}
export const shortTex = (n: string) => n.replace(/^FSB025_Bourgogne_1945_/, '…_');
function TexCompare({ x, ch }: { x: TexInfo; ch: string }) {
  const chans = CH[x.slot] || ['rgb'];
  const url = (h: string | null) => (h ? api.blobUrl(h, { max: 512, ch }) : '');
  return (
    <div className="texcmp">
      <div className="texcmp-h"><span className="nm" title={x.name}>{x.name}</span>
        {chans.length > 1 && <Seg size="xs" value={ch} onChange={(v) => useStore.setState({ texCh: v })} opts={chans.map((c) => ({ v: c, label: c.toUpperCase(), tip: t('ch_' + x.slot + '_' + c) }))} />}
      </div>
      <div className="texpair">
        <figure>{x.base_hash ? <img src={url(x.base_hash)} alt="" /> : <div className="img" />}<figcaption><span>{t('baseLabel')}</span></figcaption></figure>
        <figure><img src={url(x.hash)} alt="" /><figcaption><span>{x.changed ? t('afterLabel') : t('sameAsBase')}</span><span className="num faint">{x.width}×{x.height}</span></figcaption></figure>
      </div>
      <div className="ch-desc">{t('ch_' + x.slot + '_' + ch)}</div>
    </div>
  );
}
