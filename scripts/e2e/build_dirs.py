#!/usr/bin/env python3
"""Build dir_a/b/c three-direction previews for Robin theme. Run on Windows from out/e2e/robin."""
import json, os, subprocess, sys, shutil
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageEnhance

ROOT = Path(r"<WORK_DIR>\wowsmod\out\e2e\robin")
EXE = Path(r"<WORK_DIR>\wowsmod\target\release\wowsmod.exe")
PORT = Path(r"<USERPROFILE>\e2e_port")
REFS = Path(r"<WORK_DIR>\refs\robin\refs")
GLB = PORT / "bourg.glb"
FIG = PORT / "figure_cutout.png"
STAR = PORT / "starfish.png"
NOTE = PORT / "note.png"
POSE = ROOT / "robin_pose.glb"
MIC = ROOT / "mic.glb"
SHOTS = ROOT / "shots"
BOOK = ROOT / "book"
env = dict(os.environ, WOWSMOD_HOME=r"<WORK_DIR>\wowsmod\out\m4\home", WOWSMOD_BOOK_IGNORE="1", PYTHONIOENCODING="utf-8")

# port-albedo results
WHITE = "#F4F4DA"       # screen white
SKY = "#DCD4CB"         # screen #9FD9ED (muted) — for large areas
SKY_BAND = "#71C1C9"    # screen #50C6EB
CREAM = "#F4E5A6"       # screen #F5EAC2
LAV = "#F49AB9"         # screen #D19ED9

HULL_A = "FSB025_Bourgogne_1945_Hull_a.dds"
HULL_MG = "FSB025_Bourgogne_1945_Hull_mg.dds"
DH_A = "FSB025_Bourgogne_1945_DeckHouse_a.dds"
DH_MG = "FSB025_Bourgogne_1945_DeckHouse_mg.dds"
HULL_MFM = "FSB025_Bourgogne_1945_Hull.mfm"
DH_MFM = "FSB025_Bourgogne_1945_DeckHouse.mfm"

def run(out_name, *args, check=True):
    outp = PORT / out_name
    cmd = [str(EXE), "--json", *args]
    print("+", " ".join(args)[:180], flush=True)
    p = subprocess.run(cmd, capture_output=True, env=env, cwd=str(ROOT))
    txt = p.stdout.decode("utf-8", "replace")
    outp.write_text(txt, encoding="utf-8")
    err = p.stderr.decode("utf-8", "replace")
    try:
        r = json.loads(txt) if txt.strip()[:1] == "{" else None
    except Exception:
        r = None
    if r is None:
        print("NONJSON", txt[:400], err[:400])
        if check: raise SystemExit(p.returncode or 1)
        return {"ok": False, "raw": txt, "stderr": err}
    if check and r.get("ok") is False:
        print("FAIL", json.dumps(r, ensure_ascii=False)[:800])
        raise SystemExit(1)
    keep = {k: r.get(k) for k in ("ok","error","out","outs","textures_written","texture_reuses","material_mode","warnings","placement","files","rev","next") if k in r}
    print(" ", json.dumps(keep, ensure_ascii=False)[:300], flush=True)
    return r

def ensure_dirs():
    SHOTS.mkdir(exist_ok=True)
    BOOK.mkdir(exist_ok=True)
    (BOOK / "overlays").mkdir(exist_ok=True)
    # copy assets into project-local
    for src, dst in [(FIG, BOOK/"overlays"/"figure_cutout.png"), (STAR, BOOK/"overlays"/"starfish.png"), (NOTE, BOOK/"overlays"/"note.png"), (PORT/"ASSETS.md", ROOT/"ASSETS.md"), (PORT/"soft_side.png", SHOTS/"soft_side.png")]:
        if src.exists():
            shutil.copy2(src, dst)

