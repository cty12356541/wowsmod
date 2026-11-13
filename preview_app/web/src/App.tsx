import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import * as RT from '@radix-ui/react-tooltip';
import { useStore } from './store';
import { TitleBar } from './components/TitleBar';
import { LeftPanel } from './components/LeftPanel';
import { Viewport } from './components/Viewport';
import { RightPanel, StatusBar, Toasts, Dialogs, OfflineBar } from './components/Shell';

export default function App() {
  const lang = useStore((s) => s.lang);
  return (
    <RT.Provider delayDuration={450} skipDelayDuration={150}>
      <div className="app" lang={lang === 'zh' ? 'zh-CN' : lang}>
        <TitleBar />
        <OfflineBar />
        <PanelGroup direction="horizontal" autoSaveId="wm.layout" className="main">
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
