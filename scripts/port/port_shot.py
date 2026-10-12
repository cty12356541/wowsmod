#!/usr/bin/env python3
"""港口实拍小工具(Windows): 启动/前置游戏窗口、截图、鼠标轨道/缩放/点击、关闭。只做港口, 不进战斗/训练房。

用法(每步一条命令, 便于 AI 看截图后决定下一步):
  python port_shot.py launch --exe <GAME_DIR>\\Korabli.exe [--via <LESTA_DIR>\\GameCenter\\lgc.exe] [--wait 240]
  python port_shot.py wait  [--proc Korabli64] [--wait 240]          # 等游戏窗口出现
  python port_shot.py shot  out.png [--thumb out.jpg] [--clean]     # 前置游戏窗口并整屏截图; --clean = 按住右键隐藏港口界面再截
  python port_shot.py orbit DX DY [--shot out.png]                  # 右键拖动转镜头(像素); --shot 在松开前截(无界面)
  python port_shot.py port <港口名|行,列> [--thumb sel.jpg]          # 固化步骤: 头像下拉 → 选择港口 → 点格子 → 保存修改(见 PORTS)
  python port_shot.py ports                                          # 列出已知港口名与格子位置
  python port_shot.py zoom N                                         # 滚轮: N>0 拉近, N<0 拉远
  python port_shot.py click X Y [--right]                            # 屏幕坐标点击(先看截图定坐标)
  python port_shot.py key ESC|ENTER|<字母>                          # 发一个按键
  python port_shot.py close [--proc Korabli64]                       # 结束游戏进程(务必在最后执行)
  python port_shot.py sequence spec.json                             # 按 JSON 步骤列表批量执行(同上动作名)
进程名默认 Korabli64 / WorldOfWarships64(先找到哪个用哪个)。
港口里按住右键时界面会隐藏(游戏自带), --clean / orbit --shot 利用这一点。
坐标按 16:9、界面缩放 100% 以屏幕比例换算; 别的缩放/客户端版本先 `shot --thumb` 看图核对 UI 坐标表。
"""
import argparse, ctypes, json, subprocess, sys, time
from pathlib import Path

PROCS = ["Korabli64", "WorldOfWarships64", "Korabli", "WorldOfWarships"]

def _u32():
    return ctypes.windll.user32

def find_pid(names):
    for n in names:
        out = subprocess.run(f'tasklist /FI "IMAGENAME eq {n}.exe" /FO CSV /NH', shell=True, capture_output=True, text=True, encoding="utf-8", errors="replace").stdout
        for line in out.splitlines():
            parts = [p.strip('"') for p in line.split(",")]
            if len(parts) > 1 and parts[0].lower() == f"{n}.exe".lower():
                return int(parts[1]), n
    return None, None

def find_hwnd(pid):
    from ctypes import wintypes
    u = _u32()
    found = []
    proc = ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
    def cb(h, l):
        if not u.IsWindowVisible(h):
            return True
        p = wintypes.DWORD()
        u.GetWindowThreadProcessId(h, ctypes.byref(p))
        if p.value == pid:
            found.append(h)
            return False
        return True
    u.EnumWindows(proc(cb), 0)
    return found[0] if found else None

def front(names):
    pid, n = find_pid(names)
    if not pid:
        raise SystemExit(json.dumps({"ok": False, "error": "游戏进程不在", "tried": names}, ensure_ascii=False))
    h = find_hwnd(pid)
    if h:
        u = _u32()
        u.ShowWindow(h, 9)
        u.SetForegroundWindow(h)
        time.sleep(0.5)
    return pid, n, h

def screen():
    u = _u32()
    return u.GetSystemMetrics(0), u.GetSystemMetrics(1)

def move_abs(x, y):
    sw, sh = screen()
    _u32().mouse_event(0x0001 | 0x8000, int(x * 65535 / (sw - 1)), int(y * 65535 / (sh - 1)), 0, 0)

def orbit(dx, dy, steps=40, out=None, thumb=None):
    sw, sh = screen(); cx, cy = sw // 2, sh // 2
    move_abs(cx, cy); time.sleep(0.05)
    _u32().mouse_event(0x0008, 0, 0, 0, 0)
    r = None
    try:
        for i in range(steps):
            move_abs(cx + dx * (i + 1) / steps, cy + dy * (i + 1) / steps); time.sleep(0.02)
        if out:
            time.sleep(0.8); r = shot(out, thumb); r["clean"] = True
    finally:
        _u32().mouse_event(0x0010, 0, 0, 0, 0); time.sleep(0.4)
    return r

