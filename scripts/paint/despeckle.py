"""原壳降噪「融」: 把 _a 上的小暗斑(舷窗/铆钉/流锈/小舱口印)填成周围色块, 保留色块边界与亮的发光线.
用法: despeckle.py <in.png> <out.png> [--mask side.png] [--rows v0:v1 ...] [--k 7] [--thr 10] [--scale 4] [--report r.json]
  --rows: 只处理的行带(0..1, 图像上沿=0), 可重复; 缺省整张
  做法: 降采样 1/scale → 中值滤波 k → 升采样得 med; 原像素比 med 暗 > thr(亮度) 且 色相接近(色度差<--chroma) 且 med 饱和度>--min-sat 的视为小斑, 用 med 替换
  不碰: 比 med 亮的像素(发光线/金饰), 色度差大的(色块边/图案)"""
import sys, json, argparse, numpy as np
from PIL import Image, ImageFilter
ap = argparse.ArgumentParser(); ap.add_argument('inp'); ap.add_argument('out')
ap.add_argument('--rows', action='append', default=[]); ap.add_argument('--k', type=int, default=7)
ap.add_argument('--thr', type=float, default=10); ap.add_argument('--scale', type=int, default=4); ap.add_argument('--report'); ap.add_argument('--mask', help='只在遮罩内处理(PNG 白=处理, 如 paint 产出的 hull_side_mask.png; 比 --rows 通用)'); ap.add_argument('--chroma', type=float, default=0.06); ap.add_argument('--min-sat', type=float, default=30)
a = ap.parse_args()
im = Image.open(a.inp).convert('RGB'); W, H = im.size
med = im.resize((W // a.scale, H // a.scale), Image.BOX).filter(ImageFilter.MedianFilter(a.k)).resize((W, H), Image.BILINEAR)
o = np.asarray(im, float); m = np.asarray(med, float)
lum = lambda x: x @ np.array([0.299, 0.587, 0.114])
lo, lm = lum(o), lum(m)
cho = o / np.maximum(o.sum(-1, keepdims=True), 1); chm = m / np.maximum(m.sum(-1, keepdims=True), 1)
sel = (lm - lo > a.thr) & (np.abs(cho - chm).sum(-1) < a.chroma) & (m.max(-1) - m.min(-1) > a.min_sat)  # 低饱和(白/米色)底上的印字/铭牌不动
band = np.zeros(H, bool)
for r in a.rows or ['0:1']:
    v0, v1 = map(float, r.split(':')); band[int(v0 * H):int(v1 * H)] = True
sel &= band[:, None]
if a.mask: sel &= np.asarray(Image.open(a.mask).convert('L').resize((W, H), Image.NEAREST)) > 127
sel = np.asarray(Image.fromarray((sel * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(5))) > 0  # 斑边两圈也换, 免留晕
o[sel] = m[sel]
Image.fromarray(o.clip(0, 255).astype(np.uint8)).save(a.out)
rep = dict(ok=True, size=[W, H], replaced_px=int(sel.sum()), frac=round(float(sel.mean()), 4), rows=a.rows)
print(json.dumps(rep)); a.report and json.dump(rep, open(a.report, 'w'))
