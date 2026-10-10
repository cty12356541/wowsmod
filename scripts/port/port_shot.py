#!/usr/bin/env python3
"""港口实拍小工具(Windows): 启动/前置游戏窗口、截图、鼠标轨道/缩放/点击、关闭。只做港口, 不进战斗/训练房。

用法(每步一条命令, 便于 AI 看截图后决定下一步):
  python port_shot.py launch --exe D:\\Korabli\\Korabli.exe [--via D:\\Lesta\\GameCenter\\lgc.exe] [--wait 240]
  python port_shot.py wait  [--proc Korabli64] [--wait 240]          # 等游戏窗口出现
  python port_shot.py shot  out.png [--thumb out.jpg]               # 前置游戏窗口并整屏截图(可另存缩略图给 AI 看)
  python port_shot.py orbit DX DY                                   # 右键拖动转镜头(像素)
  python port_shot.py zoom N                                         # 滚轮: N>0 拉近, N<0 拉远
  python port_shot.py click X Y [--right]                            # 屏幕坐标点击(先看截图定坐标)
  python port_shot.py key ESC|ENTER|<字母>                          # 发一个按键
  python port_shot.py close [--proc Korabli64]                       # 结束游戏进程(务必在最后执行)
  python port_shot.py sequence spec.json                             # 按 JSON 步骤列表批量执行(同上动作名)
进程名默认 Korabli64 / WorldOfWarships64(先找到哪个用哪个)。
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

def orbit(dx, dy, steps=40):
    sw, sh = screen(); cx, cy = sw // 2, sh // 2
    move_abs(cx, cy); time.sleep(0.05)
    _u32().mouse_event(0x0008, 0, 0, 0, 0)
    for i in range(steps):
        move_abs(cx + dx * (i + 1) / steps, cy + dy * (i + 1) / steps); time.sleep(0.02)
    _u32().mouse_event(0x0010, 0, 0, 0, 0); time.sleep(0.4)

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
        return shot(a["out"], a.get("thumb"))
    if action == "orbit":
        orbit(float(a["dx"]), float(a["dy"])); return {"ok": True}
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
    o = ap.parse_args()
    names = o.proc or PROCS
    act = o.action
    if act == "close":
        r = close(names)
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
        a = {"exe": o.exe, "via": o.via, "wait": o.wait, "thumb": o.thumb, "right": o.right}
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
