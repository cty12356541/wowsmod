export function projectPrefix(search: string): string {
  const id = new URLSearchParams(search).get('project');
  return '/api/v1' + (id ? '/projects/' + encodeURIComponent(id) : '');
}
export function showLibrary(search: string, libraryMode: boolean): boolean {
  const query = new URLSearchParams(search);
  return !query.has('render') && !query.get('project') && (query.get('library') === '1' || libraryMode);
}
export class ProjectCache<T> {
  private values = new Map<string, T>();
  get(project: string, revision: string) { return this.values.get(JSON.stringify([project, revision])); }
  set(project: string, revision: string, value: T) { this.values.set(JSON.stringify([project, revision]), value); }
  clear() { this.values.clear(); }
}
