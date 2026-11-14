import { useEffect, useLayoutEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import * as D from '@radix-ui/react-dialog';
import { ArrowLeft, ArrowRight, FolderPlus, FolderSearch, RefreshCw, Search, Ship, Star, X, Settings2, AlertTriangle, ChevronDown } from 'lucide-react';
import { libraryApi } from '../lib/api';
import { defaultFilters, facetValues, filterProjects, groupProjects, shipLabel, type LibraryFilters } from '../lib/library';
import { useStore, actions } from '../store';
import type { LibraryEntry, LibraryStatus, LibrarySummary } from '../lib/types';
import { t } from '../lib/i18n';

const storageKey = 'wm.library.v1';
interface SavedView { filters: LibraryFilters; selected: string | null; scroll: number }
function readView(): SavedView {
  try {
    const saved = JSON.parse(sessionStorage.getItem(storageKey) || '{}');
    const filters = { ...defaultFilters, ...saved.filters };
    for (const key of ['nations', 'levels', 'species', 'statuses'] as const) if (!Array.isArray(filters[key])) filters[key] = [];
    if (typeof filters.search !== 'string') filters.search = '';
    if (!['opened', 'added', 'name'].includes(filters.sort)) filters.sort = 'opened';
    return { filters, selected: typeof saved.selected === 'string' ? saved.selected : null, scroll: Number(saved.scroll) || 0 };
  } catch { return { filters: { ...defaultFilters }, selected: null, scroll: 0 }; }
}
function saveView(view: SavedView) { try { sessionStorage.setItem(storageKey, JSON.stringify(view)); } catch { /* Storage may be unavailable. */ } }
const productionLabel = (status: LibraryStatus) => t('libStatus_' + status);
export const installLabel = (state: string) => t('libInstall_' + state) || t('libInstall_unknown');
const metadata = (entry: LibraryEntry) => [entry.ship_metadata.nation ? t('nation_' + entry.ship_metadata.nation) || entry.ship_metadata.nation : t('libUnknown'), entry.ship_metadata.level == null ? null : t('tierShort', { n: entry.ship_metadata.level }), entry.ship_metadata.species ? t('species_' + entry.ship_metadata.species) || entry.ship_metadata.species : null].filter(Boolean).join(' · ');
const pathTail = (path: string) => path.split(/[\\/]/).filter(Boolean).slice(-2).join(' / ');

export function Library() {
  const lang = useStore((s) => s.lang); void lang;
  const [saved] = useState(readView);
  const [filters, setFilters] = useState(saved.filters);
  const [selected, setSelected] = useState<string | null>(saved.selected);
  const [data, setData] = useState<LibrarySummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [pathMode, setPathMode] = useState<'add' | 'scan' | null>(null);
  const [path, setPath] = useState('');
  const [edit, setEdit] = useState<null | { entry: LibraryEntry; kind: 'edit' | 'relocate' | 'remove' }>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const alive = useRef(true);
  const restored = useRef(false);
  const view = useRef<SavedView>(saved);
  view.current = { filters, selected, scroll: view.current.scroll };
  const persist = () => { view.current.scroll = scroll.current?.scrollTop || 0; saveView(view.current); };
  async function load() {
    setLoading(true); setError('');
    try { const result = await libraryApi.list(); if (alive.current) setData(result); }
    catch (e: any) { if (alive.current) setError(e.message); }
    finally { if (alive.current) setLoading(false); }
  }
  useEffect(() => { alive.current = true; document.title = t('libTitle') + ' · wowsmod'; load(); return () => { alive.current = false; saveView(view.current); }; }, []);
  useEffect(() => { saveView(view.current); }, [filters, selected]);
  useLayoutEffect(() => { if (data && !restored.current && scroll.current) { scroll.current.scrollTop = saved.scroll; restored.current = true; } }, [data, saved]);
  const visible = useMemo(() => filterProjects(data?.projects || [], filters), [data, filters]);
  const groups = useMemo(() => groupProjects(visible), [visible]);
  const current = groups.find((g) => g.key === selected);
  const allCurrent = selected ? (data?.projects || []).filter((p) => groupProjects([p])[0].key === selected) : [];
  const patchFilters = (patch: Partial<LibraryFilters>) => { setFilters((f) => ({ ...f, ...patch })); if (scroll.current) scroll.current.scrollTop = 0; };
  async function mutate(operation: () => Promise<any>, message: string) {
    setBusy(true); setError(''); setNotice('');
    try { const result = await operation(); if (!alive.current) return; await load(); setNotice([message, ...(result.warnings || []), result.truncated ? t('libScanTruncated') : ''].filter(Boolean).join(' · ')); setEdit(null); }
    catch (e: any) { if (alive.current) setError(e.message); }
    finally { if (alive.current) setBusy(false); }
  }
  async function open(entry: LibraryEntry) {
    setBusy(true); setError(''); persist();
    try { const result = await libraryApi.open(entry.id); location.assign(result.url); }
    catch (e: any) { if (alive.current) { setError(e.message); setBusy(false); } }
  }
  function submitPath(e: FormEvent) {
    e.preventDefault(); const target = path.trim(); if (!target) return;
    mutate(() => pathMode === 'scan' ? libraryApi.scan(target) : libraryApi.add(target), pathMode === 'scan' ? t('libScanDone') : t('libAdded'));
  }
  return <div className="library" lang={lang === 'zh' ? 'zh-CN' : lang}>
    <header className="titlebar library-header"><a className="brand library-brand" href="/?library=1"><span className="brand-mark" aria-hidden>wm</span><span className="brand-name">wowsmod</span></a><div className="library-header-actions"><select className="input library-select" aria-label={t('language')} value={lang} onChange={(e) => actions.setLang(e.target.value as typeof lang)}><option value="zh">中文</option><option value="en">English</option><option value="ru">Русский</option></select><button className="btn" disabled={loading || busy} onClick={load}><RefreshCw size={14} className={loading ? 'spin' : ''} />{t('libRefresh')}</button></div></header>
    <div className="library-scroll" ref={scroll} onScroll={() => { view.current.scroll = scroll.current?.scrollTop || 0; saveView(view.current); }}>
      <main className="library-content">
        <section className="library-heading"><div><h1>{t('libTitle')}</h1><p>{t('libIntro')}</p></div><div className="library-actions"><button className="btn" aria-expanded={pathMode === 'scan'} onClick={() => { setPathMode(pathMode === 'scan' ? null : 'scan'); setPath(''); }}><FolderSearch size={14} />{t('libScan')}</button><button className="btn primary" aria-expanded={pathMode === 'add'} onClick={() => { setPathMode(pathMode === 'add' ? null : 'add'); setPath(''); }}><FolderPlus size={14} />{t('libAdd')}</button></div></section>
        {pathMode && <form className="library-path" onSubmit={submitPath}><label htmlFor="library-path">{pathMode === 'scan' ? t('libScanPath') : t('libProjectPath')}</label><div><input className="input" autoFocus id="library-path" value={path} onChange={(e) => setPath(e.target.value)} placeholder="D:\\Mods\\MyProject" required /><button className="btn primary" disabled={busy || !path.trim()}>{busy ? t('libWorking') : pathMode === 'scan' ? t('libScan') : t('libAdd')}</button><button type="button" className="btn" onClick={() => setPathMode(null)}>{t('cancel')}</button></div><p>{pathMode === 'scan' ? t('libScanHint') : t('libAddHint')}</p></form>}
        {error && <div role="alert" className="library-feedback error"><AlertTriangle size={14} /><span>{error}</span><button className="ibtn" aria-label={t('close')} onClick={() => setError('')}><X size={14} /></button></div>}
        {notice && <div role="status" className="library-feedback success">{notice}</div>}
        {!!data?.warnings?.length && <details className="library-warnings"><summary><AlertTriangle size={14} />{t('libWarnings', { n: data.warnings.length })}</summary><ul>{data.warnings.map((warning, i) => <li key={i}>{warning}</li>)}</ul></details>}
        <div className="library-client"><span className={'dot ' + (data?.game.available ? 'ok' : 'warn')} /><span>{data?.game.available ? `${t('libClient')} ${data.game.version || ''} · ${t('libBuild')} ${data.game.build || '—'}` : t('libNoClient')}</span>{data?.game.running && <b>{t('gameRunning')}</b>}<span className="grow" /><span>{t('libCount', { ships: groupProjects(data?.projects || []).length, n: data?.projects.length || 0 })}</span></div>
        <section className="library-filters" aria-label={t('libFilters')}><label className="input-wrap library-search"><Search size={14} /><span className="sr">{t('libSearch')}</span><input className="input" type="search" placeholder={t('libSearch')} value={filters.search} onChange={(e) => patchFilters({ search: e.target.value })} /></label><div className="library-facet-row">
          <Facet title={t('libNation')} values={facetValues(data?.projects || [], 'nation')} selected={filters.nations} label={(v) => v === 'unknown' ? t('libUnknown') : t('nation_' + v) || v} onChange={(nations) => patchFilters({ nations })} />
          <Facet title={t('libTier')} values={facetValues(data?.projects || [], 'level')} selected={filters.levels} label={(v) => v === 'unknown' ? t('libUnknown') : t('tierShort', { n: v })} onChange={(levels) => patchFilters({ levels })} />
          <Facet title={t('libClass')} values={facetValues(data?.projects || [], 'species')} selected={filters.species} label={(v) => v === 'unknown' ? t('libUnknown') : t('species_' + v) || v} onChange={(species) => patchFilters({ species })} />
          <Facet title={t('libProduction')} values={['working', 'review', 'complete', 'archived']} selected={filters.statuses} label={(v) => productionLabel(v as LibraryStatus)} onChange={(statuses) => patchFilters({ statuses })} />
          <button className="btn favorite-filter" aria-pressed={filters.favorites} onClick={() => patchFilters({ favorites: !filters.favorites })}><Star size={14} fill={filters.favorites ? 'currentColor' : 'none'} />{t('libFavorites')}</button><span className="grow" /><label className="library-sort"><span className="sr">{t('libSort')}</span><select className="input library-select" value={filters.sort} onChange={(e) => patchFilters({ sort: e.target.value as LibraryFilters['sort'] })}><option value="opened">{t('libSortOpened')}</option><option value="added">{t('libSortAdded')}</option><option value="name">{t('libSortName')}</option></select></label><button className="btn ghost" onClick={() => patchFilters({ ...defaultFilters })}>{t('libReset')}</button>
        </div><p className="note library-filter-hint">{t('libFilterHint')}</p></section>
        {selected && <nav className="library-breadcrumb" aria-label={t('libBreadcrumb')}><button className="btn ghost" onClick={() => { setSelected(null); if (scroll.current) scroll.current.scrollTop = 0; }}><ArrowLeft size={14} />{t('libAllShips')}</button><span aria-hidden>/</span><b>{allCurrent[0] ? shipLabel(allCurrent[0], t) : selected}</b></nav>}
        {loading && !data ? <div className="library-empty" role="status"><RefreshCw size={16} className="spin" /><h2>{t('libLoading')}</h2></div> : !data?.projects.length ? <div className="library-empty"><FolderPlus size={20} /><h2>{error ? t('libUnavailable') : t('libEmpty')}</h2><p>{t('libEmptyHint')}</p><button className="btn primary" onClick={() => setPathMode('add')}>{t('libAdd')}</button></div> : !visible.length || (selected && !current) ? <div className="library-empty"><Search size={20} /><h2>{t('libNoResults')}</h2><p>{t('libNoResultsHint')}</p><button className="btn" onClick={() => { setSelected(null); patchFilters({ ...defaultFilters }); }}>{t('libReset')}</button></div> : selected && current ? <section className="library-variants" aria-label={t('libVariants')}><div className="library-section-title"><h2>{shipLabel(current.projects[0], t)}</h2><span>{metadata(current.projects[0])} · {t('libVariantsN', { n: current.projects.length })}</span></div><div className="variant-grid">{current.projects.map((entry) => <Variant key={entry.id} entry={entry} busy={busy} onOpen={() => open(entry)} onEdit={() => setEdit({ entry, kind: 'edit' })} onRelocate={() => setEdit({ entry, kind: 'relocate' })} onRemove={() => setEdit({ entry, kind: 'remove' })} onFavorite={() => mutate(() => libraryApi.update(entry.id, { favorite: !entry.favorite }), t('libSaved'))} />)}</div></section> : <section className="ship-grid" aria-label={t('libShips')}>{groups.map((group) => {
          const first = group.projects[0]; const installed = group.projects.filter((p) => p.install.state === 'installed' || p.install.state === 'outdated');
          return <button key={group.key} className="ship-card" onClick={() => { setSelected(group.key); if (scroll.current) scroll.current.scrollTop = 0; }}><div className="ship-card-symbol" aria-hidden><Ship size={18} /></div><div className="ship-card-body"><span className="ship-card-code mono">{group.key === 'global' ? t('libGlobal') : group.key}</span><h2>{shipLabel(first, t)}</h2><p>{metadata(first)}</p><div className="ship-card-foot"><span>{t('libVariantsN', { n: group.projects.length })}</span><span className={'library-badge ' + (installed.length ? 'installed' : '')}>{installed.length ? t('libAppliedN', { n: installed.length }) : t('libInstall_not_installed')}</span><ArrowRight size={14} /></div></div></button>;
        })}</section>}
      </main><footer className="library-footer">{t('libFooter')}</footer>
    </div>
    <LibraryEditor edit={edit} busy={busy} error={error} onClose={() => setEdit(null)} onSave={(patch) => edit && mutate(() => libraryApi.update(edit.entry.id, patch), t('libSaved'))} onRelocate={(newPath) => edit && mutate(() => libraryApi.relocate(edit.entry.id, newPath), t('libRelocated'))} onRemove={() => edit && mutate(() => libraryApi.remove(edit.entry.id), t('libRemoved'))} />
  </div>;
}

function Facet({ title, values, selected, label, onChange }: { title: string; values: string[]; selected: string[]; label: (value: string) => string; onChange: (values: string[]) => void }) {
  return <details className="library-facet"><summary className="btn">{title}{selected.length > 0 && <span className="count">{selected.length}</span>}<ChevronDown size={12} aria-hidden /></summary><fieldset className="menu"><legend className="sr">{title}</legend>{values.length ? values.map((value) => <label key={value}><input type="checkbox" checked={selected.includes(value)} onChange={(e) => onChange(e.target.checked ? [...selected, value] : selected.filter((v) => v !== value))} />{label(value)}</label>) : <span>{t('libNoValues')}</span>}<button type="button" className="btn ghost" onClick={() => onChange([])}>{t('libClear')}</button></fieldset></details>;
}
function Variant({ entry, busy, onOpen, onEdit, onRelocate, onRemove, onFavorite }: { entry: LibraryEntry; busy: boolean; onOpen: () => void; onEdit: () => void; onRelocate: () => void; onRemove: () => void; onFavorite: () => void }) {
  return <article className={'variant-card' + (!entry.available ? ' unavailable' : '')}><div className="variant-head"><span className={'library-badge status-' + entry.status}>{productionLabel(entry.status)}</span><span className={'library-badge ' + entry.install.state}>{installLabel(entry.install.state)}</span><button className="ibtn" aria-label={entry.favorite ? t('libUnfavorite') : t('libFavorite')} aria-pressed={entry.favorite} disabled={busy} onClick={onFavorite}><Star size={14} fill={entry.favorite ? 'currentColor' : 'none'} /></button></div><h3>{entry.display_name || entry.project?.name || entry.id}</h3><p className="variant-version">{entry.project?.version ? 'v' + entry.project.version : '—'} · {entry.project?.mode || t('libUnknown')} · {t('libBuild')} {entry.project?.game_build || '—'}</p><div className="variant-tags">{entry.tags.map((tag) => <span key={tag}>#{tag}</span>)}</div><dl><dt>{t('libFolder')}</dt><dd title={entry.path}>{pathTail(entry.path)}</dd><dt>{t('libLastOpened')}</dt><dd>{entry.opened_unix ? new Date(entry.opened_unix * 1000).toLocaleString() : t('libNeverOpened')}</dd><dt>ID</dt><dd className="mono" title={entry.id}>{entry.id}</dd></dl>{!entry.available && <div className="variant-warning"><AlertTriangle size={14} />{t('libLostPath')}</div>}{entry.ship_metadata.warning && <p className="variant-warning">{entry.ship_metadata.warning}</p>}{entry.warnings?.map((warning, i) => <p className="variant-warning" key={i}>{warning}</p>)}<div className="variant-actions"><button className="btn primary" disabled={busy || !entry.available} onClick={onOpen}>{t('libOpen')}<ArrowRight size={14} /></button><button className="btn" disabled={busy} onClick={onEdit}><Settings2 size={14} />{t('libEdit')}</button><button className="btn ghost" disabled={busy} onClick={onRelocate}>{t('libRelocate')}</button><button className="btn ghost danger" disabled={busy} onClick={onRemove}>{t('libRemove')}</button></div></article>;
}
function LibraryEditor({ edit, busy, error, onClose, onSave, onRelocate, onRemove }: { edit: null | { entry: LibraryEntry; kind: 'edit' | 'relocate' | 'remove' }; busy: boolean; error: string; onClose: () => void; onSave: (patch: { display_name: string; tags: string[]; status: LibraryStatus; favorite: boolean }) => void; onRelocate: (path: string) => void; onRemove: () => void }) {
  const [name, setName] = useState(''); const [tags, setTags] = useState(''); const [status, setStatus] = useState<LibraryStatus>('working'); const [favorite, setFavorite] = useState(false); const [path, setPath] = useState('');
  useEffect(() => { if (edit) { setName(edit.entry.display_name); setTags(edit.entry.tags.join(', ')); setStatus(edit.entry.status); setFavorite(edit.entry.favorite); setPath(edit.entry.path); } }, [edit]);
  return <D.Root open={!!edit} onOpenChange={(open) => !open && !busy && onClose()}><D.Portal><D.Overlay className="dlg-ov" /><D.Content className="dlg library-dialog" aria-describedby="library-editor-description"><D.Title className="dlg-t">{t(edit?.kind === 'remove' ? 'libRemove' : edit?.kind === 'relocate' ? 'libRelocate' : 'libEdit')}</D.Title><D.Description id="library-editor-description" className="dlg-d">{edit?.kind === 'remove' ? t('libRemoveHint') : edit?.kind === 'relocate' ? t('libRelocateHint') : t('libEditHint')}</D.Description><form onSubmit={(e) => { e.preventDefault(); if (edit?.kind === 'remove') onRemove(); else if (edit?.kind === 'relocate') onRelocate(path.trim()); else onSave({ display_name: name.trim(), tags: [...new Set(tags.split(/[,，]/).map((tag) => tag.trim()).filter(Boolean))], status, favorite }); }}>{edit?.kind === 'edit' ? <><label>{t('libDisplayName')}<input className="input" autoFocus required value={name} onChange={(e) => setName(e.target.value)} /></label><label>{t('libTags')}<input className="input" value={tags} onChange={(e) => setTags(e.target.value)} /></label><label>{t('libProduction')}<select className="input library-select" value={status} onChange={(e) => setStatus(e.target.value as LibraryStatus)}>{(['working', 'review', 'complete', 'archived'] as LibraryStatus[]).map((value) => <option key={value} value={value}>{productionLabel(value)}</option>)}</select></label><label className="library-check"><input type="checkbox" checked={favorite} onChange={(e) => setFavorite(e.target.checked)} />{t('libFavorite')}</label></> : edit?.kind === 'relocate' ? <label>{t('libProjectPath')}<input className="input" autoFocus required value={path} onChange={(e) => setPath(e.target.value)} /></label> : <p className="mono library-remove-path">{edit?.entry.path}</p>}{error && <p role="alert" className="library-feedback error">{error}</p>}<div className="dlg-a"><button type="button" className="btn" disabled={busy} onClick={onClose}>{t('cancel')}</button><button className={'btn ' + (edit?.kind === 'remove' ? 'danger' : 'primary')} disabled={busy || (edit?.kind === 'edit' && !name.trim()) || (edit?.kind === 'relocate' && !path.trim())}>{busy ? t('libWorking') : t(edit?.kind === 'remove' ? 'libRemove' : 'libSave')}</button></div></form></D.Content></D.Portal></D.Root>;
}
