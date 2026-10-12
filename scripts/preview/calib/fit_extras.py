#!/usr/bin/env python3
"""预览侧补偿: contactAO 固定(观感), 网格搜索 goldHue×periHue 专压金色/peri/royal ΔE。
用法: PYTHONUNBUFFERED=1 python3 fit_extras.py --spec ... --work ... --write gamelook.json
"""
import argparse, json, os, subprocess, sys
import numpy as np
from PIL import Image
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cmetrics import lab, de2000
from fit_game_look import W, H, wowsmod, masks, port_targets, trimmed

def render(proj, query, out, gl):
    q = f"{query}&w={W}&h={H}&env=water&water=full&wm=0&gl={json.dumps(gl,separators=(',',':'))}"
    r = subprocess.run([wowsmod(), '--json', 'render', proj, '--look', 'game-day', '--query', q, '-o', out],
                       capture_output=True, text=True)
    if r.returncode != 0 or '"ok": true' not in r.stdout:
        raise SystemExit(r.stdout[-500:] + r.stderr[-300:])

def eval_knobs(spec, work, knobs, tag):
    T = port_targets(spec); rows = []; proj = spec['project']
    os.makedirs(f'{work}/extras', exist_ok=True)
    for s in spec['shots']:
        M = masks(spec, s, work)
        o = f"{work}/extras/{s['name']}_{tag}.png"
        render(proj, s['query'], o, knobs)
        img = np.asarray(Image.open(o).convert('RGB'), float)
        for cls, m in M.items():
            if m.sum() < 30: continue
            tgt = T[(s['name'], cls)][0]
            v = trimmed(img[m])
            rows.append(dict(shot=s['name'], cls=cls, de=float(de2000(lab(v), lab(tgt))),
                             w=s['regions'][cls].get('w', 1.0)))
    mean = float(np.average([r['de'] for r in rows], weights=[r['w'] for r in rows]))
    focus = [r['de'] for r in rows if r['cls'] in ('gold','peri','royal')]
    fmean = float(np.mean(focus)) if focus else mean
    return mean, fmean, rows

if __name__ == '__main__':
    ap = argparse.ArgumentParser()
    ap.add_argument('--spec', required=True); ap.add_argument('--work', required=True)
    ap.add_argument('--write'); ap.add_argument('--ao', type=float, default=0.45)
    ap.add_argument('--water', type=float, default=1.45)
    args = ap.parse_args()
    spec = json.load(open(args.spec))
    base = dict(contactAO=0, goldHue=0, periHue=0, waterOpa=1.0)
    bmean, bf, brows = eval_knobs(spec, args.work, base, 'base0')
    print(f'baseline mean={bmean:.2f} focus(gold/peri/royal)={bf:.2f}')
    # grid
    best = None
    for gh in [0, 8, 14, 20, 26]:
        for ph in [0, 6, 12, 18]:
            k = dict(contactAO=args.ao, goldHue=gh, periHue=ph, waterOpa=args.water)
            tag = f'g{gh}_p{ph}'
            m, f, rows = eval_knobs(spec, args.work, k, tag)
            # 目标: 压 focus, 整体别差过 baseline+0.8
            score = f + max(0, m - bmean - 0.8) * 2
            print(f'  ao={args.ao} gold={gh} peri={ph} → mean={m:.2f} focus={f:.2f} score={score:.2f}')
            if best is None or score < best[0]:
                best = (score, m, f, k, rows)
    score, m, f, k, rows = best
    print(f'BEST {k} mean={m:.2f} focus={f:.2f} (baseline mean={bmean:.2f} focus={bf:.2f})')
    for r in sorted(rows, key=lambda x: -x['de']):
        print(f'  {r["shot"]}/{r["cls"]}: {r["de"]:.2f}')
    json.dump(dict(baseline=dict(mean=bmean, focus=bf), best=dict(mean=m, focus=f, knobs=k),
                   rows=rows), open(f'{args.work}/extras/fit_extras.json','w'), ensure_ascii=False, indent=1)
    if args.write:
        G = json.load(open(args.write)); G['day'].update(k)
        # night: stronger AO, milder hue (desat scene)
        G['night']['contactAO'] = min(0.75, k['contactAO'] + 0.15)
        G['night']['goldHue'] = max(0, k['goldHue'] * 0.45)
        G['night']['periHue'] = max(0, k['periHue'] * 0.4)
        G['night']['waterOpa'] = 1.35
        G['version'] = G.get('version', 0) + 1
        G['_fit_extras'] = dict(baseline_mean=round(bmean,3), best_mean=round(m,3),
                                baseline_focus=round(bf,3), best_focus=round(f,3))
        json.dump(G, open(args.write,'w'), ensure_ascii=False, indent=1)
        print('wrote', args.write)
