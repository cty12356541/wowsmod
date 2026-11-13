"""无头出图交付物 + 航行/齐射 GIF。python3 tools/make_render_v5.py [renders|gif]"""
import sys, json, os, time, io
from playwright.sync_api import sync_playwright
from PIL import Image
B = os.environ.get('URL', 'http://127.0.0.1:8960/'); OUT = './preview_app/shots/render'
what = sys.argv[1:] or ['renders', 'gif']
RENDERS = [
    ('r01_front_quarter_split_vs_base.png', 'view=fq&compare=split&base=base&lang=zh'),
    ('r02_side_by_side_step1_vs_step0.png', 'view=side&compare=side&rev=1&base=0&lang=zh'),
    ('r03_funnel_emissive_channel_dark.png', 'view=funnel&channel=emissive&env=dark&lang=zh'),
    ('r04_focus_HP_FGM_1_albedo.png', 'view=fq&focus=HP_FGM_1&channel=albedo&lang=zh'),
    ('r05_water_cut_dusk_port.png', 'view=port&env=water&water=cut&light=dusk&lang=zh'),
    ('r06_sim_salvo_frame.png', 'az=62&el=9&dist=17&env=water&speed=q2&turret=68&aimKm=14&fire=main&fireT=0.07&lang=zh'),
    ('r07_sim_night_glow_pulse_t0.25.png', 'az=118&el=12&dist=15&env=water&light=night&speed=q3&rudder=-0.5&emis=1&emisSpeed=0.8&t=0.25&lang=zh'),
    ('r08_far_lod_tint.png', 'az=60&el=16&dist=46&lod=auto&lodTint=1&lang=zh'),
    ('r09_wire_rear_quarter_en.png', 'view=rq&channel=wire&lang=en&w=1280&h=720'),
    ('r10_top_normal_ru.png', 'view=top&channel=normal&lang=ru&w=1280&h=720'),
    ('sheet_4views_base_vs_head.png', 'sheet=1&views=side,fq,rq,top&lang=zh'),
    ('sheet_timeline_3steps_fq_funnel.png', 'sheet=1&views=fq,funnel&revs=0,1,2&cellW=640&cellH=360&lang=zh'),
]
def open_render(p):
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'])
    pg = b.new_page(viewport={'width': 800, 'height': 600}); logs = []
    pg.on('console', lambda m: logs.append(m.text))
    pg.goto(B + '?render=wait'); pg.wait_for_function('typeof window.__render === "function"')
    return b, pg, logs
def render(pg, q, data=False):
    P = pg.evaluate('(q) => window.__renderParse(new URLSearchParams(q))', q)
    if data: P['data'] = True
    return pg.evaluate('(p) => window.__render(p)', P)

with sync_playwright() as p:
    b, pg, logs = open_render(p)
    if 'renders' in what:
        for name, q in RENDERS:
            r = render(pg, q)
            if not r['ok']: print('ERROR', name, r['error']); continue
            pg.set_viewport_size({'width': r['width'], 'height': r['height']})
            pg.locator('#out').screenshot(path=f'{OUT}/{name}')
            print(f"saved {OUT}/{name} {r['width']}x{r['height']} {r['ms']}ms")
        # 确定性校验：同一参数渲染两次，逐像素比较（dataURL 路径 = MCP 模式取图方式）
        import base64, hashlib
        h = []
        for _ in range(2):
            r = render(pg, 'view=fq&env=water&speed=full&turret=40&fire=main&fireT=0.05&t=1.5&w=640&h=360', data=True)
            h.append(hashlib.sha256(base64.b64decode(r['dataURL'].split(',', 1)[1])).hexdigest())
        print('determinism', 'OK' if h[0] == h[1] else 'DIFF', h[0][:16], h[1][:16])
        json.dump({'renders': RENDERS, 'determinism_sha256': h}, open(f'{OUT}/renders.json', 'w'), ensure_ascii=False, indent=1)
    if 'gif' in what:
        frames = []; N = 54; Tf = 9.0
        for i in range(N):
            T = 7.0 + i / 12
            q = f'az=58&el=10&dist=19&env=water&speed=full&rudder=0.5&turret=70&aimKm=14&w=800&h=450&wm=0&t={T:.4f}'
            if T >= Tf: q += f'&warm={Tf}&fire=main&fireT={T - Tf:.4f}'
            else: q += f'&warm={T:.4f}'
            r = render(pg, q, data=True)
            import base64
            frames.append(Image.open(io.BytesIO(base64.b64decode(r['dataURL'].split(',', 1)[1]))).convert('RGB'))
            print('frame', i, r['ms'], 'ms', flush=True)
        pal = [f.quantize(colors=200, method=Image.MEDIANCUT, dither=Image.FLOYDSTEINBERG) for f in frames]
        pal[0].save(f'./preview_app/shots/sim_salvo_sailing.gif', save_all=True, append_images=pal[1:], duration=83, loop=0, optimize=True)
        os.makedirs('/tmp/gifr', exist_ok=True)
        for i, f in enumerate(frames): f.save(f'/tmp/gifr/{i:03d}.png')
        print('saved gif')
    errs = [l for l in logs if 'ERROR' in l]
    if errs: print('\n'.join(errs[:5]))
    b.close()