# 港口 UI 坐标(屏幕比例, Lesta 客户端 26.10 / 16:9 / 界面 100% 实测)
UI = {
    "profile_dropdown": (0.9551, 0.0257),   # 右上头像右侧的下拉箭头
    "menu_select_port": (0.8848, 0.2292),   # 下拉菜单「选择港口」
    "save": (0.4658, 0.9444),               # 港口选择页「保存修改」
    "close": (0.5527, 0.9444),              # 「关闭」(有改动时在右侧; 无改动时只剩居中的关闭)
    "close_only": (0.4990, 0.9444),
}
TILE_X = [0.1953, 0.3477, 0.5000, 0.6523, 0.8047]
TILE_Y = [0.2205, 0.3594, 0.5000]
# 港口选择页 3×5 格(行,列), 名字随客户端语言/版本可能变, 以截图为准
PORTS = {
    "吴市": (0, 0), "喀琅施塔得": (0, 1), "圣彼得堡": (0, 2), "夏威夷": (0, 3), "峡湾": (0, 4),
    "新罗西斯克": (1, 0), "日本": (1, 1), "普里玛星": (1, 2), "海军基地": (1, 3), "海洋": (1, 4),
    "猎岛基地": (2, 0), "符拉迪沃斯托克": (2, 1), "菲律宾": (2, 2), "黑桃城": (2, 3), "龙": (2, 4),
}
NIGHT_PORTS = ["黑桃城"]  # 夜景(偏黑白调色)

def click_frac(fx, fy, right=False):
    sw, sh = screen(); click(int(fx * sw), int(fy * sh), right)

