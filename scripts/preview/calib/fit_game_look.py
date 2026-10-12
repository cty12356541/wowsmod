#!/usr/bin/env python3
"""游戏对齐(港口)光照拟合 —— 让工作台「游戏内」预览贴近港口实拍。

流程(可复用: 新增港口实拍 → 在区域规格里加一条 shot → 重跑):
  1. basis  : 对每个 shot 的机位, 用 `wowsmod render --look game-day --query ...&gl=<override>` 出
              albedo / normal 通道 + 4 张「原始辐射度」基图(raw 模式, 无色调映射):
              太阳(白, I=1) / 天空半球(白) / 地面半球(白) / 环境贴图(envI=1)。光照对辐射度线性 →
              任意灯光颜色强度的辐射度 = 基图的逐通道线性组合, 可离线拟合不必反复出图。
  2. fit    : 区域 = 港口图像素框(+色相过滤) ↔ 工作台像素(albedo 色类 ∩ 朝向 ∩ 受光/背光)。
              参数: 太阳 RGB、天空 RGB、地面 RGB、envI、toe(3)、曲线指数 tmP、饱和度 sat;
              目标: 各区域 ΔE2000 均值(自写 Nelder–Mead, 只依赖 numpy + Pillow)。
              结果写回 preview 前端 src/engine/gamelook.json 的 day 预设(--write)。
  3. eval   : 真实出图(展台 studio / 游戏内 game-day)同掩码量 ΔE2000, 出表 + 并排图(港口 | 工作台)。

用法:
  python3 fit_game_look.py --spec regions_p2v6.json --work ./work/e2e/calib/fit basis
  python3 fit_game_look.py --spec ... --work ... fit [--write <gamelook.json>]
  python3 fit_game_look.py --spec ... --work ... eval --tag after
需要: 常驻出图实例(wowsmod render --warm <proj>)可大幅提速; 盒子上无游戏时设 WOWSMOD_GAME。
"""
import argparse, json, os, subprocess, sys, math
import numpy as np
from PIL import Image
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cmetrics import srgb2lin, lin2srgb, lab, de2000, hsv, load, region

W, H = 1280, 720
RAW = {'sun': 0.25, 'sky': 0.5, 'gnd': 0.5, 'env': 0.5}
OFF = {'bloomS': 0, 'emisK': 0, 'fogDen': 0, 'exposure': 1}
BASIS = {
    'sun': dict(OFF, raw=RAW['sun'], sunI=1, sun='#ffffff', hemi=0, envI=0),
    'sky': dict(OFF, raw=RAW['sky'], sunI=0, hemi=1, hemiSky='#ffffff', hemiGround='#000000', envI=0),
    'gnd': dict(OFF, raw=RAW['gnd'], sunI=0, hemi=1, hemiSky='#000000', hemiGround='#ffffff', envI=0),
    'env': dict(OFF, raw=RAW['env'], sunI=0, hemi=0, envI=1),
}
KEYS = list(BASIS)

def wowsmod():
    return os.environ.get('WOWSMOD', './target/release/wowsmod')

def render(proj, query, out, look='game-day', extra=''):
    q = f"{query}&w={W}&h={H}&env=water&water=full&wm=0{extra}"
    r = subprocess.run([wowsmod(), '--json', 'render', proj, '--look', look, '--query', q, '-o', out], capture_output=True, text=True)
    if r.returncode != 0 or '"ok": true' not in r.stdout:
        raise SystemExit(f'render failed: {q}\n{r.stdout[-800:]}\n{r.stderr[-800:]}')

def gl(d): return '&gl=' + json.dumps(d, separators=(',', ':'))

# ---------------------------------------------------------------- 贴图色类
def tex_colors(spec):
    """贴图上的目标色(与画师脚本 to_tex 一致)"""
    pa = spec.get('port_albedo', {'slope': [0.7091, 1.1475, 1.1523], 'toe': [0.0249, 0.0294, 0.0137]})
    S, T = np.array(pa['slope']), np.array(pa['toe'])
    out = {}
    for k, hx in spec['palette'].items():
        c = np.array([int(hx[i:i + 2], 16) for i in (1, 3, 5)], float)
        m = spec.get('colmode', {}).get(k, 'raw')
        port = lin2srgb(T + srgb2lin(c / 255) / S) * 255
        out[k] = c if m == 'raw' else (0.5 * c + 0.5 * port if m == 'half' else port)
    return out

def erode(m, n=1):
    for _ in range(n):
        m = m & np.roll(m, 1, 0) & np.roll(m, -1, 0) & np.roll(m, 1, 1) & np.roll(m, -1, 1)
    return m

