"""React 版截图/自测（仅开发用）：python3 tools/ui.py '<json steps>'
steps: ["js", code] | ["wait", s] | ["waitfn", js] | ["click", selector] | ["key", key] | ["shot", path] | ["hover", sel] | ["move", x, y]
环境变量：URL（默认 http://127.0.0.1:8960/）、W/H（默认 1600×900）、GPU=1（不用 swiftshader）、
          READY_TIMEOUT（秒，默认 300）、NOWAIT=1（不等 __ready）
等待就绪时每 10 s 打印一次加载阶段（phase / 加载文案 / 贴图进度），出现致命错误立即退出，
超时会打印最后状态与控制台日志，而不是静默等满再截一张“正在解析模型”的图。"""
import sys, time, json, os
from playwright.sync_api import sync_playwright
url = os.environ.get('URL', 'http://127.0.0.1:8960/')
W, H = int(os.environ.get('W', 1600)), int(os.environ.get('H', 900))
RT = float(os.environ.get('READY_TIMEOUT', 300))
steps = json.loads(sys.argv[1]) if len(sys.argv) > 1 else [["shot", "/tmp/test.png"]]
args = [] if os.environ.get('GPU') else ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist']
STATE = """() => { const l = document.querySelector('.loading'), f = document.querySelector('.fatal');
  return { ready: window.__ready === true, loading: l ? l.innerText.replace(/\\s+/g, ' ').slice(0, 120) : null,
           fatal: f ? f.innerText.replace(/\\s+/g, ' ').slice(0, 200) : null }; }"""

def wait_ready(pg, logs):
    t0 = time.time(); last = None
    while True:
        st = pg.evaluate(STATE)
        if st['ready']: print(f'READY {time.time() - t0:.1f}s'); return True
        if st['fatal']: print('FATAL', st['fatal']); return False
        if time.time() - t0 > RT:
            print(f'NOT READY after {RT:.0f}s, last state: {st}'); return False
        if st != last or int(time.time() - t0) % 10 == 0:
            print(f'  [{time.time() - t0:5.1f}s] {st["loading"] or "(no loading overlay)"}'); last = st
        time.sleep(1)

with sync_playwright() as p:
    b = p.chromium.launch(args=args)
    ctx = b.new_context(viewport={'width': W, 'height': H}, device_scale_factor=1, record_video_dir=os.environ.get('VIDEO') or None, record_video_size={'width': W, 'height': H} if os.environ.get('VIDEO') else None)
    pg = ctx.new_page(); logs = []
    pg.on('console', lambda m: logs.append(f'{m.type}: {m.text}'))
    pg.on('pageerror', lambda e: logs.append(f'PAGEERROR: {e}'))
    pg.on('requestfailed', lambda r: logs.append(f'REQFAIL: {r.url} {r.failure}'))
    pg.on('response', lambda r: logs.append(f'HTTP {r.status}: {r.url}') if r.status >= 400 else None)
    pg.goto(url)
    if not os.environ.get('NOWAIT') and not wait_ready(pg, logs):
        print('\n'.join(logs[-30:])); ctx.close(); b.close(); sys.exit(2)
    time.sleep(0.5)
    for s in steps:
        k = s[0]
        if k == 'js':
            r = pg.evaluate(s[1])
            if r is not None: print('js ->', json.dumps(r, ensure_ascii=False)[:3000])
        elif k == 'wait': time.sleep(s[1])
        elif k == 'waitfn': pg.wait_for_function(s[1], timeout=RT * 1000, polling=500)
        elif k == 'click': pg.click(s[1], timeout=10000)
        elif k == 'hover': pg.hover(s[1], timeout=10000)
        elif k == 'move': pg.mouse.move(s[1], s[2])
        elif k == 'key': pg.keyboard.press(s[1])
        elif k == 'shot': pg.screenshot(path=s[1], timeout=120000); print('saved', s[1])
    print('\n'.join(l for l in logs[-30:] if 'Download the React DevTools' not in l))
    ctx.close(); b.close()
