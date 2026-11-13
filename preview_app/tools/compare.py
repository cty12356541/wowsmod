"""对比图：修改前 vs 修改后、你的界面 vs 参考工具。python3 tools/compare.py"""
from PIL import Image, ImageDraw, ImageFont
R = './preview_app'
F = '/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc'
try: font = ImageFont.truetype(F, 22); small = ImageFont.truetype(F, 17)
except Exception:
    import glob; F = (glob.glob('/usr/share/fonts/**/NotoSansCJK*.ttc', recursive=True) + glob.glob('/usr/share/fonts/**/NotoSerifCJK-Regular.ttc', recursive=True))[0]
    font = ImageFont.truetype(F, 22); small = ImageFont.truetype(F, 17)

def fit(im, w, h, bg=(246, 247, 249)):
    im = im.convert('RGB'); s = min(w / im.width, h / im.height); im2 = im.resize((max(1, int(im.width * s)), max(1, int(im.height * s))), Image.LANCZOS)
    c = Image.new('RGB', (w, h), bg); c.paste(im2, ((w - im2.width) // 2, (h - im2.height) // 2)); return c

def sheet(rows, out, title, colnames, cw=960, ch=540):
    pad, head, cap = 24, 64, 40
    W = pad * 3 + cw * 2; H = head + len(rows) * (ch + cap + pad) + pad
    S = Image.new('RGB', (W, H), (255, 255, 255)); d = ImageDraw.Draw(S)
    d.text((pad, 18), title, font=font, fill=(29, 33, 37))
    for i, n in enumerate(colnames): d.text((pad + i * (cw + pad), head - 4), n, font=small, fill=(107, 114, 124))
    y = head + 24
    for a, b, caption in rows:
        for i, p in enumerate((a, b)):
            x = pad + i * (cw + pad); S.paste(fit(Image.open(p), cw, ch), (x, y)); d.rectangle([x, y, x + cw - 1, y + ch - 1], outline=(220, 223, 227))
        d.text((pad, y + ch + 8), caption, font=small, fill=(67, 73, 80)); y += ch + cap + pad
    S.save(out, optimize=True); print('saved', out, S.size)

ba = [
    (f'{R}/shots/r1/01_real_textures_overview.png', f'{R}/shots/v5/01_real_textures_overview.png', '01 真实贴图总览：浮层卡片 → 固定三栏（大纲/贴图 | 视口 | 历史/检查器），视口工具栏 32px 不再漂浮'),
    (f'{R}/shots/r1/02_split_before_after_deepsea.png', f'{R}/shots/v5/02_split_before_after_deepsea.png', '02 改前/改后分屏：深海蓝不再发棕（Khronos PBR Neutral 色调映射）；标签收进角落，回看提示移到底部'),
    (f'{R}/shots/r1/03_dark_studio_glow.png', f'{R}/shots/v5/03_dark_studio_glow.png', '03 暗色影棚发光：贴图通道对比（_mg 的 B 通道）与公式说明放在左栏，不遮挡视口'),
    (f'{R}/shots/r1/04_timeline_review.png', f'{R}/shots/v5/04_timeline_review.png', '04 时间线回看：步骤行 24–28px、命令默认折叠 4 条；并排对比两侧有版本标签'),
    (f'{R}/shots/r1/05_russian_ui.png', f'{R}/shots/v5/05_russian_ui.png', '05 俄文界面：长词不截断布局（属性列 88px + 自适应），检查器带 LOD 表与数据来源标签'),
    (f'{R}/shots/r1/06_low_quality.png', f'{R}/shots/v5/06_low_quality.png', '06 低画质档：状态栏分段控件切换，显示绘制次数 / 三角形 / 显存'),
]
sheet(ba, f'{R}/shots/compare/before_vs_after.png', 'wowsmod 预览 · 修改前（r1 原生 JS 版） vs 修改后（M5 React 版）', ['修改前 r1', '修改后 v5'])

ref = [
    (f'{R}/shots/v5/05_russian_ui.png', f'{R}/refs/blender_startup.png', 'Blender：左大纲 / 中视口 / 右属性；视口顶部一条细工具栏；状态栏显示场景统计。我们沿用同样的三栏骨架与 24px 行高'),
    (f'{R}/shots/v5/01_real_textures_overview.png', f'{R}/refs/figma_ui3_a.png', 'Figma UI3：白底面板、1px 细线分隔、11px 分组标题、分段控件代替多个按钮；单一蓝色强调色'),
    (f'{R}/shots/v5/07_sim_sailing_wake.png', f'{R}/refs/substance_texset.png', 'Substance：属性行 = 标签 + 控件 + 来源小标记；我们用「游戏数据 / 近似」小标签区分数据来源'),
    (f'{R}/shots/v5/04_timeline_review.png', f'{R}/refs/linear_a.png', 'Linear：列表行的序号 + 标题 + 灰色元信息；当前项左侧 2px 强调条；我们的时间线采用同样的层级'),
    (f'{R}/shots/v5/10_sim_far_lod_check.png', f'{R}/refs/vscode_ui.png', 'VS Code：22px 状态栏放连接状态 / 性能 / 画质；面板标签页 30px，下划线表示当前页'),
]
sheet(ref, f'{R}/shots/compare/ours_vs_reference.png', 'wowsmod 预览 vs 参考工具（Blender / Figma UI3 / Substance / Linear / VS Code）', ['我们的界面（M5）', '参考工具'])
