import { RotateCcw, Flame } from 'lucide-react';
import { useStore, actions } from '../store';
import { t } from '../lib/i18n';
import { TEL_FRAC, REVERSE_FRAC } from '../engine/sim';
import { Prop, Section, Seg, Slider, Switch, Tip, IconBtn } from './ui';

const TELS = ['rev', 'stop', 'q1', 'q2', 'q3', 'full'] as const;

export function SimPanel() {
  const s = useStore(); const sim = s.sim; const sn = s.snap;
  if (!sim) return <div className="empty"><div className="t">{t('simUnavailable')}</div><div>{t('simUnavailableDesc')}</div></div>;
  const sh = sim.ship;
  const telKn = (k: string) => { const f = (TEL_FRAC as any)[k]; return f >= 0 ? f * sh.maxSpeed_kn : REVERSE_FRAC * sh.maxSpeed_kn; };
  const mounts = (sn?.mounts || []).filter((m) => m.group !== 'aa');
  const groups = ['main', 'secondary'] as const;
  return (
    <div className="simpanel">
      <Section title={t('secSail')} right={<IconBtn sm icon={<RotateCcw size={14} />} label={t('simReset')} onClick={() => actions.simReset()} />}>
        <div className="ctl-row">
          <span className="lbl">{t('telegraph')}</span>
          <Seg block size="xs" value={s.tel} onChange={(v) => actions.setTel(v)} ariaLabel={t('telegraph')}
            opts={TELS.map((k) => ({ v: k, label: t('tel_' + k), tip: `${t('tel_' + k + '_long')} · ${telKn(k).toFixed(1)} ${t('kn')}`, kbd: 'W / S' }))} />
        </div>
        <div className="ctl-row">
          <span className="lbl">{t('rudder')}</span>
          <Seg block size="xs" value={s.rudder} onChange={(v) => actions.setRudder(v)} ariaLabel={t('rudder')}
            opts={[-1, -0.5, 0, 0.5, 1].map((v) => ({ v, label: t('rud_' + String(v).replace('.', '_').replace('-', 'm')), kbd: 'A / D' }))} />
        </div>
        <div className="readout num">
          <Tip label={t('srcGPtip', { k: 'maxSpeed' })}><div><span className="k">{t('speed')}</span><b>{sn ? sn.speedKn.toFixed(1) : '0.0'}</b><span className="u">/ {sh.maxSpeed_kn} {t('kn')}</span></div></Tip>
          <div><span className="k">{t('rudderAngle')}</span><b>{sn ? (sn.rudder * (sh.maxRudderAngle_deg || 30)).toFixed(0) : 0}°</b><span className="u">/ ±{sh.maxRudderAngle_deg || 30}°</span></div>
          <div><span className="k">{t('heel')}</span><b>{sn ? sn.heel.toFixed(1) : '0.0'}°</b></div>
          <div><span className="k">{t('heading')}</span><b>{sn ? sn.heading.toFixed(0) : 0}°</b></div>
        </div>
        <Prop k={t('maxSpeed')} src="exact" srcTip={t('srcGP')}>{sh.maxSpeed_kn} {t('kn')}</Prop>
        <Prop k={t('rudderTime')} src="exact" srcTip={t('srcGP')}>{sh.rudderTime_s.toFixed(1)} {t('sec')}</Prop>
        <Prop k={t('turnRadius')} src="exact" srcTip={t('srcGP')}>{sh.turningRadius_m} {t('m')}</Prop>
        {sh.forwardEngineUpTime_s != null && <Prop k={t('engineUp')} src="exact" srcTip={t('srcGP')}>{sh.forwardEngineUpTime_s.toFixed(1)} / {(sh.backwardEngineUpTime_s ?? 0).toFixed(1)} {t('sec')}</Prop>}
        <Prop k={t('heelWaves')} src="approx" srcTip={t('approxHeelTip')}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%' }}><Slider value={s.waves} min={0} max={2} step={0.1} onChange={actions.setWaves} label={t('waves')} /><span className="num faint" style={{ width: 26 }}>{s.waves.toFixed(1)}</span></div>
        </Prop>
        <Prop k={t('wakeFoam')} src="approx" srcTip={t('approxWakeTip')}><span className="faint">{t('auto')}</span></Prop>
      </Section>

      <Section title={t('secCamera')}>
        <Seg block size="xs" value={s.camMode} onChange={(m) => actions.setCamMode(m)} ariaLabel={t('secCamera')}
          opts={[{ v: 'orbit', label: t('cam_orbit'), tip: t('cam_orbit_tip') }, { v: 'free', label: t('cam_free'), tip: t('cam_free_tip') }, { v: 'game', label: t('cam_game'), tip: t('cam_game_tip') }]} />
      </Section>

      <Section title={t('secGuns')} right={<Tip label={t('combatTip')}><span className="faint">C</span></Tip>}>
        <div className="ctl-row">
          <Seg block size="xs" value={s.combat ? 'combat' : 'port'} onChange={(v) => actions.setCombat(v === 'combat')} opts={[{ v: 'port', label: t('statePort'), tip: t('statePortTip') }, { v: 'combat', label: t('stateCombat'), tip: t('stateCombatTip') }]} />
        </div>
        <Prop k={t('aimMouse')}><Switch on={s.aimMouse} onChange={actions.setAimMouse} label={t('aimMouse')} /><span className="faint" style={{ marginLeft: 8 }}>{t('aimMouseHint')}</span></Prop>
        <Prop k={t('aimDist')} src="approx" srcTip={t('approxElevTip')}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%' }}><Slider value={s.aimKm} min={1} max={30} step={0.5} onChange={actions.setAimKm} label={t('aimDist')} /><span className="num" style={{ width: 52, textAlign: 'right', whiteSpace: 'nowrap' }}>{s.aimKm.toFixed(1)} km</span></div>
        </Prop>
        <Prop k={t('reloadReal')} src="exact" srcTip={t('reloadRealTip')}><Switch on={s.reloadReal} onChange={actions.setReloadReal} label={t('reloadReal')} /></Prop>
        {groups.map((g) => {
          const ms = mounts.filter((m) => m.group === g); if (!ms.length) return null;
          const spec = sim.mounts[ms[0].id];
          return (
            <div key={g} className="gungroup">
              <div className="firerow">
                <span className="lbl">{t('group_' + g)}<span className="faint num"> · {ms.length}× {spec?.barrelDiameter ? Math.round(spec.barrelDiameter * 1000) + ' mm' : ''}</span></span>
                <Tip label={t('salvoTip')} kbd={g === 'main' ? 'Space' : 'R'}><button className="btn" onClick={() => actions.fire(g, 'salvo')}><Flame size={14} />{t('salvo')}</button></Tip>
                <Tip label={t('singleTip')} kbd={g === 'main' ? 'F' : undefined}><button className="btn" onClick={() => actions.fire(g, 'single')}>{t('single1')}</button></Tip>
              </div>
              <div className="mounts">
                {ms.map((m) => {
                  const sp = sim.mounts[m.id]; const st = !s.combat ? 'rest' : !m.inSector ? 'blocked' : m.reload > 0 ? 'reload' : m.aimed ? 'ready' : 'turning';
                  return (
                    <Tip key={m.id} side="left" label={<div className="num" style={{ lineHeight: '18px' }}>
                      <div><b>{m.id}</b> · {sp?.model}</div>
                      <div>{t('rotSpeed')}: {sp?.rotationSpeed ? `${sp.rotationSpeed[0]}°/s` : '–'} <span className="faint">({t('src_exact')})</span></div>
                      <div>{t('horizSector')}: {sp?.horizSector ? `${sp.horizSector[0]}° … ${sp.horizSector[1]}°` : '–'}</div>
                      <div>{t('vertSector')}: {sp?.vertSector ? `${sp.vertSector[0]}° … ${sp.vertSector[1]}°` : '–'}</div>
                      <div>{t('reloadTime')}: {sp?.shotDelay ? sp.shotDelay.toFixed(1) + ' s' : '–'}</div>
                    </div>}>
                      <div className="mount" data-st={st}>
                        <span className="id mono">{m.id.replace('HP_', '')}</span>
                        <span className="num">{fmtDeg(m.yaw)}</span>
                        <span className="num faint">{m.pitch.toFixed(1)}°</span>
                        <span className="st">{st === 'reload' ? <span className="num">{m.reload.toFixed(1)}s</span> : t('mst_' + st)}</span>
                        <i className="bar" style={{ width: m.reloadMax ? (1 - m.reload / m.reloadMax) * 100 + '%' : '100%' }} />
                      </div>
                    </Tip>
                  );
                })}
              </div>
            </div>
          );
        })}
        <Prop k={t('rotLimits')} src="exact" srcTip={t('srcGP')}><span className="faint">{t('rotLimitsDesc')}</span></Prop>
        <Prop k={t('fx')} src="approx" srcTip={t('approxFxTip')}><span className="faint">{t('fxDesc')}</span></Prop>
        <Prop k={t('aaMounts')}><span className="faint">{t('aaStatic')}</span></Prop>
      </Section>

      <Section title={t('secEmis')}>
        <Seg block size="xs" value={s.emisMode} onChange={(v) => actions.setEmis(v)} ariaLabel={t('secEmis')}
          opts={[{ v: -1, label: t('em_mod'), tip: t('em_mod_tip') }, { v: 0, label: t('em_0') }, { v: 1, label: t('em_1') }, { v: 2, label: t('em_2'), tip: t('em_2_tip') }, { v: 3, label: t('em_3') }]} />
        <Prop k={t('emSpeed')} src="approx" srcTip={t('approxEmisTip')}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%' }}><Slider value={s.emisSpeed} min={0.1} max={4} step={0.1} onChange={(v) => actions.setEmis(s.emisMode, v)} label={t('emSpeed')} /><span className="num" style={{ width: 40, textAlign: 'right' }}>{s.emisSpeed.toFixed(1)} Hz</span></div>
        </Prop>
        <Prop k={t('emFormula')} src="exact" srcTip={t('emFormulaTip')}><span className="mono faint" style={{ fontSize: 11 }}>mg.B² · albedo · P(t)</span></Prop>
      </Section>

      <Section title={t('secLod')}>
        <div className="ctl-row">
          <span className="lbl">{t('distance')}</span>
          <Seg block size="xs" value={s.distPreset || ''} onChange={(v) => actions.setDist(v as any)} opts={[{ v: 'near', label: t('d_near'), kbd: '1' }, { v: 'mid', label: t('d_mid'), kbd: '2' }, { v: 'far', label: t('d_far'), kbd: '3' }]} />
        </div>
        <div className="ctl-row">
          <span className="lbl">LOD</span>
          <Seg block size="xs" value={String(s.lodMode)} onChange={(v) => actions.setLodMode(v === 'auto' ? 'auto' : +v)} opts={[{ v: 'auto', label: t('auto') }, { v: '0', label: '0' }, { v: '1', label: '1' }, { v: '2', label: '2' }, { v: '3', label: '3' }]} />
        </div>
        <Prop k={t('lodTint')}><Switch on={s.lodTint} onChange={actions.setLodTint} label={t('lodTint')} />{s.lodTint && <span className="legend"><i className="l0" />0<i className="l1" />1<i className="l2" />2<i className="l3" />3</span>}</Prop>
        <Prop k={t('lodBias')} src="approx" srcTip={t('approxLodTip')}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%' }}><Slider value={s.lodBias} min={0.25} max={3} step={0.05} onChange={actions.setLodBias} label={t('lodBias')} /><span className="num" style={{ width: 34, textAlign: 'right' }}>{s.lodBias.toFixed(2)}×</span></div>
        </Prop>
        {sn && <LodReadout />}
      </Section>
      <div className="legend-foot"><span className="src exact">{t('src_exact')}</span>{t('legendExact')}<span className="src approx" style={{ marginLeft: 8 }}>{t('src_approx')}</span>{t('legendApprox')}</div>
    </div>
  );
}
const fmtDeg = (d: number) => (d >= 0 ? '+' : '') + d.toFixed(0) + '°';

function LodReadout() {
  const sn = useStore((s) => s.snap)!; const parts = useStore((s) => s.parts);
  const groups = ['hull', 'main', 'secondary', 'aa'] as const;
  return (
    <table className="tbl num" style={{ marginTop: 4 }}>
      <thead><tr><th>{t('group')}</th><th>L0</th><th>L1</th><th>L2</th><th>L3</th><th>{t('hiddenShort')}</th></tr></thead>
      <tbody>
        {groups.map((g) => {
          const ps = parts.filter((p) => p.group === g && p.lods && p.lods.length); if (!ps.length) return null;
          const c = [0, 0, 0, 0, 0]; ps.forEach((p) => { const l = sn.lod[p.id]; c[l === undefined || l < 0 ? 4 : Math.min(3, l)]++; });
          return <tr key={g}><td>{t('group_' + g)}</td>{c.map((n, i) => <td key={i} className={n ? '' : 'faint'}>{n || '·'}</td>)}</tr>;
        })}
      </tbody>
      <caption className="faint">{t('camDist')}: {sn.camDist.toFixed(1)} · {t('lodRule')}</caption>
    </table>
  );
}
