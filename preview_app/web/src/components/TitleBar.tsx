import { Download, Languages, MoreHorizontal, Loader2, Gamepad2 } from 'lucide-react';
import { useStore, actions } from '../store';
import { t, langName, type Lang } from '../lib/i18n';
import { IconBtn, Menu, Seg, Tip } from './ui';

export function TitleBar() {
  const s = useStore();
  const p = s.project; const lang = s.lang;
  const shipName = p ? p.ship.name[lang] || p.ship.name.en || p.ship.code : '';
  const head = s.steps.find((x) => x.id === s.head);
  const inst = p?.install; const job = s.job;
  const busy = !!job && job.state === 'running';
  const offline = s.conn === 'offline';
  const instLabel = busy ? (job!.kind === 'install' ? t('installing') : t('uninstalling')) + (job!.progress ? ` ${Math.round(job!.progress * 100)}%` : '')
    : inst?.state === 'installed' ? t('installed') : inst?.state === 'outdated' ? t('installUpdate') : t('install');
  const instTip = inst?.state === 'installed' ? t('installInfo', { n: s.steps.find((x) => x.id === inst.rev)?.index ?? '?', files: inst.files })
    : inst?.state === 'outdated' ? t('installOutdated') : t('notInstalled');
  return (
    <header className="titlebar">
      <div className="brand">
        <div className="brand-mark" aria-hidden>wm</div>
        <span className="brand-name">{t('appName')}</span>
      </div>
      {p && (
        <div className="crumb">
          <span className="sep">/</span>
          <b>{p.project.name}</b>
          <span className="sep">·</span>
          <span>{shipName}{(p.ship.level != null || p.ship.species) && <> <span className="faint">{[p.ship.level != null ? t('tierShort', { n: p.ship.level }) : null, p.ship.species ? t('species_' + p.ship.species) : null].filter(Boolean).join(' · ')}</span></>}</span>
        </div>
      )}
      <div className="tb-center">
        <Seg size="ws" ariaLabel={t('workspace')} value={s.workspace} onChange={(w) => actions.setWorkspace(w)}
          opts={[{ v: 'review', label: t('wsReview'), tip: t('wsReviewTip'), kbd: 'Tab' }, { v: 'sim', label: t('wsSim'), tip: s.sim ? t('wsSimTip') : t('simUnavailable'), disabled: !s.sim, kbd: 'Tab' }]} />
      </div>
      <div className="tb-right">
        {p?.activity?.state === 'working' && <span className="tb-status"><Loader2 size={12} className="spin" />{t('stWorking')}</span>}
        {offline && <span className="tb-status err">{t('stOffline')}</span>}
        {p?.game.running && <Tip label={t('gameRunningHint')}><span className="tb-status"><Gamepad2 size={14} />{t('gameRunning')}</span></Tip>}
        <div className="vsep" />
        <div className="btn-group">
          <Tip label={instTip}>
            <button className={'btn' + (inst?.state === 'outdated' || inst?.state === 'not_installed' ? ' primary' : '')} disabled={busy || offline || !head} onClick={() => actions.install('install')}>
              {busy ? <Loader2 size={14} className="spin" /> : <Download size={14} />}{instLabel}
            </button>
          </Tip>
          <Menu align="end" trigger={<button className={'btn' + (inst?.state === 'outdated' || inst?.state === 'not_installed' ? ' primary' : '')} aria-label={t('more')} disabled={busy || offline}><MoreHorizontal size={14} /></button>}
            items={[{ label: t('reinstall'), onSelect: () => actions.install('install'), disabled: !head }, { label: t('uninstall'), onSelect: () => useStore.setState({ dialog: { kind: 'uninstall' } }), disabled: inst?.state === 'not_installed' }]} />
        </div>
        <Menu align="end" trigger={<IconBtn icon={<Languages size={16} />} label={t('language')} />}
          items={(['zh', 'en', 'ru'] as Lang[]).map((l) => ({ label: langName(l), checked: l === lang, onSelect: () => actions.setLang(l) }))} />
      </div>
    </header>
  );
}