def make_overlays():
    soft = Image.open(SHOTS/"soft_side.png").convert("RGBA")
    W, H = soft.size  # 1600x341
    fig = Image.open(BOOK/"overlays"/"figure_cutout.png").convert("RGBA")
    star = Image.open(BOOK/"overlays"/"starfish.png").convert("RGBA")
    note = Image.open(BOOK/"overlays"/"note.png").convert("RGBA")

    # A: small starfish + notes along mid hull (restrained)
    a = Image.new("RGBA", (W, H), (0,0,0,0))
    # hull band roughly y 180..260 in soft image (eyeball from silhouette)
    for i, x in enumerate([280, 520, 780, 1040, 1280]):
        s = star.resize((48,48), Image.LANCZOS)
        a.alpha_composite(s, (x, 210))
        if i % 2 == 0:
            n = note.resize((28,36), Image.LANCZOS)
            a.alpha_composite(n, (x+55, 195))
    a.save(BOOK/"overlays"/"design_a_side.png")

    # B: midships figure panel + wave suggestion (theme)
    b = Image.new("RGBA", (W, H), (0,0,0,0))
    # place figure on midships hull (center-left of mid)
    f = fig.copy(); f.thumbnail((220, 220), Image.LANCZOS)
    # fade edges
    mask = Image.new("L", f.size, 0); d = ImageDraw.Draw(mask)
    d.ellipse((4,4,f.size[0]-4,f.size[1]-4), fill=255)
    mask = mask.filter(ImageFilter.GaussianBlur(6))
    f.putalpha(mask)
    bx, by = 620, 120
    b.alpha_composite(f, (bx, by))
    # lavender ribbon streak
    d2 = ImageDraw.Draw(b)
    for y in range(200, 250, 3):
        d2.line([(400, y), (1100, y-8)], fill=(209,158,217,90), width=2)
    for i, x in enumerate([350, 500, 900, 1100]):
        s = star.resize((36,36), Image.LANCZOS)
        b.alpha_composite(s, (x, 225))
    b.save(BOOK/"overlays"/"design_b_side.png")

    # C: larger figure + rainbow streaks + more motifs
    c = Image.new("RGBA", (W, H), (0,0,0,0))
    f = fig.copy(); f.thumbnail((320, 320), Image.LANCZOS)
    mask = Image.new("L", f.size, 0); d = ImageDraw.Draw(mask)
    d.rounded_rectangle((0,0,f.size[0],f.size[1]), radius=20, fill=255)
    mask = mask.filter(ImageFilter.GaussianBlur(4))
    f.putalpha(mask)
    c.alpha_composite(f, (520, 40))
    d2 = ImageDraw.Draw(c)
    cols = [(255,80,80,140),(255,180,60,140),(255,255,80,140),(80,220,120,140),(80,160,255,140),(180,100,255,140)]
    for i, col in enumerate(cols):
        y0 = 160 + i*8
        d2.line([(200, y0+40), (1400, y0-20)], fill=col, width=6)
    for x in range(250, 1450, 120):
        s = star.resize((40,40), Image.LANCZOS)
        c.alpha_composite(s, (x, 230))
    c.save(BOOK/"overlays"/"design_c_side.png")
    print("overlays made", W, H)

def paint_base(proj, band=True, band_color=SKY_BAND, hull_color=WHITE, dh_color=SKY):
    """Recolor hull/deckhouse + clear hull mg B. Optional mid-height wave band via --box."""
    # whole hull → white/cream
    run(f"{proj}_re_hull.json", "tex", "recolor", HULL_A, "--project", proj,
        "--to-color", hull_color, "--exclude-hue", "0:65", "--soft", "0.04", "--chroma-detail", "0.2")
    run(f"{proj}_re_dh.json", "tex", "recolor", DH_A, "--project", proj,
        "--to-color", dh_color, "--exclude-hue", "0:65", "--soft", "0.04", "--chroma-detail", "0.25")
    # clear camouflage B on hull so recolor shows (DeckHouse keep for glow later)
    run(f"{proj}_mg0.json", "tex", "channel", HULL_MG, "--project", proj, "--channel", "b", "--set", "0")
    if band and GLB.exists():
        # glTF y: waterline ~0, deck ~0.5-1.0; band y 0.05..0.55 full length, both sides
        run(f"{proj}_band.json", "tex", "recolor", HULL_A, "--project", proj,
            "--to-color", band_color, "--glb", str(GLB), "--material", "Hull",
            "--box=-2,0.05,-9,2,0.55,9", "--feather", "8", "--chroma-detail", "0.15",
            "--exclude-hue", "0:65")