def masks(spec, shot, work):
    """工作台像素掩码: albedo 色类 ∩ 朝向 ∩ 受光条件 ∩ 可选框"""
    d = f"{work}/{shot['name']}"
    alb = load(f'{d}/albedo.png'); nrm = load(f'{d}/normal.png') / 255 * 2 - 1
    sun = srgb2lin(load(f'{d}/sun.png') / 255) / RAW['sun']
    tc = tex_colors(spec); names = list(tc)
    A = lab(alb); C = np.stack([lab(tc[k]) for k in names])
    dist = np.sqrt(((A[..., None, 0] - C[:, 0]) * 0.45) ** 2 + (A[..., None, 1] - C[:, 1]) ** 2 + (A[..., None, 2] - C[:, 2]) ** 2)
    near = dist.argmin(-1); dmin = dist.min(-1)
    el = math.radians(shot.get('el', 0)); up = np.array([0, math.cos(el), math.sin(el)])
    nd = nrm @ up
    face = {'top': nd > 0.8, 'side': np.abs(nd) < 0.35}[shot['facing']]
    albl = np.maximum(srgb2lin(alb / 255) @ np.array([0.2126, 0.7152, 0.0722]), 0.01)
    sunr = (sun @ np.array([0.2126, 0.7152, 0.0722])) / albl
    lit = {'sun': sunr > 0.35, 'shade': sunr < 0.02}[shot.get('light', 'sun' if shot['facing'] == 'top' else 'shade')]
    out = {}
    for cls, r in shot['regions'].items():
        wcls = r.get('wb_class', cls); wcl = [wcls] if isinstance(wcls, str) else wcls
        m = np.zeros(alb.shape[:2], bool)
        for c in wcl: m |= (near == names.index(c)) & (dmin < r.get('tol', 14))
        m = erode(m, r.get('erode', 1)) & face & lit
        if 'wb_boxes' in r:
            bm = np.zeros_like(m)
            for x0, y0, x1, y1 in r['wb_boxes']: bm[y0:y1, x0:x1] = True
            m &= bm
        out[cls] = m
    return out

def port_targets(spec):
    t = {}
    for s in spec['shots']:
        img = load(s['port'])
        for cls, r in s['regions'].items():
            vals = []; n = 0
            for b in r['boxes']:
                v, k = region(img, b, r.get('flt'));
                if v is not None: vals.append(v * k); n += k
            t[(s['name'], cls)] = (np.sum(vals, 0) / n, n)
    return t

def trimmed(rgb255):
    L = rgb255 @ np.array([0.2126, 0.7152, 0.0722]); lo, hi = np.percentile(L, [15, 85])
    q = rgb255[(L >= lo) & (L <= hi)]
    return (q if len(q) >= 4 else rgb255).mean(0)

# ---------------------------------------------------------------- 模型
def tone(rad, P):
    x = np.maximum(rad * P['gain'] - P['toe'], 0)
    xp = x ** P['tmP']; y = np.minimum(P.get('tmS', 1.0) * xp / (xp + 1), 1.0)
    l = y @ np.array([0.2126, 0.7152, 0.0722])
    return np.clip(l[..., None] + (y - l[..., None]) * P['sat'], 0, 1)

def unpack(v):
    e = np.exp
    return dict(sun=e(v[0:3]), sky=e(v[3:6]), gnd=e(v[6]) * SEA, envI=e(v[9]), toe=np.abs(v[10:13]) * 0.05,
                tmP=e(v[13]), sat=e(v[14]), tmS=1.0 + abs(v[15]) * 0.3, gain=np.ones(3))

SEA = srgb2lin(np.array([0x2f, 0x6f, 0x74]) / 255)  # 地面半球 = 海面色 × 强度(只拟合强度, 防止颜色乱跑)
SEA = SEA / SEA.max()

def predict(B, P):
    rad = B['sun'] * P['sun'] + B['sky'] * P['sky'] + B['gnd'] * P['gnd'] + B['env'] * P['envI']
    return lin2srgb(tone(rad, P)) * 255

def nelder_mead(f, x0, step=0.3, iters=4000, tol=1e-6):
    n = len(x0); pts = [np.array(x0, float)]
    for i in range(n):
        p = np.array(x0, float); p[i] += step; pts.append(p)
    fs = [f(p) for p in pts]
    for it in range(iters):
        o = np.argsort(fs); pts = [pts[i] for i in o]; fs = [fs[i] for i in o]
        if abs(fs[-1] - fs[0]) < tol and it > 200: break
        c = np.mean(pts[:-1], 0); xr = c + (c - pts[-1]); fr = f(xr)
        if fr < fs[0]:
            xe = c + 2 * (c - pts[-1]); fe = f(xe)
            if fe < fr: pts[-1], fs[-1] = xe, fe
            else: pts[-1], fs[-1] = xr, fr
        elif fr < fs[-2]: pts[-1], fs[-1] = xr, fr
        else:
            xc = c + 0.5 * (pts[-1] - c); fc = f(xc)
            if fc < fs[-1]: pts[-1], fs[-1] = xc, fc
            else:
                pts = [pts[0]] + [pts[0] + 0.5 * (p - pts[0]) for p in pts[1:]]; fs = [fs[0]] + [f(p) for p in pts[1:]]
    i = int(np.argmin(fs)); return pts[i], fs[i]

