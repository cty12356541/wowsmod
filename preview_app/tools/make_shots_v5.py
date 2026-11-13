"""M5 交付截图（React 版，1600×900）。需要 mock/server.py 在 8960 端口、web/dist 已构建。
python3 tools/make_shots_v5.py [场景名前缀 ...]"""
import sys, time, json, os
from playwright.sync_api import sync_playwright
B = os.environ.get('URL', 'http://127.0.0.1:8960/'); OUT = os.environ.get('OUT', './preview_app/shots/v5')
only = sys.argv[1:]
J = json.dumps

def wait_ready(pg):
    pg.wait_for_function('window.__ready === true', timeout=300000); time.sleep(0.6)
def settle(pg, n=1.2):
    time.sleep(n)
def still(pg): pg.evaluate('WM.still()'); time.sleep(0.3)

SCENES = []
def scene(name, q='lang=zh'):
    def deco(fn): SCENES.append((name, q, fn)); return fn
    return deco

@scene('01_real_textures_overview')
def s01(pg):
    pg.evaluate('WM.store.setState({leftTab:"textures", selTex:"FSB025_Bourgogne_1945_Hull_a", texCh:"rgb"})')
    pg.evaluate('WM.V.preset("hero", true)'); settle(pg, 4); still(pg)

@scene('02_split_before_after_deepsea')
def s02(pg):
    pg.evaluate('WM.actions.view("s1")'); time.sleep(1); wait_ready(pg)
    pg.evaluate('WM.actions.setMode("split")'); pg.evaluate('WM.V.setSplit(0.5)')
    pg.evaluate('WM.V.orbit(52, 11, undefined, undefined, true)')
    pg.evaluate('WM.store.setState({leftTab:"textures", selTex:"FSB025_Bourgogne_1945_Hull_a", texCh:"rgb"})'); settle(pg, 4); still(pg)

@scene('03_dark_studio_glow')
def s03(pg):
    pg.evaluate('WM.actions.setLight("night")'); pg.evaluate('WM.actions.setHighlight(false)')
    pg.evaluate('WM.V.preset("funnel", true)')
    pg.evaluate('WM.store.setState({leftTab:"textures", selTex:"FSB025_Bourgogne_1945_DeckHouse_mg", texCh:"b"})'); settle(pg, 4); still(pg)

@scene('04_timeline_review')
def s04(pg):
    pg.evaluate('WM.actions.view("s1")'); time.sleep(1); wait_ready(pg)
    pg.evaluate('WM.actions.setMode("side")'); pg.evaluate('WM.actions.setBase("prev")'); time.sleep(1); wait_ready(pg)
    pg.evaluate('WM.V.preset("hero", true)'); settle(pg, 4); still(pg)

@scene('05_russian_ui', 'lang=ru')
def s05(pg):
    pg.evaluate('WM.actions.setMode("split")'); pg.evaluate('WM.V.setSplit(0.42)')
    pg.evaluate('WM.actions.select("HP_FGM_2")'); pg.evaluate('WM.V.preset("port", true)'); settle(pg, 4); still(pg)

@scene('06_low_quality')
def s06(pg):
    pg.evaluate('WM.actions.setQuality("low")'); time.sleep(1.5); wait_ready(pg)
    pg.evaluate('WM.V.preset("hero", true)'); settle(pg, 4); still(pg)

def sim(pg, env='water', light='day'):
    pg.evaluate('WM.actions.setWorkspace("sim")'); pg.evaluate(f'WM.actions.setEnv({J(env)})'); pg.evaluate(f'WM.actions.setLight({J(light)})')

@scene('07_sim_sailing_wake')
def s07(pg):
    sim(pg); pg.evaluate('WM.V.orbit(148, 21, 26, [0,0.6,1.5], true)')
    pg.evaluate('WM.simPose({tel:"full", rudder:0.5, warm:28})'); time.sleep(1)

@scene('08_sim_salvo')
def s08(pg):
    sim(pg); pg.evaluate('WM.V.orbit(62, 9, 17, [0,1.0,-1.2], true)')
    pg.evaluate('WM.simPose({tel:"q2", turret:68, km:14, warm:20, fire:"main", fireT:0.07})'); time.sleep(1)

@scene('09_sim_night_glow_sailing')
def s09(pg):
    sim(pg, 'water', 'night'); pg.evaluate('WM.actions.setEmis(1, 0.8)')
    pg.evaluate('WM.V.orbit(118, 12, 15, [0,1.3,0.6], true)')
    pg.evaluate('WM.simPose({tel:"q3", rudder:-0.5, warm:22, t:0.2})'); time.sleep(1)

@scene('10_sim_far_lod_check')
def s10(pg):
    sim(pg, 'studio'); pg.evaluate('WM.actions.setLodTint(true)'); pg.evaluate('WM.actions.setLodMode("auto")')
    pg.evaluate('WM.V.orbit(60, 16, 46, [0,0.6,0], true)'); pg.evaluate('WM.store.setState({distPreset:"far"})')
    pg.evaluate('WM.simPose({tel:"q2", warm:10})'); time.sleep(1)

@scene('11_sim_game_camera')
def s11(pg):
    sim(pg); pg.evaluate('WM.actions.setCamMode("game")'); time.sleep(1.5)
    pg.evaluate('WM.simPose({tel:"full", turret:0, km:12, warm:24})'); time.sleep(1)

with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'])
    for name, q, fn in SCENES:
        if only and not any(name.startswith(o) for o in only): continue
        pg = b.new_page(viewport={'width': 1600, 'height': 900}, device_scale_factor=1)
        errs = []; pg.on('pageerror', lambda e: errs.append(str(e)))
        t0 = time.time(); pg.goto(B + '?' + q); wait_ready(pg)
        fn(pg)
        path = f'{OUT}/{name}.png'; pg.screenshot(path=path, timeout=120000)
        print(f'saved {path} ({time.time()-t0:.0f}s)', ('ERR ' + '; '.join(errs)) if errs else '')
        pg.close()
    b.close()