def bake_and_composite(proj, design_png, tag):
    uv_out = BOOK / "overlays" / f"uv_{tag}.png"
    run(f"{proj}_bake.json", "bake-ref", proj,
        "--ref", str(design_png), "--material", "Hull", "--like", HULL_A,
        "--out", str(uv_out), "--view", "side", "--width", "1600", "--mirror-x")
    if uv_out.exists():
        run(f"{proj}_comp.json", "tex", "composite", HULL_A, "--project", proj,
            "--over", str(uv_out), "--keep-detail", "0.35", "--opacity", "0.92")

def add_glow(proj, b_max=40):
    # subtle hull glow on band region + deckhouse mild
    run(f"{proj}_glow_dh.json", "glow", "--project", proj, "--mg", DH_MG, "--diffuse", DH_A,
        "--color", "#9FD9ED", "--b-max", str(b_max), "--power", "0.35",
        "--mfm", DH_MFM, "--feather", "4")

def import_figure(proj, height, name="robin"):
    run(f"{proj}_imp.json", "model", "import", "--project", proj, "--mesh", str(POSE),
        "--like", "Stern_DeckHouse", "--section", "Stern",
        "--at", "0,0.44,-7.6", "--rot", "0,0,0", "--height", str(height),
        "--name", name, "--glow-mat", "天環", "--b-max", "60", "--power", "0.35", "--tex-size", "2048")

def animate_mic(proj):
    if not MIC.exists():
        print("no mic.glb, skip animate")
        return
    run(f"{proj}_anim.json", "model", "animate", "--project", proj, "--mesh", str(MIC),
        "--like", "Stern_DeckHouse", "--section", "Stern",
        "--preset", "orbit", "--at", "0,1.2,-7.6", "--radius", "0.9", "--orbit-rpm", "4",
        "--height", "0.35", "--name", "mic_orbit", "--glow-mat", ".*", "--b-max", "80", "--power", "0.4")

def render_set(proj, tag):
    run(f"{proj}_warm.json", "render", "--warm", proj, check=False)
    views = [
        ("day_fq", "view=custom&az=140&el=18&dist=22&w=1280&h=720&env=water&light=day"),
        ("day_side", "view=custom&az=90&el=8&dist=20&w=1280&h=720&env=water&light=day"),
        ("day_stern", "view=custom&az=200&el=12&dist=10&focus=hull%2FStern&w=1280&h=720&env=studio&light=day"),
        ("night_fq", "view=custom&az=140&el=18&dist=22&w=1280&h=720&env=water&light=night"),
        ("night_stern", "view=custom&az=200&el=12&dist=10&focus=hull%2FStern&w=1280&h=720&env=dark&light=night"),
    ]
    outs = []
    for name, q in views:
        op = SHOTS / f"{tag}_{name}.png"
        run(f"{proj}_{name}.json", "render", proj, "--query", q, "-o", str(op), check=False)
        outs.append(op)
    return outs

def main():
    os.chdir(ROOT)
    ensure_dirs()
    make_overlays()

    # --- A 克制: no figure, white+sky band, small motifs ---
    print("=== DIR A ===", flush=True)
    paint_base("dir_a", band=True, band_color=SKY_BAND, hull_color=WHITE, dh_color=SKY)
    bake_and_composite("dir_a", BOOK/"overlays"/"design_a_side.png", "a")
    # mild glow none / very subtle
    render_set("dir_a", "a")

    # --- B 主题强化: keep existing figure in dir_b, paint+art ---
    print("=== DIR B ===", flush=True)
    paint_base("dir_b", band=True, band_color=SKY_BAND, hull_color=WHITE, dh_color=SKY)
    bake_and_composite("dir_b", BOOK/"overlays"/"design_b_side.png", "b")
    add_glow("dir_b", b_max=48)
    # figure already imported; if missing, reimport
    # check via model list? just try render
    render_set("dir_b", "b")

    # --- C 夸张 ---
    print("=== DIR C ===", flush=True)
    paint_base("dir_c", band=True, band_color=SKY_BAND, hull_color=WHITE, dh_color=LAV)
    bake_and_composite("dir_c", BOOK/"overlays"/"design_c_side.png", "c")
    add_glow("dir_c", b_max=70)
    import_figure("dir_c", height=2.0, name="robin")
    animate_mic("dir_c")
    render_set("dir_c", "c")

    print("DONE build_dirs", flush=True)

if __name__ == "__main__":
    main()
