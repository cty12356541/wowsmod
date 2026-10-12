"""查找能显示中文的字体(运行时用系统字体, 仓库不附带字体文件)。

顺序: 环境变量 WOWSMOD_CJK_FONT → Windows(微软雅黑/黑体/宋体)→ macOS(苹方/冬青/华文)→ Linux(Noto CJK / 文泉驿 / Droid)→ fc-list 查询。
都找不到时返回 Pillow 默认字体并在 stderr 警告(中文会显示成方块): 安装 fonts-noto-cjk 或设 WOWSMOD_CJK_FONT。
"""
import os, shutil, subprocess, sys
from PIL import ImageFont

CANDIDATES = [
    "C:/Windows/Fonts/msyh.ttc", "C:/Windows/Fonts/msyhbd.ttc", "C:/Windows/Fonts/simhei.ttf", "C:/Windows/Fonts/simsun.ttc",
    "/System/Library/Fonts/PingFang.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc", "/Library/Fonts/Arial Unicode.ttf",
    "/System/Library/Fonts/STHeiti Medium.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc", "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
]

_cache = {}

def font_path():
    if "p" in _cache:
        return _cache["p"]
    env = os.environ.get("WOWSMOD_CJK_FONT")
    cands = ([env] if env else []) + CANDIDATES
    found = next((c for c in cands if c and os.path.exists(c)), None)
    if not found and shutil.which("fc-list"):
        try:
            out = subprocess.run(["fc-list", ":lang=zh", "file"], capture_output=True, text=True, timeout=10).stdout
            found = next((l.split(":")[0].strip() for l in out.splitlines() if l.strip()), None)
        except Exception:
            found = None
    if not found:
        print("警告: 没找到中文字体, 标签会显示成方块。设环境变量 WOWSMOD_CJK_FONT=<字体文件> 或安装 Noto Sans CJK。", file=sys.stderr)
    _cache["p"] = found
    return found

def font(size):
    p = font_path()
    if p:
        try:
            return ImageFont.truetype(p, size)
        except Exception:
            pass
    return ImageFont.load_default()

if __name__ == "__main__":
    print(font_path() or "(none)")
