import { registerHooks } from 'node:module';
import test from 'node:test';
import assert from 'node:assert/strict';
registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('.') && !/\.[a-z]+$/.test(specifier)) return nextResolve(specifier + '.ts', context);
  return nextResolve(specifier, context);
} });
globalThis.window = { WM_API_BASE: '' };
globalThis.location = { search: '?project=B' };
globalThis.localStorage = { getItem: (key) => key === 'wm.quality' ? 'low' : null };
let dialogOpen = false;
globalThis.document = { documentElement: {}, querySelector: () => dialogOpen ? {} : null };
const { handleWorkbenchKey } = await import('./keyboard.ts');
const { useStore } = await import('../store.ts');
const target = (tagName, { interactive = false, dialog = false } = {}) => ({ tagName, isContentEditable: false, closest: (selector) => selector.includes('dialog') ? dialog ? {} : null : interactive ? {} : null });
function key(key, element, extra = {}) {
  const event = new Event('keydown', { cancelable: true });
  Object.defineProperties(event, Object.fromEntries(Object.entries({ key, target: element, ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, repeat: false, ...extra }).map(([name, value]) => [name, { value }])));
  handleWorkbenchKey(event); return event;
}
test.beforeEach(() => { dialogOpen = false; useStore.setState({ workspace: 'review', channel: 0, sim: {}, project: null }); });
test('Tab and Shift+Tab preserve native focus navigation from Cancel/Confirm/body with or without simulation', () => {
  for (const sim of [{}, null]) for (const element of [target('BUTTON', { interactive: true }), target('BODY')]) for (const shiftKey of [false, true]) {
    useStore.setState({ sim }); const event = key('Tab', element, { shiftKey });
    assert.equal(event.defaultPrevented, false); assert.equal(useStore.getState().workspace, 'review');
  }
});
test('same-ship select and nested button controls keep letter and arrow keys', () => {
  for (const element of [target('SELECT', { interactive: true }), target('SPAN', { interactive: true }), target('SUMMARY', { interactive: true })]) {
    const event = key('w', element); assert.equal(event.defaultPrevented, false); assert.equal(useStore.getState().channel, 0);
    useStore.setState({ steps: [{ id: 's1', parts: [] }, { id: 's2', parts: [] }], head: 's2', viewRev: 's1' });
    const arrow = key('ArrowDown', element); assert.equal(arrow.defaultPrevented, false); assert.equal(useStore.getState().viewRev, 's1');
  }
});
test('open modal suppresses workbench shortcuts even before focus enters the modal', () => {
  dialogOpen = true; key('w', target('BODY')); assert.equal(useStore.getState().channel, 0);
});
test('Shift+V toggles workspaces only outside interactive controls and dialogs', () => {
  key('V', target('BUTTON', { interactive: true }), { shiftKey: true }); assert.equal(useStore.getState().workspace, 'review');
  const event = key('V', target('BODY'), { shiftKey: true }); assert.equal(event.defaultPrevented, true); assert.equal(useStore.getState().workspace, 'sim');
});
test('unhandled/prevented keys and modified shortcuts never dispatch workspace changes', () => {
  const prevented = new Event('keydown', { cancelable: true }); prevented.preventDefault(); Object.defineProperties(prevented, { key: { value: 'w' }, target: { value: target('BODY') } }); handleWorkbenchKey(prevented);
  assert.equal(useStore.getState().channel, 0);
  key('V', target('BODY'), { shiftKey: true, ctrlKey: true }); assert.equal(useStore.getState().workspace, 'review');
});
test('review channel shortcut continues to work on the canvas', () => {
  key('w', target('CANVAS')); assert.equal(useStore.getState().channel, 4);
});
