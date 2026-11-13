import { Download, Languages, MoreHorizontal, Loader2, Gamepad2 } from 'lucide-react';
import { useStore, actions, capabilityReason } from '../store';
import { t, langName, type Lang } from '../lib/i18n';
import { IconBtn, Menu, Seg, Tip } from './ui';

export function TitleBar() {
  const s = useStore();
  const p = s.project; const lang = s.lang;
  // 项目名取自 mod.toml [mod].name; 目录名(跳过 proj/project/mod 这类泛称, 取上一级)不同时一并显示, 便于区分从同一模板复制的项目
  const dirParts = (p?.project.dir || '').split(/[\\/]+/).filter(Boolean);
  const generic = /^(proj|project|mod|src|work)$/i;
  const dirTail = dirParts.length ? (generic.test(dirParts[dirParts.length - 1]) && dirParts.length > 1 ? dirParts.slice(-3, -1).join('/') : dirParts[dirParts.length - 1]) : '';
  const showDir = !!dirTail && dirTail.toLowerCase() !== (p?.project.name || '').toLowerCase();
  const shipName = p ? p.ship.name?.[lang] || p.ship.name?.en || p.ship.code || t('libGlobal') : '';
  const inst = p?.install; const job = s.job;
  const busy = !!job && job.state === 'running';
  const offline = s.conn === 'offline';
  const canApply = !!p && p.capabilities?.apply !== false;
  const instLabel = busy ? (job!.kind === 'uninstall' ? t('uninstalling') : t('installing')) + (job!.progress ? ` ${Math.round(job!.progress * 100)}%` : '') : t('libApply');
  const instTip = !canApply ? capabilityReason('apply') : inst?.state === 'installed' ? t('installInfo', { n: s.steps.find((x) => x.id === inst.rev)?.index ?? inst.rev ?? '?', files: inst.files })
    : inst?.state === 'outdated' ? t('installOutdated') : inst?.state === 'unknown' ? t('libInstall_unknown') : t('notInstalled');
  return (
    <header className="titlebar">
      <div className="brand">
        <div className="brand-mark" aria-hidden>wm</div>
        <span className="brand-name">{t('appName')}</span>
      </div>
      {p && (
        <div className="crumb">
          <span className="sep">/</span>
          <Tip label={<>{t('projDir')}：{p.project.dir}</>}><b>{p.project.name}</b></Tip>
          {showDir && <span className="faint">({dirTail})</span>}
          <span className="sep">·</span>
          <span>{shipName}{(p.ship.level != null || p.ship.species) && <> <span className="faint">{[p.ship.level != null ? t('tierShort', { n: p.ship.level }) : null, p.ship.species ? t('species_' + p.ship.species) : null].filter(Boolean).join(' · ')}</span></>}</span>
        </div>
      )}
      <div className="tb-center">
        <Seg size="ws" ariaLabel={t('workspace')} value={s.workspace} onChange={(w) => actions.setWorkspace(w)}
          opts={[{ v: 'review', label: t('wsReview'), tip: t('wsReviewTip'), kbd: 'Tab' }, { v: 'sim', label: t('wsSim'), tip: p?.capabilities?.simulation === false ? capabilityReason('simulation') : s.sim ? t('wsSimTip') : t('simUnavailable'), disabled: !s.sim || p?.capabilities?.simulation === false, kbd: 'Tab' }]} />
      </div>
      <div className="tb-right">
        {p?.activity?.state === 'working' && <span className="tb-status"><Loader2 size={12} className="spin" />{t('stWorking')}</span>}
        {offline && <span className="tb-status err">{t('stOffline')}</span>}
        {p?.game.running && <Tip label={t('gameRunningHint')}><span className="tb-status"><Gamepad2 size={14} />{t('gameRunning')}</span></Tip>}
        {inst && <span className="tb-status" title={instTip}>{t('libInstall_' + inst.state)}</span>}
        <div className="vsep" />
        <div className="btn-group">
          <Tip label={instTip}>
            <button title={instTip} className={'btn' + (inst?.state === 'outdated' || inst?.state === 'not_installed' ? ' primary' : '')} disabled={busy || s.applyBusy || offline || !canApply} onClick={() => actions.prepareApply()}>
              {busy ? <Loader2 size={14} className="spin" /> : <Download size={14} />}{instLabel}
            </button>
          </Tip>
          <Menu align="end" trigger={<button className={'btn' + (inst?.state === 'outdated' || inst?.state === 'not_installed' ? ' primary' : '')} aria-label={t('more')} disabled={busy || offline}><MoreHorizontal size={14} /></button>}
            items={[{ label: t('reinstall'), onSelect: () => actions.prepareApply(), disabled: !canApply }, { label: t('uninstall'), onSelect: () => useStore.setState({ dialog: { kind: 'uninstall' } }), disabled: !canApply || inst?.state === 'not_installed' || inst?.state === 'unknown' }]} />
        </div>
        <Menu align="end" trigger={<IconBtn icon={<Languages size={16} />} label={t('language')} />}
          items={(['zh', 'en', 'ru'] as Lang[]).map((l) => ({ label: langName(l), checked: l === lang, onSelect: () => actions.setLang(l) }))} />
      </div>
    </header>
  );
}
