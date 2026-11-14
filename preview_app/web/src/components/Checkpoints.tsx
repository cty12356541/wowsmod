import { useEffect, useRef, useState, type FormEvent } from 'react';
import * as D from '@radix-ui/react-dialog';
import { libraryApi } from '../lib/api';
import { checkpointChanges, createCheckpointLoader } from '../lib/checkpoints';
import { t } from '../lib/i18n';
import { useStore } from '../store';
import type { Checkpoint, CheckpointDiff, CheckpointList, CheckpointRestore } from '../lib/types';

export function CheckpointsButton({ projectId }: { projectId: string }) {
  return <D.Root><D.Trigger asChild><button className="btn" type="button">{t('checkpointTitle')}</button></D.Trigger><D.Portal><D.Overlay className="dlg-ov" /><D.Content className="dlg workbench-dialog" aria-describedby="checkpoint-library-description"><D.Title className="dlg-t">{t('checkpointTitle')}</D.Title><D.Description id="checkpoint-library-description" className="dlg-d">{t('checkpointHint')}</D.Description><Checkpoints projectId={projectId} /><div className="dlg-a"><D.Close asChild><button className="btn">{t('close')}</button></D.Close></div></D.Content></D.Portal></D.Root>;
}

export function Checkpoints({ projectId }: { projectId: string | null }) {
  if (!projectId) return <section className="checkpoints"><h3>{t('checkpointTitle')}</h3><p className="note">{t('checkpointLibraryHint')}</p><a href="/?library=1">{t('libBack')}</a></section>;
  return <CheckpointHistory key={projectId} projectId={projectId} />;
}
function CheckpointHistory({ projectId }: { projectId: string }) {
  const [state, setState] = useState<{ data: CheckpointList | null; error: string; busy: boolean }>({ data: null, error: '', busy: true });
  const alive = useRef(true); const operation = useRef<AbortController | null>(null);
  const [loader] = useState(() => createCheckpointLoader((signal) => libraryApi.checkpoints(projectId, signal), (data, error, busy) => { if (alive.current) setState({ data, error, busy }); }));
  const [label, setLabel] = useState(''); const [saving, setSaving] = useState(false); const [saveError, setSaveError] = useState(''); const [notice, setNotice] = useState('');
  const [selected, setSelected] = useState<Checkpoint | null>(null); const [polls, setPolls] = useState(0);
  const checkpointOpener = useRef<HTMLButtonElement | null>(null);
  useEffect(() => { alive.current = true; loader.load(); return () => { alive.current = false; loader.cancel(); operation.current?.abort(); }; }, [loader]);
  const pending = state.data?.checkpoints.some((checkpoint) => checkpoint.preview.state === 'pending');
  useEffect(() => {
    if (!pending || polls >= 60 || state.busy || state.error) return;
    const timer = setTimeout(() => { setPolls((value) => value + 1); loader.load(); }, 2000);
    return () => clearTimeout(timer);
  }, [pending, polls, state.busy, state.error, loader]);
  async function create(event: FormEvent) {
    event.preventDefault(); if (saving || !label.trim()) return;
    const ctl = new AbortController(); operation.current = ctl; setSaving(true); setSaveError(''); setNotice('');
    try {
      const current = await libraryApi.sourceRevision(projectId, ctl.signal);
      if (current.project_id !== projectId || !current.revision) throw new Error(t('checkpointSourceMissing'));
      await libraryApi.createCheckpoint(projectId, { label: label.trim(), expected_revision: current.revision }, ctl.signal);
      if (ctl.signal.aborted || !alive.current) return;
      setLabel(''); setNotice(t('checkpointSaved')); setPolls(0); await loader.load();
    } catch (error: any) { if (alive.current && !ctl.signal.aborted) setSaveError(error.message); }
    finally { if (alive.current && !ctl.signal.aborted) setSaving(false); }
  }
  const selectedCurrent = selected && (state.data?.checkpoints.find((checkpoint) => checkpoint.id === selected.id) || selected);
  return <section className="checkpoints">
    <div className="checkpoint-heading"><h3>{t('checkpointTitle')}</h3><button className="btn ghost" disabled={state.busy || saving} onClick={() => { setPolls(0); loader.load(); }}>{t('libRefresh')}</button></div>
    <p className="note">{t('checkpointHint')}</p>
    <form className="checkpoint-create" onSubmit={create}><label><span className="sr">{t('checkpointLabel')}</span><input className="input" required maxLength={128} value={label} onChange={(event) => setLabel(event.target.value)} placeholder={t('checkpointLabel')} disabled={saving} /></label><button className="btn primary" disabled={saving || !label.trim()}>{saving ? t('libWorking') : t('checkpointSave')}</button></form>
    {(state.error || saveError) && <p role="alert" className="variant-warning">{state.error || saveError}</p>}
    {notice && <p role="status">{notice}</p>}
    {state.busy && !state.data && <p role="status">{t('loadingShort')}</p>}
    {state.data && <>
      <details className="checkpoint-scope"><summary>{t('checkpointScope', { v: state.data.scope_version })}</summary><p>{t('checkpointScopeHint')}</p><p className="mono workbench-revision">{state.data.excluded_roots.join(' · ')}</p></details>
      {state.data.checkpoints.length ? <ul className="checkpoint-list">{state.data.checkpoints.map((checkpoint) => <li key={checkpoint.id}><button type="button" className="checkpoint-open" onClick={(event) => { checkpointOpener.current = event.currentTarget; setSelected(checkpoint); }}><b>{checkpoint.label}</b><time dateTime={new Date(checkpoint.created_unix * 1000).toISOString()}>{new Date(checkpoint.created_unix * 1000).toLocaleString()}</time><span className="note">{t('checkpointFiles', { n: checkpoint.file_count, mb: (checkpoint.total_bytes / 1048576).toFixed(1) })}</span><span className="mono">{checkpoint.revision.slice(0, 19)}</span></button></li>)}</ul> : <p className="note">{t('checkpointEmpty')}</p>}
      {pending && polls >= 60 && <p className="note">{t('checkpointPreviewPending')}</p>}
    </>}
    {selectedCurrent && <CheckpointDetail key={selectedCurrent.id} projectId={projectId} checkpoint={selectedCurrent} onClose={() => setSelected(null)} returnFocus={() => { if (checkpointOpener.current?.isConnected) checkpointOpener.current.focus(); }} />}
  </section>;
}

