import { useEffect, useRef, useState } from 'react';
import * as D from '@radix-ui/react-dialog';
import { libraryApi } from '../lib/api';
import { buildDiffers, createCompatibilityLoader, matchingVerification } from '../lib/compatibility';
import { t } from '../lib/i18n';
import type { CompatibilityInfo, LibraryEntry, VerificationOutcome, VerificationRecord } from '../lib/types';

export function CompatibilitySummary({ entry, targetBuild, onChanged, compact = false }: { entry: LibraryEntry; targetBuild: string | null; onChanged?: () => void; compact?: boolean }) {
  const [open, setOpen] = useState(false);
  const historical = entry.verification;
  return <div className={'compat-summary' + (compact ? ' compact' : '')}>
    {!compact && <>
    <p className="note">{t('compatBuilds', { source: entry.project?.game_build || '—', target: targetBuild || '—' })}</p>
    {buildDiffers(entry.project?.game_build || null, targetBuild) && <p className="variant-warning">{t('compatMismatch')}</p>}
    {historical && <p className="note">{t('compatHistorical')}: {t('verification_' + historical.outcome)} · {historical.build} · <time dateTime={new Date(historical.recorded_unix * 1000).toISOString()}>{new Date(historical.recorded_unix * 1000).toLocaleDateString()}</time><br /><span className="mono" title={historical.revision}>{historical.revision.slice(0, 19)}</span></p>}
    </>}
    <D.Root open={open} onOpenChange={setOpen}>
      <D.Trigger asChild><button className="btn" type="button">{t('compatTitle')}</button></D.Trigger>
      <D.Portal><D.Overlay className="dlg-ov" /><D.Content className="dlg workbench-dialog" aria-describedby="compat-description">
        <D.Title className="dlg-t">{t('compatTitle')} · {entry.display_name}</D.Title>
        <D.Description id="compat-description" className="dlg-d">{t('compatUserOnly')}</D.Description>
        <CompatibilityDetail key={entry.id} entry={entry} targetBuild={targetBuild} onChanged={onChanged} />
        <div className="dlg-a"><D.Close asChild><button className="btn" type="button">{t('close')}</button></D.Close></div>
      </D.Content></D.Portal>
    </D.Root>
  </div>;
}

function CompatibilityDetail({ entry, targetBuild, onChanged }: { entry: LibraryEntry; targetBuild: string | null; onChanged?: () => void }) {
  const [state, setState] = useState<{ data: CompatibilityInfo | null; error: string; loading: boolean }>({ data: null, error: '', loading: true });
  const [loader] = useState(() => createCompatibilityLoader(libraryApi.compatibility, (data, error, loading) => setState({ data, error, loading })));
  const [build, setBuild] = useState(targetBuild || '');
  const [outcome, setOutcome] = useState<VerificationOutcome>('passed');
  const [scene, setScene] = useState(''); const [note, setNote] = useState(''); const [evidence, setEvidence] = useState('');
  const [saving, setSaving] = useState(false); const [saveError, setSaveError] = useState(''); const [notice, setNotice] = useState('');
  const saveController = useRef<AbortController | null>(null);
  useEffect(() => { loader.load(entry.id); return () => { loader.cancel(); saveController.current?.abort(); }; }, [entry.id, loader]);
  const { data, error, loading } = state;
  const matching = data && matchingVerification(data);
  async function save(event: React.FormEvent) {
    event.preventDefault(); if (!data?.current_revision || saving) return;
    const ctl = new AbortController(); saveController.current = ctl;
    setSaving(true); setSaveError(''); setNotice('');
    try {
      await libraryApi.recordVerification(entry.id, { expected_revision: data.current_revision, build: build.trim(), outcome, scene: scene.trim(), note: note.trim(), evidence: evidence.split(/\r?\n/).map((line) => line.trim()).filter(Boolean) }, ctl.signal);
      if (ctl.signal.aborted) return;
      setNotice(t('compatSaved')); onChanged?.(); await loader.load(entry.id);
    } catch (e: any) { if (!ctl.signal.aborted) setSaveError(e.message); }
    finally { if (!ctl.signal.aborted) setSaving(false); }
  }
  return <div className="compat-detail">
    <button className="btn" type="button" disabled={loading || saving} onClick={() => loader.load(entry.id)}>{t('compatRecheck')}</button>
    {loading && <p role="status">{t('compatRead')}</p>}
    {(error || saveError) && <p role="alert" className="library-feedback error">{error || saveError}</p>}
    {notice && <p role="status" className="library-feedback success">{notice}</p>}
    {data && <>
      <p>{t('compatBuilds', { source: data.project_build || '—', target: data.target_build || '—' })}</p>
      <p className="mono workbench-revision">{t('compatRevision')}: {data.current_revision || '—'}</p>
      <p className={'compat-result ' + (matching?.outcome || '')}>{matching ? `${t('compatCurrent')}: ${t('verification_' + matching.outcome)}` : t('compatNotRecorded')}</p>
      {data.warnings.map((warning, i) => <p key={i} className="variant-warning">{warning}</p>)}
      <form className="workbench-form" onSubmit={save}>
        <label>{t('compatBuild')}<input className="input" required maxLength={128} value={build} onChange={(e) => setBuild(e.target.value)} /></label>
        <label>{t('compatScene')}<input className="input" required maxLength={256} placeholder={t('compatSceneHint')} value={scene} onChange={(e) => setScene(e.target.value)} /></label>
        <label>{t('compatOutcome')}<select className="input" value={outcome} onChange={(e) => setOutcome(e.target.value as VerificationOutcome)}>{(['passed', 'issues', 'failed'] as const).map((value) => <option key={value} value={value}>{t('verification_' + value)}</option>)}</select></label>
        <label>{t('compatNote')}<textarea className="input" rows={3} maxLength={4000} value={note} onChange={(e) => setNote(e.target.value)} /></label>
        <label>{t('compatEvidence')}<textarea className="input" rows={2} maxLength={16000} value={evidence} onChange={(e) => setEvidence(e.target.value)} /></label>
        <button className="btn primary" disabled={saving || loading || !data.current_revision || !build.trim() || !scene.trim()}>{saving ? t('libWorking') : t('compatSave')}</button>
      </form>
      <h3>{t('compatHistory')}</h3>
      {data.records.length ? <ol className="verification-list">{[...data.records].reverse().map((record) => <VerificationItem key={record.id} record={record} current={record.id === matching?.id} />)}</ol> : <p className="note">{t('compatNoHistory')}</p>}
    </>}
  </div>;
}

function VerificationItem({ record, current }: { record: VerificationRecord; current: boolean }) {
  return <li><b>{t('verification_' + record.outcome)}</b> · {record.build} · <time dateTime={new Date(record.recorded_unix * 1000).toISOString()}>{new Date(record.recorded_unix * 1000).toLocaleString()}</time>
    <p className="note">{current ? t('compatCurrent') : t('compatOld')}</p><p>{record.scene}</p>{record.note && <p className="workbench-prewrap">{record.note}</p>}
    <p className="mono workbench-revision">{record.revision}</p>{record.evidence.length > 0 && <ul>{record.evidence.map((text, index) => <li key={index} className="workbench-revision">{text}</li>)}</ul>}
  </li>;
}
