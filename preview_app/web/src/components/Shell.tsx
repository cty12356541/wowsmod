import { X, AlertTriangle, CheckCircle2, Info, WifiOff } from 'lucide-react';
import * as D from '@radix-ui/react-dialog';
import { useStore, actions, dismissToast } from '../store';
import { t } from '../lib/i18n';
import { Seg, Tip } from './ui';
import { History } from './History';
import { Inspector } from './Inspector';
import { SimPanel } from './SimPanel';

export function RightPanel() {
  const s = useStore();
  const tabs: [typeof s.rightTab, string][] = s.workspace === 'sim' ? [['sim', t('tabSim')], ['history', t('tabHistory')]] : [['history', t('tabHistory')], ['inspect', t('tabInspect')]];
  const cur = tabs.some(([k]) => k === s.rightTab) ? s.rightTab : tabs[0][0];
  return (
    <div className="panel right">
      <div className="tabs" role="tablist">
        {tabs.map(([k, l]) => <button key={k} role="tab" className="tab" data-state={cur === k ? 'active' : ''} onClick={() => useStore.setState({ rightTab: k })}>{l}{k === 'history' && <span className="count num">{s.steps.filter((x) => x.state === 'active').length || ''}</span>}</button>)}
      </div>
      <div className="tabpane scroll">
        {cur === 'history' && <History />}
        {cur === 'inspect' && <Inspector />}
        {cur === 'sim' && <SimPanel />}
      </div>
    </div>
  );
}

export function StatusBar() {
  const s = useStore(); const st = s.stats;
  const conn = s.conn;
  const head = s.steps.find((x) => x.id === s.head);
  return (
    <footer className="statusbar num">
      <span className={'dot ' + (conn === 'online' ? 'ok' : conn === 'offline' ? 'err' : 'warn')} />
      <span>{conn === 'online' ? t('connOnline') : conn === 'offline' ? t('connOffline') : t('connConnecting')}</span>
      {head && <><span className="sep" /><span>{t('headIs', { n: head.index })}</span></>}
      {s.texProg && <><span className="sep" /><span>{t('texLoading', { d: s.texProg.d, n: s.texProg.n })}</span></>}
      {s.project?.server.mock && <><span className="sep" /><Tip label={t('mockTip')} side="top"><span className="faint">{t('mockServer')}</span></Tip></>}
      <span className="grow" />
      {st && <Tip label={st.gl} side="top"><span className="faint">{st.idle ? t('idle') : `${Math.round(st.fps)} FPS`} · {t('drawsN', { n: st.draws })} · {(st.tris / 1000).toFixed(0)}k △ · {st.gpuTexMB.toFixed(0)} MB</span></Tip>}
      <span className="sep" />
      <span className="faint">{t('quality')}</span>
      <Seg size="xs" value={s.quality} onChange={(q) => actions.setQuality(q)} ariaLabel={t('quality')}
        opts={[{ v: 'high', label: t('q_high'), tip: t('q_high_tip') }, { v: 'mid', label: t('q_mid'), tip: t('q_mid_tip') }, { v: 'low', label: t('q_low'), tip: t('q_low_tip') }]} />
    </footer>
  );
}

export function OfflineBar() {
  const conn = useStore((s) => s.conn); const phase = useStore((s) => s.phase);
  if (conn !== 'offline' || phase === 'fatal') return null;
  return <div className="offline-bar" role="status"><WifiOff size={14} /><span>{t('offlineBar')}</span></div>;
}

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((x) => (
        <div key={x.id} className={'toast ' + x.kind}>
          {x.kind === 'error' ? <AlertTriangle size={16} className="ic" /> : x.kind === 'ok' ? <CheckCircle2 size={16} className="ic" /> : <Info size={16} className="ic" />}
          <div className="tx"><div className="ttl">{x.title}</div>{x.desc && <div className="desc">{x.desc}</div>}</div>
          <button className="ibtn sm" aria-label={t('close')} onClick={() => dismissToast(x.id)}><X size={14} /></button>
        </div>
      ))}
    </div>
  );
}

export function Dialogs() {
  const d = useStore((s) => s.dialog); const close = () => useStore.setState({ dialog: null });
  return (
    <D.Root open={!!d} onOpenChange={(o) => !o && close()}>
      <D.Portal>
        <D.Overlay className="dlg-ov" />
        <D.Content className="dlg" aria-describedby={undefined}>
          {d?.kind === 'uninstall' && <>
            <D.Title className="dlg-t">{t('uninstallTitle')}</D.Title>
            <p className="dlg-d">{t('uninstallDesc')}</p>
            <div className="dlg-a"><button className="btn" onClick={close}>{t('cancel')}</button><button className="btn danger" onClick={() => actions.install('uninstall')}>{t('uninstall')}</button></div>
          </>}
          {d?.kind === 'gameRunning' && <>
            <D.Title className="dlg-t">{t('gameRunningTitle')}</D.Title>
            <p className="dlg-d">{t('gameRunningDesc')}</p>
            <div className="dlg-a"><button className="btn primary" onClick={close}>{t('ok')}</button></div>
          </>}
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}
