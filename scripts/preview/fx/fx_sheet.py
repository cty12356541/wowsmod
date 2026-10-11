# 原版 vs 知更鸟 特效对照表: python3 sheet.py <out.jpg> [shots...]
import sys
from PIL import Image, ImageDraw, ImageFont
out = sys.argv[1]; shots = sys.argv[2:] or ['sail_day', 'sailside_day', 'fire_day', 'firesmoke_day', 'sail_night', 'sailside_night', 'fire_night', 'firesmoke_night']
W, H = 800, 450
try: font = ImageFont.truetype('/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc', 22)
except Exception: font = ImageFont.load_default()
o = Image.new('RGB', (2 * W, len(shots) * H + 40), (24, 24, 28)); d = ImageDraw.Draw(o)
d.text((10, 8), '原版 vanilla', fill=(230, 230, 230), font=font); d.text((W + 10, 8), '知更鸟 robin-qingge', fill=(200, 205, 255), font=font)
for i, s in enumerate(shots):
    for j, k in enumerate(['vanilla', 'robin']):
        o.paste(Image.open(f'{k}/{s}.png').convert('RGB').resize((W, H)), (j * W, 40 + i * H))
    d.text((10, 40 + i * H + 8), s, fill=(255, 230, 120), font=font)
o.save(out, quality=88); print(out, o.size)
