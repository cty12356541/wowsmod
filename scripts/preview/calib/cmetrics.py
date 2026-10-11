"""色彩度量小库(只依赖 numpy + Pillow): sRGB↔线性、CIELAB(D65)、ΔE2000、区域取色(框 + 可选颜色过滤)。
被 fit_game_look.py / regions 校验图复用。"""
import numpy as np
from PIL import Image

def srgb2lin(c):
    c = np.asarray(c, float)
    return np.where(c > 0.04045, ((c + 0.055) / 1.055) ** 2.4, c / 12.92)

def lin2srgb(l):
    l = np.clip(np.asarray(l, float), 0, 1)
    return np.where(l > 0.0031308, 1.055 * l ** (1 / 2.4) - 0.055, 12.92 * l)

_M = np.array([[0.4124564, 0.3575761, 0.1804375], [0.2126729, 0.7151522, 0.0721750], [0.0193339, 0.1191920, 0.9503041]])
_WP = np.array([0.95047, 1.0, 1.08883])

def lab(rgb255):
    """sRGB 0..255 → CIELAB(D65)"""
    l = srgb2lin(np.asarray(rgb255, float) / 255.0)
    xyz = l @ _M.T / _WP
    f = np.where(xyz > (6 / 29) ** 3, np.cbrt(xyz), xyz / (3 * (6 / 29) ** 2) + 4 / 29)
    return np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]), 200 * (f[..., 1] - f[..., 2])], -1)

def de2000(a, b):
    """CIEDE2000, a/b: (...,3) Lab"""
    a = np.asarray(a, float); b = np.asarray(b, float)
    L1, a1, b1 = a[..., 0], a[..., 1], a[..., 2]; L2, a2, b2 = b[..., 0], b[..., 1], b[..., 2]
    C1 = np.hypot(a1, b1); C2 = np.hypot(a2, b2); Cb = (C1 + C2) / 2
    G = 0.5 * (1 - np.sqrt(Cb ** 7 / (Cb ** 7 + 25 ** 7)))
    a1p = (1 + G) * a1; a2p = (1 + G) * a2
    C1p = np.hypot(a1p, b1); C2p = np.hypot(a2p, b2)
    h1p = np.degrees(np.arctan2(b1, a1p)) % 360; h2p = np.degrees(np.arctan2(b2, a2p)) % 360
    dLp = L2 - L1; dCp = C2p - C1p
    dhp = h2p - h1p; dhp = np.where(dhp > 180, dhp - 360, dhp); dhp = np.where(dhp < -180, dhp + 360, dhp)
    dhp = np.where(C1p * C2p == 0, 0, dhp)
    dHp = 2 * np.sqrt(C1p * C2p) * np.sin(np.radians(dhp / 2))
    Lbp = (L1 + L2) / 2; Cbp = (C1p + C2p) / 2
    hsum = h1p + h2p
    hbp = np.where(np.abs(h1p - h2p) > 180, (hsum + 360) / 2, hsum / 2); hbp = np.where(C1p * C2p == 0, hsum, hbp)
    T = 1 - 0.17 * np.cos(np.radians(hbp - 30)) + 0.24 * np.cos(np.radians(2 * hbp)) + 0.32 * np.cos(np.radians(3 * hbp + 6)) - 0.20 * np.cos(np.radians(4 * hbp - 63))
    dth = 30 * np.exp(-((hbp - 275) / 25) ** 2)
    Rc = 2 * np.sqrt(Cbp ** 7 / (Cbp ** 7 + 25 ** 7))
    Sl = 1 + 0.015 * (Lbp - 50) ** 2 / np.sqrt(20 + (Lbp - 50) ** 2); Sc = 1 + 0.045 * Cbp; Sh = 1 + 0.015 * Cbp * T
    Rt = -np.sin(np.radians(2 * dth)) * Rc
    return np.sqrt((dLp / Sl) ** 2 + (dCp / Sc) ** 2 + (dHp / Sh) ** 2 + Rt * (dCp / Sc) * (dHp / Sh))

def hsv(rgb255):
    r, g, b = [np.asarray(rgb255, float)[..., i] / 255 for i in range(3)]
    mx = np.maximum(np.maximum(r, g), b); mn = np.minimum(np.minimum(r, g), b); d = mx - mn + 1e-9
    h = np.where(mx == r, ((g - b) / d) % 6, np.where(mx == g, (b - r) / d + 2, (r - g) / d + 4)) * 60
    s = np.where(mx > 0, (mx - mn) / (mx + 1e-9), 0)
    return h, s, mx

_cache = {}
def load(path, size=None):
    """读图(缓存), 可按 size=(w,h) 缩放到统一坐标系"""
    k = (path, size)
    if k not in _cache:
        im = Image.open(path).convert('RGB')
        if size and im.size != tuple(size): im = im.resize(tuple(size), Image.LANCZOS)
        _cache[k] = np.asarray(im, float)
    return _cache[k]

def region(img, box, flt=None):
    """box = [x0,y0,x1,y1](像素); flt = {"hue":[lo,hi], "smin":, "smax":, "vmin":, "vmax":} 过滤后取中位数。返回 (rgb255 中位数, 像素数)"""
    x0, y0, x1, y1 = [int(round(v)) for v in box]
    p = img[y0:y1, x0:x1].reshape(-1, 3)
    if flt:
        h, s, v = hsv(p); k = np.ones(len(p), bool)
        if 'hue' in flt:
            lo, hi = flt['hue']; k &= ((h >= lo) & (h <= hi)) if lo <= hi else ((h >= lo) | (h <= hi))
        if 'smin' in flt: k &= s >= flt['smin']
        if 'smax' in flt: k &= s <= flt['smax']
        if 'vmin' in flt: k &= v >= flt['vmin']
        if 'vmax' in flt: k &= v <= flt['vmax']
        p = p[k]
    if len(p) < 4: return None, len(p)
    # 截掉两端 15% 亮度(高光/缝)后取均值, 比中位数稳
    L = p @ np.array([0.2126, 0.7152, 0.0722]); lo, hi = np.percentile(L, [15, 85])
    q = p[(L >= lo) & (L <= hi)]
    if len(q) < 4: q = p
    return q.mean(0), len(p)