def hexof(c):
    s = lin2srgb(np.clip(c, 0, 1)) * 255
    return '#' + ''.join(f'{int(round(v)):02x}' for v in s)

# ---------------------------------------------------------------- 步骤
def step_basis(spec, work, swaps):
    proj = spec['project']
    for s in spec['shots']:
        d = f"{work}/{s['name']}"; os.makedirs(d, exist_ok=True)
        render(proj, s['query'], f'{d}/albedo.png', extra='&channel=albedo')
        render(proj, s['query'], f'{d}/normal.png', extra='&channel=normal')
        for sw in swaps:
            for k in KEYS:
                o = dict(BASIS[k], mgSwap=sw)
                render(proj, s['query'], f'{d}/{k}.png' if sw == 0 else f'{d}/{k}_mg{sw}.png', extra=gl(o))
        print('basis', s['name'], 'ok')

def collect(spec, work, sw):
    data = []
    T = port_targets(spec)
    for s in spec['shots']:
        d = f"{work}/{s['name']}"; M = masks(spec, s, work); suf = '' if sw == 0 else f'_mg{sw}'
        B = {k: srgb2lin(load(f'{d}/{k}{suf}.png') / 255) / RAW[k] for k in KEYS}
        for cls, m in M.items():
            tgt, n = T[(s['name'], cls)]
            if m.sum() < 30: print(f'  ! {s["name"]}/{cls}: 工作台像素太少({m.sum()}), 跳过'); continue
            w = s['regions'][cls].get('w', 1.0)
            idx = np.flatnonzero(m.ravel()); rng = np.random.default_rng(0)
            if len(idx) > 4000: idx = np.sort(rng.choice(idx, 4000, replace=False))
            data.append(dict(shot=s['name'], cls=cls, B={k: B[k].reshape(-1, 3)[idx] for k in KEYS}, tgt=tgt, tlab=lab(tgt), n=int(m.sum()), w=w))
    return data

def loss_of(data, P):
    es = []; ws = []
    for r in data:
        p = trimmed(predict(r['B'], P)); es.append(de2000(lab(p), r['tlab'])); ws.append(r['w'])
    return float(np.average(es, weights=ws)), es

def step_fit(spec, work, write, swaps):
    best = None
    for sw in swaps:
        data = collect(spec, work, sw)
        x0 = np.concatenate([np.log([2.5, 2.4, 2.2]), np.log([0.9, 1.0, 1.2]), np.log([0.25, 0.35, 0.35]), [np.log(0.6)], [0, 0, 0], [0.0], [0.0], [0.1]])
        reg = lambda v: 0.5 * v[13] ** 2 + 3.0 * v[14] ** 2 + 1e-3 * (v[7] ** 2 + v[8] ** 2)
        f = lambda v: loss_of(data, unpack(v))[0] + reg(v)
        x, fx = x0, f(x0)
        for rnd in range(4):
            x, fx = nelder_mead(f, x, step=0.4 / (rnd + 1), iters=3000)
        L, es = loss_of(data, unpack(x))
        print(f'mgSwap={sw}: mean ΔE2000 = {L:.2f}')
        for r, e in zip(data, es): print(f'   {r["shot"]:>5}/{r["cls"]:<8} ΔE {e:5.2f}  (n={r["n"]})')
        if best is None or L < best[0]: best = (L, sw, x, data)
    L, sw, x, data = best; P = unpack(x)
    sunI = float(P['sun'].max()); hemi = float(max(P['sky'].max(), P['gnd'].max()))  # three.js 半球灯: 天/地两色共用强度
    res = dict(sun=hexof(P['sun'] / sunI), sunI=round(sunI, 4), hemiSky=hexof(P['sky'] / hemi), hemiGround=hexof(P['gnd'] / hemi), hemi=round(hemi, 4),
               envI=round(float(P['envI']), 4), gain=[1.0, 1.0, 1.0], toe=[round(float(t), 5) for t in P['toe']], tmP=round(float(P['tmP']), 4),
               sat=round(float(P['sat']), 4), tmS=round(float(P['tmS']), 4), mgSwap=sw)
    print(json.dumps(res, ensure_ascii=False))
    json.dump(dict(fit=res, loss=L, regions=[dict(shot=r['shot'], cls=r['cls'], n=r['n']) for r in data]), open(f'{work}/fit_day.json', 'w'), ensure_ascii=False, indent=1)
    if write: apply(work, write)

