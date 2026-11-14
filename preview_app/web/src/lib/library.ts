import type { LibraryEntry, LibrarySummary, Project } from './types';

export function createLibraryListLoader(read: () => Promise<LibrarySummary>, publish: {
  data: (value: LibrarySummary) => void;
  error: (message: string) => void;
  loading: (value: boolean) => void;
}) {
  let generation = 0;
  return {
    async load() {
      const current = ++generation;
      publish.loading(true);
      publish.error('');
      try {
        const result = await read();
        if (current === generation) publish.data(result);
      } catch (error) {
        if (current === generation) publish.error(error instanceof Error ? error.message : String(error));
      } finally {
        if (current === generation) publish.loading(false);
      }
    },
    invalidate() { generation++; },
  };
}

export interface LibraryFilters {
  search: string; nations: string[]; levels: string[]; species: string[]; statuses: string[];
  favorites: boolean; sort: 'opened' | 'added' | 'name' | 'modified';
}
export const defaultFilters: LibraryFilters = { search: '', nations: [], levels: [], species: [], statuses: [], favorites: false, sort: 'opened' };
export function libraryRefreshKey(project: Project | null, catalogEpoch: number): string {
  return JSON.stringify([catalogEpoch, project?.game.path, project?.game.build, project?.install.state, project?.install.rev, project?.install.files, project?.install.time]);
}
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
    let order: number;
    switch (filters.sort) {
      case 'name': order = a.display_name.localeCompare(b.display_name, 'zh-CN'); break;
      case 'added': order = b.added_unix - a.added_unix; break;
      case 'modified': order = (b.modified_unix ?? -1) - (a.modified_unix ?? -1); break;
      default: order = (b.opened_unix || 0) - (a.opened_unix || 0);
    }
    return order || a.id.localeCompare(b.id);
  });
}
export function groupProjects(projects: LibraryEntry[]): { key: string; projects: LibraryEntry[] }[] {
  const groups = new Map<string, LibraryEntry[]>();
  projects.forEach((p) => { const key = shipKey(p); groups.set(key, [...(groups.get(key) || []), p]); });
  return [...groups].map(([key, entries]) => ({ key, projects: entries }));
}
export function shipLabel(entry: LibraryEntry, translate: (key: string) => string): string {
  return entry.ship_metadata.name_local || entry.ship_metadata.param || entry.project?.ship || translate(shipKey(entry) === 'global' ? 'libGlobal' : 'libUnknown');
}
export function facetValues(projects: LibraryEntry[], field: 'nation' | 'level' | 'species'): string[] {
  return [...new Set(projects.map((p) => p.ship_metadata[field] == null || p.ship_metadata[field] === '' ? 'unknown' : String(p.ship_metadata[field])))].sort((a, b) => field === 'level' ? (Number(a) || 99) - (Number(b) || 99) : a.localeCompare(b));
}
