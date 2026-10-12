"""React 版截图/自测（仅开发用）：python3 tools/ui.py '<json steps>'
steps: ["js", code] | ["wait", s] | ["waitfn", js] | ["click", selector] | ["key", key] | ["shot", path] | ["hover", sel] | ["move", x, y]
环境变量：URL（默认 http://127.0.0.1:8960/）、W/H（默认 1600×900）、GPU=1（不用 swiftshader）"""
import sys, time, json, os
from playwright.sync_api import sync_playwright
url = os.environ.get('URL', 'http://127.0.0.1:8960/')
W, H = int(os.environ.get('W', 1600)), int(os.environ.get('H', 900))
steps = json.loads(sys.argv[1]) if len(sys.argv) > 1 else [["shot", "/tmp/test.png"]]
args = [] if os.environ.get('GPU') else ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist']
with sync_playwright() as p:
    b = p.chromium.launch(args=args)
    ctx = b.new_context(viewport={'width': W, 'height': H}, device_scale_factor=1, record_video_dir=os.environ.get('VIDEO') or None, record_video_size={'width': W, 'height': H} if os.environ.get('VIDEO') else None)
    pg = ctx.new_page(); logs = []
    pg.on('console', lambda m: logs.append(f'{m.type}: {m.text}'))
    pg.on('pageerror', lambda e: logs.append(f'PAGEERROR: {e}'))
    pg.goto(url)
    if not os.environ.get('NOWAIT'):
        try: pg.wait_for_function('window.__ready === true', timeout=240000)
        except Exception as e: print('NOT READY', e)
    time.sleep(0.5)
    for s in steps:
        k = s[0]
        if k == 'js':
            r = pg.evaluate(s[1])
            if r is not None: print('js ->', json.dumps(r, ensure_ascii=False)[:3000])
        elif k == 'wait': time.sleep(s[1])
        elif k == 'waitfn': pg.wait_for_function(s[1], timeout=240000)
        elif k == 'click': pg.click(s[1], timeout=10000)
        elif k == 'hover': pg.hover(s[1], timeout=10000)
        elif k == 'move': pg.mouse.move(s[1], s[2])
        elif k == 'key': pg.keyboard.press(s[1])
        elif k == 'shot': pg.screenshot(path=s[1], timeout=120000); print('saved', s[1])
    print('\n'.join(l for l in logs[-30:] if 'Download the React DevTools' not in l))
    ctx.close(); b.close()
