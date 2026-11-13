"""cdp.py — 只用 Python 标准库，按 API.md §11.5 走一遍 `wowsmod render` 的 CDP 流程（Rust 实现的参考与验收工具）。

  py tools/cdp.py render  OUT.png "view=fq&compare=split&base=base" [OUT2.png "query2" ...]
  py tools/cdp.py bench   [--seconds 8] [--sizes 1600x900,2560x1440] [--quality high,mid,low] [--scenes review,sim]
  py tools/cdp.py find    （只打印找到的浏览器）

选项：--url http://127.0.0.1:8960/  --browser <exe>  --headed（bench 用有窗口的浏览器）  --swiftshader
流程：找 Edge/Chrome → --headless=new --remote-debugging-port=0 --user-data-dir=<临时> → 读 DevToolsActivePort → WebSocket
→ Target.createTarget/attach → Emulation.setDeviceMetricsOverride → navigate ?render=wait → 等 WM_RENDER_READY
→ Runtime.evaluate(__render(p), awaitPromise) → Page.captureScreenshot(clip) → 写 PNG → 打印绝对路径。"""
import base64, json, os, shutil, socket, struct, subprocess, sys, tempfile, time
from urllib.parse import urlparse, parse_qsl

# ---------- 找浏览器 ----------
def find_browser(explicit=None):
    if explicit: return explicit if os.path.isfile(explicit) else None
    env = os.environ.get('WOWSMOD_BROWSER')
    if env and os.path.isfile(env): return env
    if os.name == 'nt':
        import winreg
        for exe in ('msedge.exe', 'chrome.exe'):
            for hive in (winreg.HKEY_LOCAL_MACHINE, winreg.HKEY_CURRENT_USER):
                try:
                    with winreg.OpenKey(hive, r'SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\\' + exe) as k:
                        p = winreg.QueryValue(k, None)
                        if p and os.path.isfile(p.strip('"')): return p.strip('"')
                except OSError: pass
        cands = [os.path.expandvars(p) for p in (r'%ProgramFiles(x86)%\Microsoft\Edge\Application\msedge.exe', r'%ProgramFiles%\Microsoft\Edge\Application\msedge.exe',
                 r'%ProgramFiles%\Google\Chrome\Application\chrome.exe', r'%LocalAppData%\Google\Chrome\Application\chrome.exe')]
    else:
        cands = [shutil.which(x) or '' for x in ('microsoft-edge', 'google-chrome', 'chromium', 'chromium-browser')]
    return next((c for c in cands if c and os.path.isfile(c)), None)

# ---------- 最小 WebSocket 客户端（RFC 6455，只用文本帧） ----------
class WS:
    def __init__(self, url):
        u = urlparse(url); self.s = socket.create_connection((u.hostname, u.port), timeout=180)
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall((f'GET {u.path} HTTP/1.1\r\nHost: {u.hostname}:{u.port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n'
                        f'Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n').encode())
        hdr = b''
        while b'\r\n\r\n' not in hdr: hdr += self.s.recv(1)
        if b' 101 ' not in hdr.split(b'\r\n')[0]: raise RuntimeError('websocket handshake failed: ' + hdr.decode(errors='replace'))
    def send(self, text):
        data = text.encode(); n = len(data); mask = os.urandom(4)
        head = bytes([0x81]) + (bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack('>H', n) if n < 65536 else bytes([0x80 | 127]) + struct.pack('>Q', n))
        self.s.sendall(head + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))
    def _exact(self, n):
        buf = bytearray()
        while len(buf) < n:
            c = self.s.recv(min(1 << 20, n - len(buf)))
            if not c: raise ConnectionError('websocket closed')
            buf += c
        return bytes(buf)
    def recv(self):
        out = bytearray()
        while True:
            b0, b1 = self._exact(2); n = b1 & 0x7f
            if n == 126: n = struct.unpack('>H', self._exact(2))[0]
            elif n == 127: n = struct.unpack('>Q', self._exact(8))[0]
            payload = self._exact(n); op = b0 & 0x0f
            if op == 8: raise ConnectionError('websocket closed by browser')
            if op == 9: continue
            out += payload
            if b0 & 0x80: return out.decode()