def apply(work, write):
    """把 work/fit_day.json 的结果写进前端预设(只改 day 里拟合出的键, 其余手调键不动)"""
    F = json.load(open(f'{work}/fit_day.json'))
    G = json.load(open(write)); G['day'].update(F['fit']); G['version'] = G.get('version', 0) + 1
    G['_fit'] = dict(spec=os.path.basename(args.spec), loss_day=round(F['loss'], 3), regions=len(F['regions']))
    json.dump(G, open(write, 'w'), ensure_ascii=False, indent=1); print('写入', write)

def step_eval(spec, work, tag, looks):
    """真实出图 + 同掩码量 ΔE; looks: [(label, look, extra)]"""
    T = port_targets(spec); rows = []; proj = spec['project']
    os.makedirs(f'{work}/eval', exist_ok=True)
    for s in spec['shots']:
        M = masks(spec, s, work); d = f"{work}/{s['name']}"
        imgs = {}
        for lab_, look, extra in looks:
            o = f'{work}/eval/{s["name"]}_{tag}_{lab_}.png'
            ex = extra
            if look == 'studio': ex += ''
            q = s['query']
            r = subprocess.run([wowsmod(), '--json', 'render', proj, '--look', look, '--query', f"{q}&w={W}&h={H}&wm=0{ex}", '-o', o], capture_output=True, text=True)
            if '"ok": true' not in r.stdout: raise SystemExit(r.stdout[-500:])
            imgs[lab_] = np.asarray(Image.open(o).convert('RGB'), float)
        for cls, m in M.items():
            if m.sum() < 30: continue
            tgt = T[(s['name'], cls)][0]
            row = dict(shot=s['name'], cls=cls, port=[round(float(v)) for v in tgt])
            for lab_ in imgs:
                v = trimmed(imgs[lab_][m]); row[lab_] = dict(rgb=[round(float(c)) for c in v], de=round(float(de2000(lab(v), lab(tgt))), 2))
            rows.append(row)
        # 并排图: 港口 | 各 look, 掩码区域描边
        port = Image.open(s['port']).convert('RGB')
        cw = port.size[0] * H // port.size[1]; port = port.resize((cw, H))
        tiles = [port] + [Image.fromarray(imgs[k].astype(np.uint8)) for k in imgs]
        outl = Image.new('RGB', (sum(t.size[0] for t in tiles), H + 28), (246, 247, 249))
        from PIL import ImageDraw
        dr = ImageDraw.Draw(outl); x = 0
        for name, t in zip(['港口实拍 port'] + list(imgs), tiles):
            outl.paste(t, (x, 28)); dr.text((x + 8, 8), name, fill=(40, 44, 52)); x += t.size[0]
        outl.save(f'{work}/eval/{s["name"]}_{tag}_side.jpg', quality=88)
    json.dump(rows, open(f'{work}/eval/{tag}.json', 'w'), ensure_ascii=False, indent=1)
    labs = [l for l, _, _ in looks]
    print(f'{"shot/region":<16}' + ''.join(f'{l:>12}' for l in labs))
    for r in rows: print(f'{r["shot"] + "/" + r["cls"]:<16}' + ''.join(f'{r[l]["de"]:>12.2f}' for l in labs))
    print(f'{"mean":<16}' + ''.join(f'{np.mean([r[l]["de"] for r in rows]):>12.2f}' for l in labs))

if __name__ == '__main__':
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--spec', required=True); ap.add_argument('--work', required=True)
    ap.add_argument('step', choices=['basis', 'fit', 'apply', 'eval', 'all'])
    ap.add_argument('--write', help='把拟合结果写入 gamelook.json(day 预设)')
    ap.add_argument('--swaps', default='0,1', help='_mg 通道解释: 0=R光泽/G金属(文档), 1=对调')
    ap.add_argument('--tag', default='eval')
    ap.add_argument('--looks', default='studio:studio:&env=studio,game:game-day:&env=water&water=full',
                    help='label:look:extra, 逗号分隔')
    args = ap.parse_args()
    spec = json.load(open(args.spec)); swaps = [int(s) for s in args.swaps.split(',')]
    if args.step in ('basis', 'all'): step_basis(spec, args.work, swaps)
    if args.step in ('fit', 'all'): step_fit(spec, args.work, args.write, swaps)
    if args.step == 'apply': apply(args.work, args.write)
    if args.step in ('eval', 'all'):
        looks = [tuple(x.split(':', 2)) for x in args.looks.split(',')]
        step_eval(spec, args.work, args.tag, looks)
