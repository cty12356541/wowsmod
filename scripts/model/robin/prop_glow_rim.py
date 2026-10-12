"""外带道具发光描边(P5.1+): 在共享图集基础上为个别道具生成专用 _a / 发光遮罩, 夜里靠发光读出轮廓。
  python3 prop_glow_rim.py <props_dir> <out_dir> [--rim 0.6] [--gold 0.55] [--rim-w 0.07] [--tint 0.6] [--fill 0]
输出:
  wing_atlas.png  翼用 _a: 羽片列(0,1)两侧外缘细带向发光色 #58B8FF 染 --tint(白天只多一条淡天青描边)
  wing_glow.png   翼用遮罩: 原发光列(7)=1.0, 羽片外缘细带=--rim(相对船体主发光线 1.0), 羽片整面底光=--fill
                  (r42: 夜里翼下表面背光=暗剪影, 只有描边; fill≈0.2 让任意角度都能读出羽面)
  halo_glow.png   光环用遮罩: 原发光列=1.0, 金色列(6, 金环/金饰)=--gold → 暖金微光(发光色=漫反射金)
遮罩值 w → _mg B = sqrt(w)·255, 着色器发光 ∝ B² → 亮度与 w 成正比。
羽片 UV: u=横向(图集列内 0.12..0.88), v=沿长; 只按 u 取外缘(不按 v 取梢, 引擎 v 可能翻转, 见 prop_atlas.py 翻转安全)。
用法(项目内):
  wowsmod tex import wing_atlas.png --like rb_wing_L_a.dds --project P   (R 同)
  wowsmod glow --project P --mg rb_wing_L_mg.dds --mask wing_glow.png --power 0.82 --mfm rb_wing_L.mfm   (R 同)
  wowsmod glow --project P --mg rb_halo_mg.dds --mask halo_glow.png --power 0.82 --mfm rb_halo.mfm
"""
import sys, os, json, argparse, numpy as np
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument('props'); ap.add_argument('out')
ap.add_argument('--rim', type=float, default=0.6); ap.add_argument('--gold', type=float, default=0.55)
ap.add_argument('--rim-w', type=float, default=0.07); ap.add_argument('--tint', type=float, default=0.6)
ap.add_argument('--fill', type=float, default=0.0)
a = ap.parse_args()
os.makedirs(a.out, exist_ok=True)
meta = json.load(open(f'{a.props}/prop_atlas.json')); cw = meta['cell']; size = meta['size']
cols = meta['cols']; GLC, GOLD = cols.index('glow'), cols.index('gold')
atl = np.asarray(Image.open(f'{a.props}/prop_atlas.png').convert('RGB'), float)
base = np.asarray(Image.open(f'{a.props}/prop_glow.png').convert('L'), float) / 255.0
gl_rgb = atl[size // 2, GLC * cw + cw // 2]          # 图集发光列颜色(= #58B8FF 经港口补偿)

lu = (np.arange(cw) + 0.5) / cw                       # 列内横坐标
fu = (lu - 0.12) / 0.76                               # 羽片 UV u
rim = (fu < a.rim_w) | (fu > 1 - a.rim_w)             # 含列内衬边(防 mip 串色)
soft = np.clip(np.minimum(fu, 1 - fu) / a.rim_w, 0, 1); soft = np.where(rim, 1 - 0.35 * soft, 0)  # 最外缘最亮

wing_a = atl.copy(); wing_m = base.copy()
for k in (cols.index('feather_wp'), cols.index('feather_pv')):
    sl = slice(k * cw, (k + 1) * cw)
    t = (a.tint * soft)[None, :, None]
    wing_a[:, sl] = wing_a[:, sl] * (1 - t) + gl_rgb * t
    face = np.where((fu > 0) & (fu < 1), a.fill, 0.0)
    wing_m[:, sl] = np.maximum(wing_m[:, sl], np.maximum(a.rim * soft, face)[None, :])
halo_m = base.copy(); halo_m[:, GOLD * cw:(GOLD + 1) * cw] = np.maximum(halo_m[:, GOLD * cw:(GOLD + 1) * cw], a.gold)

Image.fromarray(wing_a.clip(0, 255).astype(np.uint8)).save(f'{a.out}/wing_atlas.png')
for n, m in (('wing_glow', wing_m), ('halo_glow', halo_m)):
    Image.fromarray((m.clip(0, 1) * 255 + 0.5).astype(np.uint8)).save(f'{a.out}/{n}.png')
json.dump(dict(rim=a.rim, fill=a.fill, gold=a.gold, rim_w=a.rim_w, tint=a.tint, glow_rgb=gl_rgb.tolist(),
               note='intensity relative to hull main glow line (mask 1.0, same emissivePower)'),
          open(f'{a.out}/rim.json', 'w'), indent=1)
print('ok', a.out)