function CheckpointDetail({ projectId, checkpoint, onClose, returnFocus }: { projectId: string; checkpoint: Checkpoint; onClose: () => void; returnFocus: () => void }) {
  const [diff, setDiff] = useState<CheckpointDiff | null>(null); const [error, setError] = useState(''); const [busy, setBusy] = useState(false);
  const [destination, setDestination] = useState(''); const [restored, setRestored] = useState<CheckpointRestore | null>(null); const [imageFailed, setImageFailed] = useState(false);
  const ctl = useRef<AbortController | null>(null); const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; ctl.current?.abort(); }; }, []);
  async function inspect() {
    const controller = new AbortController(); ctl.current?.abort(); ctl.current = controller; setBusy(true); setError('');
    try { const value = await libraryApi.checkpointDiff(projectId, checkpoint.id, controller.signal); if (alive.current && !controller.signal.aborted) setDiff(value); }
    catch (e: any) { if (alive.current && !controller.signal.aborted) setError(e.message); }
    finally { if (alive.current && !controller.signal.aborted) setBusy(false); }
  }
  async function restore(event: FormEvent) {
    event.preventDefault(); if (!destination.trim() || busy) return;
    const controller = new AbortController(); ctl.current?.abort(); ctl.current = controller; setBusy(true); setError('');
    try {
      const result = await libraryApi.restoreCheckpoint(projectId, checkpoint.id, { destination: destination.trim(), expected_revision: checkpoint.revision }, controller.signal);
      if (!alive.current || controller.signal.aborted) return;
      setRestored(result); if (result.entry) useStore.setState({ catalogEpoch: useStore.getState().catalogEpoch + 1 });
    } catch (e: any) { if (alive.current && !controller.signal.aborted) setError(e.message); }
    finally { if (alive.current && !controller.signal.aborted) setBusy(false); }
  }
  return <D.Root open onOpenChange={(open) => !open && !busy && onClose()}><D.Portal><D.Overlay className="dlg-ov" /><D.Content className="dlg workbench-dialog" aria-describedby="checkpoint-description" onCloseAutoFocus={(event) => { event.preventDefault(); returnFocus(); }}>
    <D.Title className="dlg-t">{checkpoint.label}</D.Title><D.Description id="checkpoint-description" className="dlg-d">{t('checkpointFrozen')}</D.Description>
    {checkpoint.preview.state === 'ready' && checkpoint.preview.url && !imageFailed ? <img className="checkpoint-preview" src={checkpoint.preview.url} width={640} height={360} alt={checkpoint.label} onError={() => setImageFailed(true)} /> : <p className="note">{t(checkpoint.preview.state === 'pending' ? 'checkpointPreviewPending' : 'checkpointPreviewFailed')}{checkpoint.preview.reason ? ` · ${checkpoint.preview.reason}` : ''}</p>}
    <p className="mono workbench-revision">{checkpoint.revision}</p><p>{t('compatBuilds', { source: checkpoint.project_build || '—', target: checkpoint.target_build || '—' })}</p><p>{t('checkpointFiles', { n: checkpoint.file_count, mb: (checkpoint.total_bytes / 1048576).toFixed(1) })}</p>
    <button className="btn" disabled={busy} onClick={inspect}>{t('checkpointDiff')}</button>
    {diff && <div className="checkpoint-diff"><p>{t('checkpointChanges', { n: checkpointChanges(diff), unchanged: diff.unchanged_count })}</p><p className="mono workbench-revision">{t('checkpointCurrent')}: {diff.current_revision}</p>{(['added', 'removed', 'changed'] as const).map((kind) => <details key={kind}><summary>{t('checkpoint_' + kind)} · {diff[kind].length}</summary><ul>{diff[kind].map((path) => <li key={path} className="mono workbench-revision">{path}</li>)}</ul></details>)}</div>}
    <form className="workbench-form" onSubmit={restore}><h3>{t('checkpointRestore')}</h3><p className="note">{t('checkpointRestoreHint')}</p><label>{t('checkpointDestination')}<input className="input" required disabled={busy || !!restored} value={destination} onChange={(event) => setDestination(event.target.value)} placeholder="D:\Mods\NewVariant" /></label><button className="btn primary" disabled={busy || !!restored || !destination.trim()}>{busy ? t('libWorking') : t('checkpointRestore')}</button></form>
    {error && <p className="library-feedback error" role="alert">{error}</p>}
    {restored && <div className="library-feedback success checkpoint-restored" role="status"><p>{t(restored.entry ? 'checkpointRestored' : 'checkpointRegisterFailed')}</p><p className="mono workbench-revision">{restored.restored_path || restored.entry?.path || destination}</p>{restored.warnings.map((warning, index) => <p key={index}>{warning}</p>)}{restored.url && <a className="btn" href={restored.url}>{t('libOpen')}</a>}</div>}
    <div className="dlg-a"><D.Close asChild><button className="btn" disabled={busy}>{t('close')}</button></D.Close></div>
  </D.Content></D.Portal></D.Root>;
}
