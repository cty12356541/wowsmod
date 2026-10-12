"""完整预览工作台(带左右面板的 UI)截图: 总览 / 分屏改前改后 / 时间线选中最新步 / 暗色影棚看发光。
用法: py -3 workbench_shot.py <preview URL> <输出目录> [WxH] [前缀]
只用标准库; 复用 preview_app/tools/cdp.py 的 CDP 客户端。"""
import json, os, sys, time, base64
sys.path.insert(0, os.environ.get('WM_CDP_TOOLS', r'<WORK_DIR>\preview_app\tools'))
import cdp as C

url, outdir = sys.argv[1], sys.argv[2]
W, H = map(int, (sys.argv[3] if len(sys.argv) > 3 else '1920x1080').split('x'))
pre = sys.argv[4] if len(sys.argv) > 4 else 'wb'
os.makedirs(outdir, exist_ok=True)
exe = C.find_browser()
proc, ud, ws = C.launch(exe)
log = []
try:
    c = C.CDP(ws)
    C.open_page(c, url, W, H)
    t0 = time.time(); ready = False
    while time.time() - t0 < 240:
        if C.ev(c, 'window.__ready === true', 10): ready = True; break
        time.sleep(0.5)
    log.append({'ready': ready, 'load_s': round(time.time() - t0, 1)})
    time.sleep(1.5)
    def shot(name):
        time.sleep(1.2)
        d = c.call('Page.captureScreenshot', {'format': 'png'})['data']
        p = os.path.join(outdir, f'{pre}_{name}.png'); open(p, 'wb').write(base64.b64decode(d)); print('saved', p); return p
    def state():
        return C.ev(c, """(()=>{const s=WM.store.getState();return {phase:s.phase,head:s.head,viewRev:s.viewRev,mode:s.mode,
          steps:s.steps.map(x=>({id:x.id,i:x.index,title:x.title,edits:x.edits.length,tex:x.textures.length,mats:x.materials.length})),
          project:s.project&&{name:s.project.project.name,dir:s.project.project.dir,ship:s.project.ship.code}, texProg:s.texProg}})()""")
    st = state(); log.append(st); print(json.dumps(st, ensure_ascii=False)[:3000])
    shot('1_overview')
    C.ev(c, "WM.actions.setMode('split')"); shot('2_split_base_vs_head')
    C.ev(c, "WM.actions.setMode('single')")
    C.ev(c, "(async()=>{const s=WM.store.getState(); await WM.actions.view(s.head); WM.store.setState({rightTab:'history'}); return true})()")
    shot('3_timeline_latest')
    C.ev(c, "WM.actions.setEnv('studio'); WM.actions.setLight('night'); WM.actions.setGlow(1.5)")
    shot('4_dark_glow')
    errs = [e for e in c.events if e.get('method') in ('Runtime.exceptionThrown',) or (e.get('method') == 'Runtime.consoleAPICalled' and e['params'].get('type') in ('error', 'warning'))]
    log.append({'console_errors': [json.dumps(e['params'], ensure_ascii=False)[:400] for e in errs[-20:]]})
finally:
    C.close(proc, ud)
open(os.path.join(outdir, f'{pre}_log.json'), 'w', encoding='utf-8').write(json.dumps(log, ensure_ascii=False, indent=1))
