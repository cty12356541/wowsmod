"""截图/自测脚本（仅开发用）：python3 tools/shot.py '<json steps>'
steps: ["js", code] | ["render", n, dt] | ["wait", s] | ["click", sel] | ["shot", path] | ["waitfn", js] | ["log"]"""
import sys, time, json, os
from playwright.sync_api import sync_playwright
url = os.environ.get('URL', 'http://127.0.0.1:8960/')
steps = json.loads(sys.argv[1]) if len(sys.argv) > 1 else [["shot", "./preview_app/shots/test.png"]]
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--enable-webgl'])
    pg = b.new_page(viewport={'width': 1600, 'height': 900}, device_scale_factor=1)
    logs = []
    pg.on('console', lambda m: logs.append(f'{m.type}: {m.text}'))
    pg.on('pageerror', lambda e: logs.append(f'PAGEERROR: {e}'))
    pg.goto(url)
    try:
        pg.wait_for_function('window.__ready === true', timeout=float(os.environ.get('READY_TIMEOUT', 240000)))
    except Exception as e:
        print('NOT READY', e)
    if os.environ.get('NOPAUSE') is None:
        pg.evaluate('window.WM && WM.V.pause(true)')
    time.sleep(0.3)
    for s in steps:
        if s[0] == 'js': r = pg.evaluate(s[1]); r is not None and print('js ->', json.dumps(r, ensure_ascii=False)[:2000])
        elif s[0] == 'render':
            for i in range(s[1]): ms = pg.evaluate('WM.V.renderOnce(%s)' % (s[2] if len(s) > 2 else 0.05))
            print('frame ms', round(ms, 1))
        elif s[0] == 'wait': time.sleep(s[1])
        elif s[0] == 'waitfn': pg.wait_for_function(s[1], timeout=120000)
        elif s[0] == 'click': pg.click(s[1])
        elif s[0] == 'shot': pg.screenshot(path=s[1], timeout=120000); print('saved', s[1])
    print('\n'.join(logs[-40:]))
    b.close()
