import test from 'node:test';
import assert from 'node:assert/strict';
import { defaultFilters, filterProjects, groupProjects, shipLabel, libraryRefreshKey } from './library.ts';
import { setLang, t } from './i18n.ts';
import { ProjectCache, projectPrefix, showLibrary } from './scope.ts';

const entry = (id, extra = {}) => ({ id, path: `C:/mods/${id}`, display_name: 'Robin', tags: ['角色'], favorite: false, status: 'working', added_unix: 100, opened_unix: null, available: true, warnings: [], project: { name: 'same-mod', version: '1', ship: 'FSB110', ship_key: 'PFSB110', game_build: '123', mode: 'sdk' }, ship_metadata: { key: 'PFSB110', known: true, nation: 'France', level: 10, species: 'Battleship', name_local: '勃艮第', param: 'PFSB110_Bourgogne', warning: null }, install: { state: 'not_installed', rev: null, files: 0, time: null }, ...extra });
const data = [entry('a'), entry('b', { status: 'review', favorite: true }), entry('c', { status: 'archived' }), entry('d', { ship_metadata: { key: 'PJSB010', known: true, nation: 'Japan', level: 9, species: 'Cruiser', name_local: '日本船', param: 'PJSB010' }, project: { ship_key: 'PJSB010' } }), entry('e', { project: null, ship_metadata: { known: false, key: '', nation: null, level: null, species: null, name_local: null, param: null } })];
const filter = (patch = {}) => filterProjects(data, { ...defaultFilters, ...patch }).map((p) => p.id);
test('facet selections use OR within each dimension and AND between dimensions', () => {
  assert.deepEqual(filter({ nations: ['France', 'Japan'], levels: ['10'] }), ['a', 'b']);
  assert.deepEqual(filter({ nations: ['Japan'], species: ['Battleship'] }), []);
});
test('search matches Chinese ship names and tags, English mod names and ship parameters', () => {
  assert.deepEqual(filter({ search: '勃艮第' }), ['a', 'b']);
  assert.deepEqual(filter({ search: '  rOBIN  ' }), ['a', 'b', 'd', 'e']);
  assert.deepEqual(filter({ search: '角色' }), ['a', 'b', 'd', 'e']);
  assert.deepEqual(filter({ search: 'bourgogne' }), ['a', 'b']);
});
test('archived entries hide by default but explicit status selection includes them', () => {
  assert.deepEqual(filter(), ['a', 'b', 'd', 'e']);
  assert.deepEqual(filter({ statuses: ['archived'] }), ['c']);
  assert.deepEqual(filter({ favorites: true }), ['b']);
});
test('missing metadata stays browsable and is filterable without invented attributes', () => {
  assert.deepEqual(filter({ nations: ['unknown'] }), ['e']);
  assert.deepEqual(filter({ levels: ['unknown'], species: ['unknown'] }), ['e']);
});
test('three same-name variants retain independent IDs in one stable ship group', () => {
  const groups = groupProjects(data.slice(0, 3));
  assert.equal(groups.length, 1);
  assert.equal(groups[0].key, 'PFSB110');
  assert.deepEqual(groups[0].projects.map((p) => p.id), ['a', 'b', 'c']);
});
test('sorting has deterministic identity tie break and recent opened first', () => {
  assert.deepEqual(filterProjects([entry('b'), entry('a')], defaultFilters).map((p) => p.id), ['a', 'b']);
  assert.deepEqual(filterProjects([entry('a'), entry('b', { opened_unix: 200 })], defaultFilters).map((p) => p.id), ['b', 'a']);
});
test('same revision IDs from distinct project scopes never share material manifests', () => {
  const cache = new ProjectCache(); cache.set('A', 's1', { materials: 'red' });
  assert.equal(cache.get('B', 's1'), undefined);
  cache.set('B', 's1', { materials: 'blue' });
  assert.deepEqual(cache.get('A', 's1'), { materials: 'red' });
  assert.deepEqual(cache.get('B', 's1'), { materials: 'blue' });
});
test('project prefix and library entry preserve direct and render routes', () => {
  assert.equal(projectPrefix('?project=a%2Fb'), '/api/v1/projects/a%2Fb');
  assert.equal(projectPrefix('?render=1'), '/api/v1');
  assert.equal(showLibrary('', true), true);
  assert.equal(showLibrary('?library=1', false), true);
  assert.equal(showLibrary('?project=a', true), false);
  assert.equal(showLibrary('?render=1', true), false);
  assert.equal(showLibrary('', false), false);
});
test('global and unknown ship fallbacks use the active UI language', () => {
  globalThis.document = { documentElement: {} };
  const globalEntry = entry('global', { project: null, ship_metadata: { key: '', known: false } });
  const unknownEntry = entry('unknown', { project: null, ship_metadata: { key: 'UNKNOWN', known: false } });
  setLang('en');
  assert.equal(shipLabel(globalEntry, t), 'Global / unlinked');
  assert.equal(shipLabel(unknownEntry, t), 'Unknown');
  setLang('zh');
  assert.equal(shipLabel(globalEntry, t), '全局 / 未关联');
  assert.equal(shipLabel(unknownEntry, t), '未识别');
});
test('selector refresh key changes for installation/client changes and completed application only', () => {
  const project = { install: { state: 'not_installed', rev: null, files: 0, time: null }, game: { path: 'D:/Game', build: '123' }, etag: 1, activity: { state: 'idle' } };
  const key = libraryRefreshKey(project, 0);
  assert.notEqual(libraryRefreshKey({ ...project, install: { ...project.install, state: 'installed' } }, 0), key);
  assert.notEqual(libraryRefreshKey(project, 1), key);
  assert.notEqual(libraryRefreshKey({ ...project, game: { ...project.game, build: '124' } }, 0), key);
  assert.equal(libraryRefreshKey({ ...project, etag: 2, activity: { state: 'working' } }, 0), key);
  assert.equal(libraryRefreshKey(structuredClone(project), 0), key);
});
