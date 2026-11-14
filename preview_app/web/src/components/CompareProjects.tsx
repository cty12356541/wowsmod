import { useEffect, useMemo, useRef, useState } from 'react';
import * as D from '@radix-ui/react-dialog';
import { libraryApi } from '../lib/api';
import { t } from '../lib/i18n';
import { comparableProjects, createComparisonLoader, validPair } from '../lib/project-compare';
import { LibraryThumbnailCache } from '../lib/library-thumbnail';
import { LibraryThumbnail } from './LibraryThumbnail';
import type { ComparisonStatus, LibraryEntry } from '../lib/types';

export function CompareProjects({ entries, initialLeftId, onClose }: { entries: LibraryEntry[]; initialLeftId?: string; onClose: () => void }) {
  const opener = useRef<HTMLElement | null>(document.activeElement instanceof HTMLElement ? document.activeElement : null);
  const [leftId, setLeftId] = useState(initialLeftId || entries.find((entry) => entry.available)?.id || '');
  const [rightId, setRightId] = useState(() => comparableProjects(entries, initialLeftId || entries.find((entry) => entry.available)?.id || '')[0]?.id || '');
  const [state, setState] = useState<{ status: ComparisonStatus | null; error: string; expired: boolean }>({ status: null, error: '', expired: false });
  const [loaded, setLoaded] = useState<string[]>([]); const [imageError, setImageError] = useState(''); const [opening, setOpening] = useState(false); const alive = useRef(true);
  const [covers] = useState(() => new LibraryThumbnailCache((id, signal, refresh) => refresh ? libraryApi.refreshThumbnail(id, signal) : libraryApi.thumbnail(id, signal)));
  const [loader] = useState(() => createComparisonLoader({ create: libraryApi.compareProjects, status: libraryApi.comparison }, (status, error, expired) => { if (alive.current) setState({ status, error, expired }); }));
  const valid = validPair(entries, leftId, rightId);
  const left = entries.find((entry) => entry.id === leftId); const right = entries.find((entry) => entry.id === rightId);
  const candidates = useMemo(() => comparableProjects(entries, leftId), [entries, leftId]);
  useEffect(() => { alive.current = true; return () => { alive.current = false; loader.cancel(); }; }, [loader]);
  useEffect(() => {
    setLoaded([]); setImageError('');
    if (valid) loader.load(leftId, rightId); else { loader.cancel(); setState({ status: null, error: '', expired: false }); }
    return () => loader.cancel();
  }, [valid, leftId, rightId, left?.path, right?.path, loader]);
  const status = state.status;
  const urls = status?.state === 'ready' ? [status.left.url!, status.right.url!] : [];
  const ready = urls.length === 2 && urls.every((url) => loaded.includes(url)) && !imageError;
  function refresh() { setLoaded([]); setImageError(''); loader.load(leftId, rightId, true); }
  async function open(entry: LibraryEntry) { setOpening(true); try { const result = await libraryApi.open(entry.id); if (alive.current) location.assign(result.url); } catch (error: any) { if (alive.current) { setState((s) => ({ ...s, error: error.message })); setOpening(false); } } }
  return <D.Root open onOpenChange={(opened) => !opened && onClose()}><D.Portal><D.Overlay className="dlg-ov" /><D.Content className="dlg workbench-dialog comparison-dialog" aria-describedby="compare-description" onCloseAutoFocus={(event) => { event.preventDefault(); if (opener.current?.isConnected) opener.current.focus(); }}>
    <D.Title className="dlg-t">{t('pairTitle')}</D.Title><D.Description id="compare-description" className="dlg-d">{t('pairHint')}</D.Description>
    <div className="comparison-selectors">
      <label>{t('pairLeft')}<select className="input" value={leftId} onChange={(event) => { const id = event.target.value; setLeftId(id); setRightId(comparableProjects(entries, id)[0]?.id || ''); }}>{entries.map((entry) => <option key={entry.id} value={entry.id} disabled={!entry.available}>{entry.display_name} · {entry.id.slice(-8)}</option>)}</select></label>
      <label>{t('pairRight')}<select className="input" value={rightId} onChange={(event) => setRightId(event.target.value)}><option value="">{t('pairChoose')}</option>{candidates.map((entry) => <option key={entry.id} value={entry.id}>{entry.display_name} · {entry.id.slice(-8)}</option>)}</select></label>
    </div>
    {!candidates.length && <p role="status">{t('pairNoOther')}</p>}
    {(state.error || imageError || status?.reason) && <p className="library-feedback error" role="alert">{state.error || imageError || status?.reason}</p>}
    {valid && <p role="status">{ready ? t('pairSynced') : state.expired ? t('pairExpired') : status?.state === 'failed' || status?.state === 'missing' || imageError || state.error ? t('pairFailed') : t('pairPreparing')}</p>}
    <div className="comparison-images">{[left, right].map((entry, index) => <figure key={entry?.id || index}>
      {entry ? <>
        <div className="comparison-frame">
          {!ready && <LibraryThumbnail project={entry} cache={covers} />}
          {urls[index] && <img className={!ready ? 'comparison-pending-image' : ''} src={urls[index]} width={640} height={360} alt={entry.display_name} onLoad={() => setLoaded((items) => items.includes(urls[index]) ? items : [...items, urls[index]])} onError={() => setImageError(t('pairImageFailed'))} />}
        </div>
        {!ready && <p className="note">{t('pairCoverOnly')}</p>}
        <figcaption><b>{entry.display_name}</b><p className="mono workbench-revision">{entry.id}</p><p className="mono workbench-revision">{entry.path}</p>{ready && <p className="mono workbench-revision">{index === 0 ? status?.left.revision : status?.right.revision}</p>}</figcaption>
        <button className="btn" disabled={opening || !entry.available} onClick={() => open(entry)}>{t('libOpen')}</button>
      </> : <p>{t('pairChoose')}</p>}
    </figure>)}</div>
    {ready && status?.camera && <details className="comparison-recipe"><summary>{t('pairRecipe')}</summary><pre>{JSON.stringify({ camera: status.camera, recipe: status.recipe }, null, 2)}</pre></details>}
    <div className="dlg-a"><button className="btn" disabled={!valid} onClick={refresh}>{t('libRefresh')}</button><D.Close asChild><button className="btn">{t('close')}</button></D.Close></div>
  </D.Content></D.Portal></D.Root>;
}