class CDP:
    def __init__(self, ws_url):
        self.ws = WS(ws_url); self.id = 0; self.events = []; self.session = None
    def call(self, method, params=None, session=True, timeout=180):
        self.id += 1; mid = self.id; msg = {'id': mid, 'method': method, 'params': params or {}}
        if session and self.session: msg['sessionId'] = self.session
        self.ws.send(json.dumps(msg)); t0 = time.time()
        while True:
            m = json.loads(self.ws.recv())
            if m.get('id') == mid:
                if 'error' in m: raise RuntimeError(f'{method}: {m["error"]}')
                return m.get('result', {})
            self.events.append(m)
            if time.time() - t0 > timeout: raise TimeoutError(method)
    def wait_console(self, marker, timeout=60):
        t0 = time.time()
        while time.time() - t0 < timeout:
            for i, e in enumerate(self.events):
                if e.get('method') == 'Runtime.consoleAPICalled':
                    txt = ' '.join(str(a.get('value', '')) for a in e['params'].get('args', []))
                    if txt.startswith(marker): del self.events[:i + 1]; return txt
            self.events.clear()
            self.events.append(json.loads(self.ws.recv()))
        raise TimeoutError('console marker ' + marker)

def launch(exe, headed=False, swiftshader=False):
    ud = tempfile.mkdtemp(prefix='wm_cdp_')
    args = [exe, '--remote-debugging-port=0', f'--user-data-dir={ud}', '--no-first-run', '--no-default-browser-check', '--disable-extensions',
            '--hide-scrollbars', '--mute-audio', '--force-device-scale-factor=1', '--disable-background-timer-throttling',
            '--disable-renderer-backgrounding', '--disable-backgrounding-occluded-windows']
    if not headed: args.append('--headless=new')
    if swiftshader: args += ['--use-angle=swiftshader', '--enable-unsafe-swiftshader']
    args.append('about:blank')
    proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    port_file = os.path.join(ud, 'DevToolsActivePort'); t0 = time.time()
    while not (os.path.isfile(port_file) and len(open(port_file).read().split('\n')) >= 2):
        if time.time() - t0 > 20 or proc.poll() is not None: raise RuntimeError('browser did not start')
        time.sleep(0.1)
    port, path = open(port_file).read().split('\n')[:2]
    return proc, ud, f'ws://127.0.0.1:{port}{path}'

def close(proc, ud):
    try: proc.terminate(); proc.wait(5)
    except Exception: proc.kill()
    for _ in range(20):
        try: shutil.rmtree(ud); break
        except Exception: time.sleep(0.25)

def open_page(c, url, w, h):
    tid = c.call('Target.createTarget', {'url': 'about:blank'}, session=False)['targetId']
    c.session = c.call('Target.attachToTarget', {'targetId': tid, 'flatten': True}, session=False)['sessionId']
    c.call('Runtime.enable'); c.call('Page.enable')
    c.call('Emulation.setDeviceMetricsOverride', {'width': w, 'height': h, 'deviceScaleFactor': 1, 'mobile': False})
    c.call('Page.navigate', {'url': url})

def ev(c, expr, timeout=180):
    r = c.call('Runtime.evaluate', {'expression': expr, 'awaitPromise': True, 'returnByValue': True}, timeout=timeout)
    if 'exceptionDetails' in r: raise RuntimeError(r['exceptionDetails'].get('exception', {}).get('description', 'js error'))
    return r['result'].get('value')

def parse_params(q):
    d = {}
    for k, v in parse_qsl(q, keep_blank_values=True): d[k] = v
    return d

