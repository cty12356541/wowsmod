import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { useEffect, useState } from 'react';
import * as RT from '@radix-ui/react-tooltip';
import { useStore } from './store';
import { TitleBar } from './components/TitleBar';
import { LeftPanel } from './components/LeftPanel';
import { Viewport } from './components/Viewport';
import { RightPanel, StatusBar, Toasts, Dialogs, OfflineBar } from './components/Shell';
import { Library } from './components/Library';
import { WorkbenchNav } from './components/WorkbenchNav';
import { api } from './lib/api';
import { showLibrary } from './lib/scope';

export default function App() {
  const [mode, setMode] = useState<'pending' | 'library' | 'project'>(() => showLibrary(location.search, false) ? 'library' : new URLSearchParams(location.search).get('project') ? 'project' : 'pending');
  const [connectionError, setConnectionError] = useState('');
  useEffect(() => {
    if (mode !== 'pending' || connectionError) return;
    let active = true;
    api.health().then((health) => { if (active) setMode(showLibrary(location.search, !!health.library_mode) ? 'library' : 'project'); }).catch((error) => { if (active) setConnectionError(error.message); });
    return () => { active = false; };
  }, [mode, connectionError]);
  if (mode === 'library') return <Library />;
  if (mode === 'pending') return <div className="project-no-preview" role={connectionError ? 'alert' : 'status'}><h2>wowsmod</h2><p>{connectionError || 'Connecting…'}</p>{connectionError && <button className="btn" onClick={() => setConnectionError('')}>Retry / 重试</button>}</div>;
  return <Workbench />;
}

function Workbench() {
  const lang = useStore((s) => s.lang);
  const applyMessage = useStore((s) => s.applyMessage);
  const [compact, setCompact] = useState(() => window.matchMedia('(max-width: 700px)').matches);
  useEffect(() => {
    const query = window.matchMedia('(max-width: 700px)');
    const update = () => setCompact(query.matches);
    query.addEventListener('change', update);
    update();
    return () => query.removeEventListener('change', update);
  }, []);
  return (
    <RT.Provider delayDuration={450} skipDelayDuration={150}>
      <div className="app" lang={lang === 'zh' ? 'zh-CN' : lang}>
        <TitleBar />
        <WorkbenchNav />
        {applyMessage && <div className="apply-message" role="status">{applyMessage}</div>}
        <OfflineBar />
        <PanelGroup direction={compact ? 'vertical' : 'horizontal'} autoSaveId={compact ? 'wm.layout.compact' : 'wm.layout'} className={'main' + (compact ? ' compact' : '')}>
          <Panel defaultSize={19} minSize={14} maxSize={32} order={1}><LeftPanel /></Panel>
          <PanelResizeHandle className="rh" />
          <Panel minSize={36} order={2}><Viewport /></Panel>
          <PanelResizeHandle className="rh" />
          <Panel defaultSize={23} minSize={17} maxSize={36} order={3}><RightPanel /></Panel>
        </PanelGroup>
        <StatusBar />
        <Toasts />
        <Dialogs />
      </div>
    </RT.Provider>
  );
}
