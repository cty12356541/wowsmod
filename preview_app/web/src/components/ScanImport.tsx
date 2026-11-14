import { useEffect, useRef, useState, type FormEvent } from 'react';
import { libraryApi } from '../lib/api';
import { t } from '../lib/i18n';
import { createScanLoader, selectedCandidates } from '../lib/library-scan';
import type { ScanPreview } from '../lib/types';

export function ScanImport({ onImported, onClose }: { onImported: (warnings: string[]) => void; onClose: () => void }) {
  const [root, setRoot] = useState(''); const [selected, setSelected] = useState<string[]>([]);
  const [state, setState] = useState<{ data: ScanPreview | null; error: string; loading: boolean }>({ data: null, error: '', loading: false });
  const alive = useRef(true); const importController = useRef<AbortController | null>(null);
  const [loader] = useState(() => createScanLoader(libraryApi.scanPreview, (data, error, loading) => { if (alive.current) setState({ data, error, loading }); }));
  const [importing, setImporting] = useState(false); const [importError, setImportError] = useState(''); const [done, setDone] = useState(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; loader.clear(); importController.current?.abort(); }; }, [loader]);
  function changeRoot(value: string) { setRoot(value); loader.clear(); setSelected([]); setImportError(''); setDone(false); }
  function scan(event: FormEvent) { event.preventDefault(); if (!root.trim() || importing) return; setSelected([]); setImportError(''); setDone(false); loader.load(root.trim()); }
  const selectedIds = selectedCandidates(state.data?.candidates || [], selected);
  async function importSelected() {
    if (!state.data || !selectedIds.length || importing || done) return;
    const ctl = new AbortController(); importController.current = ctl; setImporting(true); setImportError('');
    try {
      const result = await libraryApi.scanImport(state.data.scan_id, selectedIds, ctl.signal);
      if (!alive.current || ctl.signal.aborted) return;
      setDone(true); setSelected([]); onImported(result.warnings || []);
    } catch (error: any) { if (alive.current && !ctl.signal.aborted) setImportError(error.message); }
    finally { if (alive.current && !ctl.signal.aborted) setImporting(false); }
  }
  return <section className="library-path scan-import" aria-label={t('scanTitle')}>
    <form onSubmit={scan}>
      <label htmlFor="scan-root">{t('libScanPath')}</label>
      <div className="scan-path-controls"><input className="input" id="scan-root" autoFocus required disabled={importing} value={root} onChange={(event) => changeRoot(event.target.value)} placeholder="D:\Mods" />
        <button className="btn primary" disabled={importing || !root.trim()}>{state.loading ? t('scanAgain') : t('scanPreview')}</button>
        <button type="button" className="btn" disabled={importing} onClick={onClose}>{t('close')}</button></div>
    </form>
    <p className="note">{t('scanHint')}</p>
    {state.loading && <p role="status">{t('libWorking')}</p>}
    {(state.error || importError) && <p className="library-feedback error" role="alert">{state.error || importError}</p>}
    {state.data && <>
      <p className="note">{t('scanStats', { n: state.data.candidates.length, dirs: state.data.visited_dirs })}</p>
      {state.data.truncated && <p className="variant-warning" role="status">{t('libScanTruncated')}</p>}
      {state.data.warnings.map((warning, index) => <p key={index} className="variant-warning">{warning}</p>)}
      {state.data.candidates.length ? <ul className="scan-candidates">{state.data.candidates.map((candidate) => <li key={candidate.candidate_id}>
        <label><input type="checkbox" disabled={candidate.status !== 'new' || importing || done} checked={selectedIds.includes(candidate.candidate_id)} onChange={(event) => setSelected((ids) => event.target.checked ? [...ids, candidate.candidate_id] : ids.filter((id) => id !== candidate.candidate_id))} />
          <span><b>{candidate.project?.name || candidate.path}</b><span className="library-badge">{t('scanStatus_' + candidate.status)}</span><span className="mono scan-path">{candidate.path}</span>{candidate.existing_id && <span className="mono scan-path">{candidate.existing_id}</span>}</span></label>
        {candidate.warning && <p className="variant-warning">{candidate.warning}</p>}
      </li>)}</ul> : <p>{t('scanNoCandidates')}</p>}
      <button className="btn primary" disabled={!selectedIds.length || importing || done} onClick={importSelected}>{importing ? t('libWorking') : t('scanImportSelected', { n: selectedIds.length })}</button>
      {done && <p className="library-feedback success" role="status">{t('scanDone')}</p>}
    </>}
  </section>;
}
