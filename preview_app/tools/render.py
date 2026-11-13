"""无头出图驱动（box 上用 Playwright 模拟 Rust `wowsmod render` 的 CDP 流程）。
用法：python3 tools/render.py OUT.png 'view=fq&compare=split' [更多 'OUT2.png' 'query2' ...]
流程：打开 /?render=wait → 等 WM_RENDER_READY → 调 window.__render(params) → 等 __renderDone → 截 #out（与 Rust 端的
Page.captureScreenshot(clip=#out) 等价；也校验 dataURL 路径）。环境变量 URL、GPU=1。"""
import sys, json, os, time, base64
from urllib.parse import parse_qsl
from playwright.sync_api import sync_playwright
base = os.environ.get('URL', 'http://127.0.0.1:8960/')
pairs = list(zip(sys.argv[1::2], sys.argv[2::2]))
args = [] if os.environ.get('GPU') else ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist']
with sync_playwright() as p:
    b = p.chromium.launch(args=args)
    pg = b.new_page(viewport={'width': 800, 'height': 600}, device_scale_factor=1)
    logs = []
    pg.on('console', lambda m: logs.append(m.text))
    pg.on('pageerror', lambda e: logs.append(f'PAGEERROR: {e}'))
    pg.goto(base + '?render=wait')
    pg.wait_for_function('typeof window.__render === "function"', timeout=60000)
    for out, q in pairs:
        t0 = time.time()
        params = pg.evaluate('(q) => window.__renderParse(new URLSearchParams(q))', q)
        res = pg.evaluate('(p) => window.__render(p)', params)
        if not res['ok']:
            print('ERROR', out, res.get('error')); continue
        w, h = res['width'], res['height']
        pg.set_viewport_size({'width': w, 'height': h})
        pg.locator('#out').screenshot(path=out)
        meta = {k: v for k, v in res['meta'].items() if k not in ('lod', 'sim', 'cells')}
        print(f'saved {out} {w}x{h} render {res["ms"]} ms, total {time.time()-t0:.1f}s', json.dumps(meta, ensure_ascii=False))
    errs = [l for l in logs if 'PAGEERROR' in l or 'WM_RENDER_ERROR' in l]
    if errs: print('\n'.join(errs))
    b.close()
