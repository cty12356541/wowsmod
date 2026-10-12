import React from 'react';
import * as RT from '@radix-ui/react-tooltip';
import * as DM from '@radix-ui/react-dropdown-menu';
import * as RS from '@radix-ui/react-slider';
import { Check, ChevronDown } from 'lucide-react';
import { t } from '../lib/i18n';

export function Tip({ label, kbd, children, side = 'bottom' }: { label?: React.ReactNode; kbd?: string; children: React.ReactElement; side?: 'top' | 'bottom' | 'left' | 'right' }) {
  if (!label) return children;
  return (
    <RT.Root>
      <RT.Trigger asChild>{children}</RT.Trigger>
      <RT.Portal><RT.Content className="tip" side={side} sideOffset={6} collisionPadding={8}>{label}{kbd && <span className="kbd">{kbd}</span>}</RT.Content></RT.Portal>
    </RT.Root>
  );
}

export const IconBtn = React.forwardRef<HTMLButtonElement, { icon: React.ReactNode; label: string; kbd?: string; on?: boolean; sm?: boolean; side?: 'top' | 'bottom' | 'left' | 'right' } & React.ButtonHTMLAttributes<HTMLButtonElement>>(
  ({ icon, label, kbd, on, sm, side, className, ...rest }, ref) => (
    <Tip label={label} kbd={kbd} side={side}>
      <button ref={ref} aria-label={label} className={'ibtn' + (sm ? ' sm' : '') + (className ? ' ' + className : '')} data-on={on ? 'true' : undefined} aria-pressed={on} {...rest}>{icon}</button>
    </Tip>
  ),
);

export interface SegOpt<T> { v: T; label: React.ReactNode; tip?: React.ReactNode; disabled?: boolean; kbd?: string }
export function Seg<T extends string | number>({ value, opts, onChange, size, block, ariaLabel }: { value: T; opts: SegOpt<T>[]; onChange: (v: T) => void; size?: 'xs' | 'ws'; block?: boolean; ariaLabel?: string }) {
  return (
    <div className={'seg' + (size ? ' ' + size : '') + (block ? ' block' : '')} role="radiogroup" aria-label={ariaLabel}>
      {opts.map((o) => (
        <Tip key={String(o.v)} label={o.tip} kbd={o.kbd}>
          <button role="radio" aria-checked={o.v === value} data-state={o.v === value ? 'on' : 'off'} disabled={o.disabled} onClick={() => onChange(o.v)}>{o.label}</button>
        </Tip>
      ))}
    </div>
  );
}

export function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label?: string }) {
  return <button className="switch" role="switch" aria-checked={on} aria-label={label} data-on={on} onClick={() => onChange(!on)} />;
}

export function Slider({ value, min, max, step, onChange, label }: { value: number; min: number; max: number; step: number; onChange: (v: number) => void; label?: string }) {
  return (
    <RS.Root className="slider" value={[value]} min={min} max={max} step={step} onValueChange={(v) => onChange(v[0])} aria-label={label}>
      <RS.Track className="slider-track"><RS.Range className="slider-range" /></RS.Track>
      <RS.Thumb className="slider-thumb" aria-label={label} />
    </RS.Root>
  );
}

export interface MenuItem { label?: React.ReactNode; onSelect?: () => void; checked?: boolean; kbd?: string; disabled?: boolean; sep?: boolean; heading?: React.ReactNode; icon?: React.ReactNode }
export function Menu({ trigger, items, align = 'start' }: { trigger: React.ReactElement; items: MenuItem[]; align?: 'start' | 'end' | 'center' }) {
  // 鼠标选中菜单项后不把焦点还给触发按钮: 否则触发按钮的 Tooltip 因 focus 打开并一直停留(键盘操作仍还焦点)
  const byPointer = React.useRef(false);
  return (
    <DM.Root modal={false}>
      <DM.Trigger asChild>{trigger}</DM.Trigger>
      <DM.Portal>
        <DM.Content className="menu" align={align} sideOffset={4} collisionPadding={8}
          onPointerDown={() => { byPointer.current = true; }} onKeyDown={() => { byPointer.current = false; }}
          onCloseAutoFocus={(e) => { if (byPointer.current) { e.preventDefault(); (document.activeElement as HTMLElement | null)?.blur?.(); } byPointer.current = false; }}>
          {items.map((it, i) => it.sep ? <DM.Separator key={i} className="menu-sep" /> : it.heading ? <DM.Label key={i} className="menu-label">{it.heading}</DM.Label> : (
            <DM.Item key={i} className="menu-item" disabled={it.disabled} onSelect={() => it.onSelect && it.onSelect()}>
              {it.checked !== undefined && <span className="ck">{it.checked ? <Check size={14} strokeWidth={2} /> : null}</span>}
              {it.icon}{it.label}{it.kbd && <span className="kbd">{it.kbd}</span>}
            </DM.Item>
          ))}
        </DM.Content>
      </DM.Portal>
    </DM.Root>
  );
}
export const SelBtn = React.forwardRef<HTMLButtonElement, { children: React.ReactNode } & React.ButtonHTMLAttributes<HTMLButtonElement>>(({ children, ...rest }, ref) => (
  <button ref={ref} className="sel" {...rest}>{children}<ChevronDown size={12} className="chev" /></button>
));

export function Section({ title, right, children }: { title: React.ReactNode; right?: React.ReactNode; children: React.ReactNode }) {
  return <section className="sec"><div className="sec-h">{title}{right && <span className="r">{right}</span>}</div><div className="sec-b">{children}</div></section>;
}
/** 属性行：src = 'exact'（按游戏数据）| 'approx'（近似示意）| undefined */
export function Prop({ k, children, src, srcTip }: { k: React.ReactNode; children: React.ReactNode; src?: 'exact' | 'approx' | 'preview'; srcTip?: React.ReactNode }) {
  return (
    <div className="prop">
      <span className="k">{k}</span><span className="v">{children}</span>
      {src ? <Tip label={srcTip} side="left"><span className={'src ' + src}>{srcLabel(src)}</span></Tip> : <span />}
    </div>
  );
}
const srcLabel = (s: string) => t('src_' + s);
