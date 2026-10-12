"""r42 坐标下降: 用 regions_r42.json 的分区 ΔE 拟合 game_look 日/夜旋钮(gl= 覆盖, 不改文件)。
用法: python3 fit_r42.py <calib_r42 dir> day|night '<start gl json>' knob=lo:hi:step ...
依赖 calib_r42/render.sh 风格的 shots.sh(名 look 镜像 query) 和 /tmp/rq.sh 等价渲染器(RQ 环境变量可改)。"""
import json, os, subprocess, sys, urllib.parse
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cmetrics import load, _cache
import r42_regional as R

D = sys.argv[1]; light = sys.argv[2]; cur = json.loads(sys.argv[3]); knobs = sys.argv[4:]
RQ = os.environ.get('RQ', '/tmp/rq.sh')
spec = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'regions_r42.json')))
shots = [l.split() for l in open(f'{D}/shots.sh') if l.strip() and not l.startswith('#')]
shots = [s for s in shots if s[1].endswith(light)]
W = {'cyan': 0.5, 'gold': 0.5}  # 细线类权重低(占比极小, 位置误差大)

def score(gl, tag):
    out = f'{D}/fit_{light}'; os.makedirs(out, exist_ok=True); tot = []
    for n, look, m, q in shots:
        f = f'{out}/{n}.png'
        subprocess.run([RQ, look, q + '&gl=' + urllib.parse.quote(json.dumps(gl)), f], env=dict(os.environ, W='1280', H='720'), check=True)
        if m == '1':
            from PIL import Image, ImageOps; ImageOps.mirror(Image.open(f)).save(f)
        _cache.clear(); s = spec['shots'][n]
        P = load(f'{D}/port/{n}.png', (1280, 720)); V = load(f, (1280, 720))
        for c, fl in spec['classes'].items():
            a, _ = R.stats(P, s['port']['hull'], fl); b, _ = R.stats(V, s['prev']['hull'], fl)
            if a is not None and b is not None: tot.append(W.get(c, 1) * float(R.de2000(R.lab(a), R.lab(b))))
        for k in ('wing', 'halo'):
            if light == 'night' and k == 'wing': continue  # 夜翅膀实拍受 A 问题影响(暗), 不拟合
            fr = 0.4 if k == 'wing' else -0.4
            tot.append(float(R.de2000(R.lab(R.bright(P, s['port'][k], fr)), R.lab(R.bright(V, s['prev'][k], fr)))))
    v = float(np.mean(tot)); print(f'{tag:28s} {v:6.3f} {json.dumps(gl)}', flush=True); return v

best = score(cur, 'start')
for rnd in range(2):
    for kd in knobs:
        k, rng = kd.split('='); lo, hi, st = map(float, rng.split(':'))
        for d in (st, -st):
            while True:
                nv = round(cur.get(k, 0) + d, 4)
                if nv < lo or nv > hi: break
                t = dict(cur, **{k: nv}); v = score(t, f'r{rnd} {k}={nv}')
                if v < best - 0.02: best, cur = v, t
                else: break
print('BEST', best, json.dumps(cur))
