import { useState } from 'react';
import { ChevronRight, Undo2, RotateCcw, Eye, Terminal, MessageSquare } from 'lucide-react';
import { useStore, actions } from '../store';
import { t, fmtTime, cmdName } from '../lib/i18n';
import type { Step } from '../lib/types';
import { IconBtn, Tip, Menu } from './ui';

export function History() {
  const steps = useStore((s) => s.steps); const head = useStore((s) => s.head); const viewRev = useStore((s) => s.viewRev); const phase = useStore((s) => s.phase);
  const [open, setOpen] = useState<string | null>(null);
  if (!steps.length) return phase === 'ready' ? <div className="empty"><div className="t">{t('noSteps')}</div><div>{t('noStepsDesc')}</div></div>
    : <div>{[80, 64, 72].map((w, i) => <div key={i} style={{ padding: '12px' }}><div className="sk" style={{ width: w + '%', marginBottom: 6 }} /><div className="sk" style={{ width: '40%' }} /></div>)}</div>;
  const list = [...steps].reverse();
  return (
    <div className="steps" role="list">
      {list.map((st) => <StepRow key={st.id} st={st} isHead={st.id === head} viewing={st.id === (viewRev || head)} open={open === st.id || (open === null && st.id === (viewRev || head))} toggle={() => setOpen(open === st.id ? '' : st.id)} />)}
    </div>
  );
}

function StepRow({ st, isHead, viewing, open, toggle }: { st: Step; isHead: boolean; viewing: boolean; open: boolean; toggle: () => void }) {
  const lang = useStore((s) => s.lang); void lang;
  const [allEdits, setAllEdits] = useState(false);
  const EDIT_MAX = 4; const edits = allEdits ? st.edits : st.edits.slice(0, EDIT_MAX);
  const rev = st.state === 'reverted';
  return (
    <div className="step" role="listitem" data-cur={viewing} data-rev={rev} data-open={open}>
      <div className="step-h" onClick={() => !rev && actions.view(st.id)}>
        <span className="idx num">{st.index}</span>
        <div className="body">
          <div className="ttl">{st.title}{isHead && <span className="badge">{t('current')}</span>}{rev && <span className="badge mute">{t('reverted')}</span>}</div>
          <div className="meta num">{fmtTime(st.time)} · {st.index === 0 ? t('baselineMeta') : t('stepMeta', { t: st.textures.length, p: st.parts.length })}</div>
        </div>
        <IconBtn sm icon={<ChevronRight size={14} className="chev-r" />} label={open ? t('collapse') : t('details')} onClick={(e) => { e.stopPropagation(); toggle(); }} />
      </div>
      {open && (
        <div className="step-d">
          {st.user_prompt && <div className="quote"><MessageSquare size={12} /><span>{st.user_prompt}</span></div>}
          {st.ai_note && <p className="note">{st.ai_note}</p>}
          {!!st.edits.length && (
            <div className="edits">
              {edits.map((e, i) => (
                <Tip key={i} label={<code style={{ whiteSpace: 'pre-wrap', maxWidth: 420, display: 'block' }}>{e.command_line}</code>} side="left">
                  <div className="edit"><Terminal size={12} /><span>{cmdName(e.cmd)}</span><span className="faint mono">{(e.outputs?.[0]?.path || e.file).split('/').pop()}</span>{e.outputs && e.outputs.length > 1 && <span className="faint num">+{e.outputs.length - 1}</span>}</div>
                </Tip>
              ))}
              {st.edits.length > EDIT_MAX && <button className="linkbtn" onClick={() => setAllEdits(!allEdits)}>{allEdits ? t('showLess') : t('showAllCmds', { n: st.edits.length })}</button>}
            </div>
          )}
          <div className="acts">
            {!rev && !viewing && <button className="btn" onClick={() => actions.view(st.id)}><Eye size={14} />{t('viewThis')}</button>}
            {!rev && !isHead && <Menu trigger={<button className="btn"><Undo2 size={14} />{t('revertHere')}</button>} items={[{ heading: t('revertConfirm', { n: st.index }) }, { label: t('revertDo'), onSelect: () => actions.revert(st.id) }]} />}
            {rev && <button className="btn" onClick={() => actions.restore(st.id)}><RotateCcw size={14} />{t('restore')}</button>}
          </div>
        </div>
      )}
    </div>
  );
}