def cmd_render(base, exe, pairs, swiftshader=False):
    proc, ud, ws = launch(exe, swiftshader=swiftshader)
    try:
        c = CDP(ws); open_page(c, base + '?render=wait', 1600, 900)
        c.wait_console('WM_RENDER_READY', 60)
        for out, q in pairs:
            t0 = time.time()
            res = ev(c, f'window.__render(window.__renderParse(new URLSearchParams({json.dumps(q)})))')
            if not res['ok']: print(json.dumps({'ok': False, 'out': None, 'error': res['error']})); continue
            w, h = res['width'], res['height']
            c.call('Emulation.setDeviceMetricsOverride', {'width': w, 'height': h, 'deviceScaleFactor': 1, 'mobile': False})
            shot = c.call('Page.captureScreenshot', {'format': 'png', 'clip': {'x': 0, 'y': 0, 'width': w, 'height': h, 'scale': 1}, 'captureBeyondViewport': True})
            out = os.path.abspath(out); open(out, 'wb').write(base64.b64decode(shot['data']))
            print(json.dumps({'ok': True, 'out': out, 'width': w, 'height': h, 'render_ms': res['ms'], 'total_s': round(time.time() - t0, 2),
                              'meta': {k: v for k, v in res['meta'].items() if k not in ('lod', 'sim', 'cells')}}, ensure_ascii=False))
    finally: close(proc, ud)

def cmd_bench(base, exe, sizes, qualities, scenes, seconds, headed, swiftshader=False):
    rows = []
    proc, ud, ws = launch(exe, headed=headed, swiftshader=swiftshader)
    try:
        c = CDP(ws)
        for (w, h) in sizes:
            for q in qualities:
                open_page(c, f'{base}?q={q}&lang=zh', w, h)
                t0 = time.time()
                while not ev(c, '!!window.__ready'):
                    if time.time() - t0 > 120: raise TimeoutError('app not ready')
                    time.sleep(0.25)
                for sc in scenes:
                    if sc == 'sim':
                        ev(c, "(() => { const a = WM.actions; a.setWorkspace('sim'); a.setEnv('water'); a.setTel('full'); a.setCombat(true); a.fire('main','salvo'); return 1; })()")
                        time.sleep(2)
                    elif sc == 'split':
                        ev(c, "(() => { WM.actions.setWorkspace('review'); WM.actions.setMode('split'); return 1; })()")
                    else:
                        ev(c, "(() => { WM.actions.setWorkspace('review'); WM.actions.setMode('single'); return 1; })()")
                    time.sleep(1)
                    r = ev(c, f'WM.bench({{seconds:{seconds}, frames:120}})', timeout=seconds + 120)
                    r.update(scene=sc, req=f'{w}x{h}', q=q); rows.append(r); print(json.dumps(r, ensure_ascii=False), flush=True)
                c.call('Target.closeTarget', {'targetId': c.call('Target.getTargetInfo', {})['targetInfo']['targetId']}, session=False); c.session = None
    finally: close(proc, ud)
    return rows

def main():
    a = sys.argv[1:]
    def opt(name, default=None):
        if name in a: i = a.index(name); v = a[i + 1]; del a[i:i + 2]; return v
        return default
    def flag(name):
        if name in a: a.remove(name); return True
        return False
    base = opt('--url', 'http://127.0.0.1:8960/'); exe = find_browser(opt('--browser'))
    seconds = float(opt('--seconds', '8')); sizes = [tuple(map(int, s.split('x'))) for s in opt('--sizes', '1600x900').split(',')]
    quals = opt('--quality', 'high,mid,low').split(','); scenes = opt('--scenes', 'review,split,sim').split(',')
    out_json = opt('--json'); headed = flag('--headed'); sw = flag('--swiftshader')
    if not a: print(__doc__); return 2
    if not exe:
        print(json.dumps({'ok': False, 'error': {'code': 'browser_not_found', 'message': '需要 Microsoft Edge 或 Google Chrome；可用 --browser 指定路径'}, 'url': base}, ensure_ascii=False)); return 3
    if a[0] == 'find': print(exe); return 0
    if a[0] == 'render': cmd_render(base, exe, list(zip(a[1::2], a[2::2])), sw); return 0
    if a[0] == 'bench':
        rows = cmd_bench(base, exe, sizes, quals, scenes, seconds, headed, sw)
        if out_json: open(out_json, 'w', encoding='utf-8').write(json.dumps(rows, ensure_ascii=False, indent=1))
        return 0
    print(__doc__); return 2

if __name__ == '__main__': sys.exit(main())
