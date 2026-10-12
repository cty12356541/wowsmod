"""外带道具共享图集(1024², 8 列×128px) + 发光遮罩。
列: 0 羽片 白→periwinkle 渐变(根→梢, 金镶边+内侧发光线) | 1 羽片 periwinkle→紫 渐变 | 2 白 | 3 periwinkle
    4 紫(violet) | 5 深蓝 navy | 6 金 | 7 发光(淡蓝紫, 遮罩满)
Blender 侧按材质名 rb_<列名> 把面 UV 压进对应列(见 robin_props_blender.py)。色值用 P2 色板 + 港口补偿(robin_p2_paint.to_tex)。
"""
import sys, os, json, numpy as np
from PIL import Image
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..', 'paint'))
import robin_p2_paint as R

def tex(name, hexv=None):
    h = hexv or R.PAL[name]
    return R.to_tex(R.hx(h)[None], R.COLMODE.get(name, 'half'))[0]

COLS = ['feather_wp', 'feather_pv', 'white', 'peri', 'violet', 'navy', 'gold', 'glow']
def build(out_dir, size=1024):
    os.makedirs(out_dir, exist_ok=True)
    cw = size // 8
    img = np.zeros((size, size, 3), float); mask = np.zeros((size, size), float)
    v = np.linspace(0, 1, size)[:, None]          # 图像行 0=顶 → glTF v=1(梢); 统一: 行 r 对应 v=1-r/size
    vv = 1 - v
    u = np.linspace(0, 1, cw)[None, :]
    W, P, V, G, N = tex('turret'), tex('peri'), tex('violet'), tex('gold'), tex('navy')
    GL = tex('glow', '#58B8FF')   # T3: 饱和天青(P3 #B4E2F8 近白, 港口发光读成白漆; 与 periwinkle 主色分开)
    def grad(a, b, t0, t1):
        t = np.clip((vv - t0) / (t1 - t0), 0, 1)[..., None]
        t = t * t * (3 - 2 * t)
        return a * (1 - t) + b * t
    # P5: 羽片更白更亮(少浅紫) + 翻转安全: 渐变改为关于 v=0.5 对称(中段纯白, 两端淡 periwinkle 染), 根/梢翻转外观一致
    #   列0 初级: 白, 两端 ≤22% periwinkle; 列1 次级: 白→periwinkle ≤45%(不再到紫)
    def sgrad(a, b, kmax):
        t = np.clip((np.abs(vv - 0.5) * 2 - 0.35) / 0.65, 0, 1)[..., None]; t = t * t * (3 - 2 * t)
        return a * (1 - kmax * t) + b * (kmax * t)
    for k, blkf in enumerate([lambda: sgrad(W, P, 0.12), lambda: sgrad(W, P, 0.28)]):  # P6: 羽更白(少 peri 染)
        blk = np.broadcast_to(blkf(), (size, 1, 3)).repeat(cw, 1).copy()
        # 羽轴(中线)浅一档 + 两侧细羽纹
        shaft = np.abs(u - 0.5) < 0.025
        blk[:, shaft[0]] = blk[:, shaft[0]] * 0.82 + W * 0.18
        # v2: 去高频细纹(港口 BC1+mip 下成条纹色块); 边缘只做柔和压暗, 金边改由几何前缘管承担
        edge = np.clip((np.abs(u - 0.5) - 0.36) / 0.14, 0, 1)
        blk[:] = blk * (1 - 0.18 * edge[..., None])
        img[:, k * cw:(k + 1) * cw] = blk
        gl = np.zeros((size, cw), bool)   # v2: 羽面不画发光细线(由几何发光槽承担)
        mask[:, k * cw:(k + 1) * cw] = np.where(gl & (vv > 0.15), 1.0, 0.0)
        img[:, k * cw:(k + 1) * cw][np.broadcast_to(gl & (vv > 0.15), (size, cw))] = GL
    for k, c in zip(range(2, 8), [W, P, V, N, G, GL]):
        img[:, k * cw:(k + 1) * cw] = c
    # T3b(翻转安全): 纯色=中带 v≈0.5; 面板细节=上带 v∈[0.62,0.98] 且下带为其上下镜像 → 引擎 v 翻转与否外观一致
    # 原 T3: 列 2–5 上半 = 面板细节区(pnl_<列>): 底部 AO、上沿倒角高光/下沿压暗、侧缘暗、两排铆钉、中缝、低频斑驳
    rows = np.arange(size); vrow = 1 - rows / size
    tt = np.clip((vrow - 0.62) / 0.36, 0, 1)[:, None]; uu = np.clip((u - 0.08) / 0.84, 0, 1)
    sm = lambda x: x * x * (3 - 2 * x)
    # P6: 面板 AO/倒角再加深(远距离分件可读): 底 AO 0.48, 顶高光 1.28, 侧缘 0.58, 中缝 0.42(更宽)
    f = 0.48 + 0.52 * sm(np.clip(tt / 0.42, 0, 1))
    f = f * np.where(tt > 0.90, 1.28, 1.0) * np.where(tt < 0.08, 0.48, 1.0) * np.where(np.abs(uu - 0.5) > 0.42, 0.58, 1.0)
    f = f * np.where(np.abs(tt - 0.5) < 6.0 / (0.36 * size), 0.42, 1.0)
    py = tt * 0.36 * size; px = uu * 0.84 * cw
    for tr in (0.13, 0.86):
        cy = tr * 0.36 * size
        for k_ in range(10):
            cx = (0.05 + 0.1 * k_) * 0.84 * cw
            d = np.sqrt((px - cx) ** 2 + (py - cy) ** 2); f = f * np.where(d < 2.6, 0.80, np.where(d < 3.6, 1.05, 1.0))
    rng = np.random.default_rng(7); mot = rng.normal(0, 1, (size // 32 + 1, cw // 32 + 1))
    mot = np.kron(mot, np.ones((32, 32)))[:size, :cw]
    from PIL import ImageFilter
    mot = np.asarray(Image.fromarray(((mot * 40) + 128).clip(0, 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(10)), float) / 40 - 3.2
    f = f * (1 + 0.025 * mot)
    top = vrow >= 0.61
    for k, c in zip(range(2, 6), [W, P, V, N]):
        img[top, k * cw:(k + 1) * cw] = (c[None, None, :] * f[top][..., None]).clip(0, 255)
        img[::-1][top, k * cw:(k + 1) * cw] = img[top, k * cw:(k + 1) * cw]   # 下带 = 上带上下镜像
    mask[:, 7 * cw:] = 1.0
    Image.fromarray(np.clip(img, 0, 255).astype(np.uint8)).save(f'{out_dir}/prop_atlas.png')
    Image.fromarray((mask * 255).astype(np.uint8)).save(f'{out_dir}/prop_glow.png')
    json.dump(dict(cols=COLS, cell=cw, size=size), open(f'{out_dir}/prop_atlas.json', 'w'))

if __name__ == '__main__':
    build(sys.argv[1] if len(sys.argv) > 1 else '.')
