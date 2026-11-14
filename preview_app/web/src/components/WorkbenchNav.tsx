import { CompareProjects } from './CompareProjects';
import { CompatibilitySummary } from './CompatibilitySummary';
import { useEffect, useState } from 'react';
import { ArrowLeft } from 'lucide-react';
import { libraryApi } from '../lib/api';
import { shipKey, shipLabel, libraryRefreshKey } from '../lib/library';
import type { LibraryEntry } from '../lib/types';
import { useStore } from '../store';
import { t } from '../lib/i18n';
import { installLabel } from './Library';

export function WorkbenchNav() {
  const project = useStore((s) => s.project); const lang = useStore((s) => s.lang); void lang;
  const catalogEpoch = useStore((s) => s.catalogEpoch);
  const id = new URLSearchParams(location.search).get('project');
  const [entries, setEntries] = useState<LibraryEntry[]>([]);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [compareOpen, setCompareOpen] = useState(false);
  const hasLibrary = !!id || project?.capabilities?.library === true;
  const refreshKey = libraryRefreshKey(project, catalogEpoch);
  useEffect(() => {
    if (!hasLibrary || !project) return;
    let alive = true;
    libraryApi.list().then((data) => { if (alive) { setEntries(data.projects); setError(''); } }).catch((e) => { if (alive) setError(e.message); });
    return () => { alive = false; };
  }, [hasLibrary, refreshKey]);
  if (!hasLibrary) return null;
  const current = entries.find((entry) => entry.id === id);
  const variants = current ? entries.filter((entry) => shipKey(entry) === shipKey(current)).map((entry) => entry.id === id && project ? { ...entry, install: project.install } : entry) : [];
  async function navigate(value: string) {
    if (!value || value === id) return;
    setBusy(true); setError('');
    try { const result = await libraryApi.open(value); location.assign(result.url); }
    catch (e: any) { setError(e.message); setBusy(false); }
  }
  return <nav className="workbench-nav" aria-label={t('libBreadcrumb')}><a href="/?library=1"><ArrowLeft size={12} /> {t('libBack')}</a>{current && <><span className="faint">/</span><span>{shipLabel(current, t)}</span><span className="faint">/</span><b>{current.display_name}</b><span className="grow" /><label><span className="sr">{t('libSwitch')}</span><select aria-label={t('libSwitch')} value={id || ''} disabled={busy} onChange={(e) => navigate(e.target.value)}>{variants.map((entry) => <option key={entry.id} value={entry.id} disabled={!entry.available}>{entry.display_name} · {installLabel(entry.install.state)} · {entry.id.slice(-8)}</option>)}</select></label></>}{current && <button className="btn" onClick={() => setCompareOpen(true)}>{t('pairTitle')}</button>}{compareOpen && <CompareProjects entries={variants} initialLeftId={id || undefined} onClose={() => setCompareOpen(false)} />}{current && <CompatibilitySummary compact entry={current} targetBuild={project?.game.build || null} onChanged={() => useStore.setState({ catalogEpoch: useStore.getState().catalogEpoch + 1 })} />}{error && <span className="variant-warning" role="alert">{error}</span>}</nav>;
}
