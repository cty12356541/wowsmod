import type { LibraryEntry } from './types';

export interface LibraryFilters {
  search: string; nations: string[]; levels: string[]; species: string[]; statuses: string[];
  favorites: boolean; sort: 'opened' | 'added' | 'name';
}
export const defaultFilters: LibraryFilters = { search: '', nations: [], levels: [], species: [], statuses: [], favorites: false, sort: 'opened' };
export function shipKey(entry: LibraryEntry): string { return entry.project?.ship_key || entry.ship_metadata.key || (entry.project?.ship ? 'unknown:' + entry.project.ship : 'global'); }
export function filterProjects(projects: LibraryEntry[], filters: LibraryFilters): LibraryEntry[] {
  const words = filters.search.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  const matches = (selected: string[], value: string | number | null | undefined) => !selected.length || selected.includes(value == null || value === '' ? 'unknown' : String(value));
  return projects.filter((entry) => {
    const meta = entry.ship_metadata;
    if (!filters.statuses.length && entry.status === 'archived') return false;
    if (!matches(filters.statuses, entry.status) || (filters.favorites && !entry.favorite)) return false;
    if (!matches(filters.nations, meta.nation) || !matches(filters.levels, meta.level) || !matches(filters.species, meta.species)) return false;
    const search = [entry.display_name, entry.project?.name, entry.project?.ship, entry.project?.ship_key, meta.name_local, meta.param, ...entry.tags].filter(Boolean).join(' ').toLocaleLowerCase();
    return words.every((word) => search.includes(word));
  }).sort((a, b) => {
    const order = filters.sort === 'name' ? a.display_name.localeCompare(b.display_name, 'zh-CN') : filters.sort === 'added' ? b.added_unix - a.added_unix : (b.opened_unix || 0) - (a.opened_unix || 0);
    return order || a.id.localeCompare(b.id);
  });
}
export function groupProjects(projects: LibraryEntry[]): { key: string; projects: LibraryEntry[] }[] {
  const groups = new Map<string, LibraryEntry[]>();
  projects.forEach((p) => { const key = shipKey(p); groups.set(key, [...(groups.get(key) || []), p]); });
  return [...groups].map(([key, entries]) => ({ key, projects: entries }));
}
export function shipLabel(entry: LibraryEntry): string {
  return entry.ship_metadata.name_local || entry.ship_metadata.param || entry.project?.ship || (shipKey(entry) === 'global' ? '全局 / 未关联' : '未识别舰船');
}
export function facetValues(projects: LibraryEntry[], field: 'nation' | 'level' | 'species'): string[] {
  return [...new Set(projects.map((p) => p.ship_metadata[field] == null || p.ship_metadata[field] === '' ? 'unknown' : String(p.ship_metadata[field])))].sort((a, b) => field === 'level' ? (Number(a) || 99) - (Number(b) || 99) : a.localeCompare(b));
}
