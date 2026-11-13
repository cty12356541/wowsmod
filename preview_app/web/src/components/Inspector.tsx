import { X, Crosshair, EyeOff, Eye, Focus } from 'lucide-react';
import { useStore, actions, viewer } from '../store';
import { t, partLabel } from '../lib/i18n';
import { Prop, Section, IconBtn } from './ui';
import { shortTex } from './LeftPanel';

export function Inspector() {
  const s = useStore(); const p = s.parts.find((x) => x.id === s.selected);
  if (!p) return <ProjectInfo />;
  const tex = s.textures.filter((x) => x.materials.some((m) => p.materials.includes(m)));
  const hidden = !!s.hidden[p.id];
  return (
    <div>
      <div className="insp-h">
        <div style={{ minWidth: 0, flex: 1 }}><div className="ttl">{partLabel(p)}</div><div className="sub mono">{p.hp || p.id}</div></div>
        <IconBtn icon={<Crosshair size={16} />} label={t('focus')} kbd="F" onClick={() => viewer?.focus([p.id])} />
        <IconBtn icon={hidden ? <EyeOff size={16} /> : <Eye size={16} />} label={t('toggleVis')} onClick={() => actions.toggleHidden(p.id)} />
        <IconBtn icon={<Focus size={16} />} label={t('solo')} onClick={() => actions.solo(p.id)} />
        <IconBtn icon={<X size={16} />} label={t('deselect')} kbd="Esc" onClick={() => actions.select(null)} />
      </div>
      <Section title={t('props')}>
        <Prop k={t('group')}>{t('group_' + p.group)}</Prop>
        {p.section && <Prop k={t('section')}>{t('sec_' + p.section) || p.section}</Prop>}
        {p.model && <Prop k={t('model')}><span className="mono" title={p.model}>{p.model}</span></Prop>}
        {p.module && <Prop k={t('module')}><span className="mono">{p.module}</span></Prop>}
        {p.caliber_mm && <Prop k={t('caliber')} src="exact" srcTip={t('srcGP')}>{p.caliber_mm} mm{p.barrels ? ` × ${p.barrels}` : ''}</Prop>}
        <Prop k={t('changeState')}>{p.changed ? <span className="mod">{t('changedVsBase')}</span> : <span className="faint">{t('unchanged')}</span>}</Prop>
        {p.bones && p.bones.length > 0 && <Prop k={t('bones')} src="exact" srcTip={t('srcVisual')}>{p.bones.map((b) => t('bone_' + b)).join(' → ')}</Prop>}
      </Section>
      {p.lods && p.lods.length > 0 && (
        <Section title={t('lodTable')} right={<span className="faint">{t('lodSrc')}</span>}>
          <table className="tbl num">
            <thead><tr><th>LOD</th><th>{t('lodExtent')}</th><th>{t('verts')}</th><th>{t('tris')}</th></tr></thead>
            <tbody>{p.lods.map((l) => <tr key={l.lod} data-cur={s.snap ? s.snap.lod[p.id] === l.lod : l.lod === 0}><td>{l.lod}</td><td>{l.extent}</td><td>{l.verts.toLocaleString()}</td><td>{l.tris.toLocaleString()}</td></tr>)}</tbody>
          </table>
        </Section>
      )}
      <Section title={t('materials')} right={<span className="faint num">{p.materials.length}</span>}>
        {p.materials.map((m) => <div key={m} className="kv mono" title={m}>{m}</div>)}
      </Section>
      <Section title={t('texturesUsed')} right={<span className="faint num">{tex.length}</span>}>
        {tex.map((x) => (
          <div key={x.name} className="kv clickable" onClick={() => useStore.setState({ leftTab: 'textures', selTex: x.name })}>
            <span className="mono" style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis' }} title={x.name}>{shortTex(x.name)}</span>
            {x.changed && <span className="m">M</span>}
          </div>
        ))}
      </Section>
    </div>
  );
}

function ProjectInfo() {
  const p = useStore((s) => s.project); const parts = useStore((s) => s.parts); const tex = useStore((s) => s.textures); const lang = useStore((s) => s.lang);
  if (!p) return <div className="empty"><div>{t('loadingShort')}</div></div>;
  const ch = parts.filter((x) => x.changed).length;
  return (
    <div>
      <div className="empty compact"><div className="t">{t('noSelection')}</div><div>{t('noSelectionDesc')}</div></div>
      <Section title={t('project')}>
        <Prop k={t('projName')}>{p.project.name}</Prop>
        <Prop k={t('ship')}>{p.ship.name[lang] || p.ship.name.en} <span className="faint mono">{p.ship.index}</span></Prop>
        <Prop k={t('hullModel')}><span className="mono">{p.ship.hull_model}</span></Prop>
        <Prop k={t('game')}>{p.game.client} {p.game.version}</Prop>
        <Prop k={t('changedParts')}>{ch} / {parts.length}</Prop>
        <Prop k={t('changedTex')}>{tex.filter((x) => x.changed).length} / {tex.length}</Prop>
        <Prop k={t('projDir')}><span className="mono" title={p.project.dir}>{p.project.dir}</span></Prop>
      </Section>
    </div>
  );
}
