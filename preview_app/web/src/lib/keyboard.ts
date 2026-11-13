import { useStore, actions, viewer } from '../store';

export const handleWorkbenchKey = (e: KeyboardEvent) => {
    // Tab always belongs to the browser (including body initial focus and modal focus traps).
    if (e.defaultPrevented || e.key === 'Tab') return;
    const tg = e.target as HTMLElement | null;
    if (tg?.isContentEditable || tg?.closest?.('input, textarea, select, option, button, a[href], summary, [contenteditable], [tabindex], [role="button"], [role="checkbox"], [role="switch"], [role="slider"], [role="tab"], [role="menuitem"], [role="combobox"], [role="listbox"], [role="textbox"]')) return;
    if (tg?.closest?.('[role="dialog"], [role="alertdialog"]') || document.querySelector('[role="dialog"], [role="alertdialog"]')) return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const s = useStore.getState(); const k = e.key.toLowerCase();
    if (e.shiftKey) {
      if (k === 'v' && !e.repeat && s.sim && s.project?.capabilities?.simulation !== false) { e.preventDefault(); actions.setWorkspace(s.workspace === 'review' ? 'sim' : 'review'); }
      return;
    }
    if (s.workspace === 'review') {
      const m = { '1': 'single', '2': 'split', '3': 'side', '4': 'overlay' }[k]; if (m) { actions.setMode(m as any); return; }
      if (k === 'f' && s.selected) { viewer?.focus([s.selected]); return; }
      const ch = { a: 1, n: 2, e: 3, w: 4, m: 5 }[k]; if (ch !== undefined) { actions.setChannel(s.channel === ch ? 0 : ch); return; }
      if (k === 'escape') { actions.select(null); return; }
      if (k === 'arrowup' || k === 'arrowdown') {
        const act = s.steps; const i = act.findIndex((x) => x.id === (s.viewRev || s.head)); const j = i + (k === 'arrowup' ? -1 : 1);
        if (act[j]) { e.preventDefault(); actions.view(act[j].id); }
      }
      return;
    }
    // 模拟
    const tels = ['rev', 'stop', 'q1', 'q2', 'q3', 'full'] as const; const ti = tels.indexOf(s.tel as any);
    if (k === 'w') actions.setTel(tels[Math.min(5, ti + 1)]);
    else if (k === 's') actions.setTel(tels[Math.max(0, ti - 1)]);
    else if (k === 'a') actions.setRudder(Math.max(-1, s.rudder - 0.5));
    else if (k === 'd') actions.setRudder(Math.min(1, s.rudder + 0.5));
    else if (k === ' ') { e.preventDefault(); actions.fire('main', 'salvo'); }
    else if (k === 'f') actions.fire('main', 'single');
    else if (k === 'r') actions.fire('secondary', 'salvo');
    else if (k === 'c') actions.setCombat(!s.combat);
    else if (k === '1' || k === '2' || k === '3') actions.setDist((['near', 'mid', 'far'] as const)[+k - 1]);
  };