def port_select(name, thumb=None):
    if "," in name:
        r, c = (int(x) for x in name.split(","))
    elif name in PORTS:
        r, c = PORTS[name]
    else:
        return {"ok": False, "error": f"未知港口 {name}", "known": list(PORTS)}
    click_frac(*UI["profile_dropdown"]); time.sleep(1.5)
    click_frac(*UI["menu_select_port"]); time.sleep(3)
    click_frac(TILE_X[c], TILE_Y[r]); time.sleep(1.5)
    res = {"ok": True, "port": name, "tile": [r, c]}
    if thumb:
        res["selector_thumb"] = shot(thumb.replace(".jpg", "_sel.png"), thumb)["thumb"]
    before = _small()
    click_frac(*UI["save"]); time.sleep(2.5)
    diff = _diff(before, _small())
    res["screen_change"] = round(diff, 1)
    if diff < 6:
        # 画面几乎没变 = 没有「保存修改」(选的就是当前港口): 点居中的「关闭」退出选择页
        click_frac(*UI["close_only"]); time.sleep(2)
        res["note"] = "选的就是当前港口, 已关闭选择页"
    else:
        time.sleep(18)
        res["note"] = "已保存并等待加载; 用 shot --thumb 确认"
    sw, sh = screen(); move_abs(sw // 2, int(sh * 0.62))  # 鼠标移回画面中央, 免得停在船列表上弹提示
    return res

def _small():
    from PIL import ImageGrab
    return ImageGrab.grab().convert("L").resize((160, 90))

def _diff(a, b):
    pa, pb = a.tobytes(), b.tobytes()
    return sum(abs(x - y) for x, y in zip(pa, pb)) / len(pa)

def shot_clean(out, thumb=None, hold=1.2):
    sw, sh = screen(); move_abs(sw // 2, int(sh * 0.62)); time.sleep(0.05)
    _u32().mouse_event(0x0008, 0, 0, 0, 0); time.sleep(hold)
    try:
        r = shot(out, thumb)
    finally:
        _u32().mouse_event(0x0010, 0, 0, 0, 0); time.sleep(0.4)
    r["clean"] = True
    return r

def zoom(n):
    sw, sh = screen(); move_abs(sw // 2, sh // 2)
    d = 120 if n > 0 else -120
    for _ in range(abs(n)):
        _u32().mouse_event(0x0800, 0, 0, d, 0); time.sleep(0.05)
    time.sleep(0.4)

def click(x, y, right=False):
    move_abs(x, y); time.sleep(0.1)
    down, up = (0x0008, 0x0010) if right else (0x0002, 0x0004)
    _u32().mouse_event(down, 0, 0, 0, 0); time.sleep(0.05); _u32().mouse_event(up, 0, 0, 0, 0); time.sleep(0.6)

VK = {"ESC": 0x1B, "ENTER": 0x0D, "SPACE": 0x20, "TAB": 0x09}
def key(k):
    vk = VK.get(k.upper(), ord(k.upper()[0]))
    _u32().keybd_event(vk, 0, 0, 0); time.sleep(0.05); _u32().keybd_event(vk, 0, 2, 0); time.sleep(0.5)

def shot(out, thumb=None):
    from PIL import ImageGrab
    im = ImageGrab.grab()
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    im.save(out)
    r = {"ok": True, "out": str(out), "size": im.size}
    if thumb:
        t = im.convert("RGB"); t.thumbnail((1280, 720)); t.save(thumb, quality=85); r["thumb"] = str(thumb)
    return r

def wait(names, secs):
    t0 = time.time()
    while time.time() - t0 < secs:
        pid, n = find_pid(names)
        if pid and find_hwnd(pid):
            return {"ok": True, "pid": pid, "proc": n, "waited_s": round(time.time() - t0)}
        time.sleep(3)
    return {"ok": False, "error": f"{secs}s 内没等到游戏窗口"}

def close(names):
    pid, n = find_pid(names)
    if not pid:
        return {"ok": True, "note": "游戏未运行"}
    subprocess.run(f"taskkill /PID {pid} /F", shell=True, capture_output=True)
    time.sleep(2)
    return {"ok": find_pid(names)[0] is None, "killed": n}

def do(action, a, names):
    if action == "launch":
        exe = a.get("via") or a["exe"]
        subprocess.Popen([exe] if not a.get("via") else [exe], close_fds=True)
        return wait(names, int(a.get("wait", 240))) if not a.get("via") else {"ok": True, "note": "已启动启动器; 在启动器里点开始游戏后用 wait"}
    if action == "wait":
        return wait(names, int(a.get("wait", 240)))
    front(names)
    if action == "shot":
        return shot_clean(a["out"], a.get("thumb")) if a.get("clean") else shot(a["out"], a.get("thumb"))
    if action == "orbit":
        r = orbit(float(a["dx"]), float(a["dy"]), out=a.get("shot"), thumb=a.get("thumb")); return r or {"ok": True}
    if action == "port":
        return port_select(a["name"], a.get("thumb"))
    if action == "zoom":
        zoom(int(a["n"])); return {"ok": True}
    if action == "click":
        click(int(a["x"]), int(a["y"]), bool(a.get("right"))); return {"ok": True}
    if action == "key":
        key(a["key"]); return {"ok": True}
    if action == "sleep":
        time.sleep(float(a["s"])); return {"ok": True}
    raise SystemExit(f"未知动作 {action}")

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("action")
    ap.add_argument("args", nargs="*")
    ap.add_argument("--exe"); ap.add_argument("--via"); ap.add_argument("--wait", type=int, default=240)
    ap.add_argument("--proc", action="append"); ap.add_argument("--thumb"); ap.add_argument("--right", action="store_true")
    ap.add_argument("--clean", action="store_true"); ap.add_argument("--shot")
    o = ap.parse_args()
    names = o.proc or PROCS
    act = o.action
    if act == "close":
        r = close(names)
    elif act == "ports":
        r = {"ok": True, "ports": {k: list(v) for k, v in PORTS.items()}, "night": NIGHT_PORTS, "ui": UI}
    elif act == "sequence":
        steps = json.load(open(o.args[0], encoding="utf-8"))
        r = {"ok": True, "steps": []}
        for s in steps:
            s = dict(s); a = s.pop("do")
            if a == "close":
                rr = close(names)
            else:
                rr = do(a, s, names)
            r["steps"].append({"do": a, **rr})
    else:
        a = {"exe": o.exe, "via": o.via, "wait": o.wait, "thumb": o.thumb, "right": o.right, "clean": o.clean, "shot": o.shot}
        if act == "port": a["name"] = o.args[0]
        if act == "shot": a["out"] = o.args[0]
        if act == "orbit": a["dx"], a["dy"] = o.args[0], o.args[1]
        if act == "zoom": a["n"] = o.args[0]
        if act == "click": a["x"], a["y"] = o.args[0], o.args[1]
        if act == "key": a["key"] = o.args[0]
        if act == "sleep": a["s"] = o.args[0]
        r = do(act, a, names)
    print(json.dumps(r, ensure_ascii=False))

if __name__ == "__main__":
    main()
