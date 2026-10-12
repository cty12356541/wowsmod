"""Windows 上跑 wowsmod 的小包装: 设好环境变量, 按 UTF-8 收 --json 输出(PowerShell 管道会按 OEM 代码页弄坏中文), 存文件并打印摘要。
python wm.py <out.json|-> <wowsmod 参数...>
"""
import json, os, subprocess, sys
EXE = r"<WORK_DIR>\wowsmod\target\release\wowsmod.exe"
env = dict(os.environ, WOWSMOD_HOME=r"<WORK_DIR>\wowsmod\out\m4\home", WOWSMOD_BOOK_IGNORE="1", PYTHONIOENCODING="utf-8")
out = sys.argv[1]
p = subprocess.run([EXE, "--json"] + sys.argv[2:], capture_output=True, env=env)  # 用调用者的当前目录, 相对路径才对得上
txt = p.stdout.decode("utf-8", "replace")
if out != "-":
    open(out, "w", encoding="utf-8").write(txt)
sys.stdout.reconfigure(encoding="utf-8")
try:
    r = json.loads(txt)
    keep = {k: v for k, v in r.items() if k in ("ok", "error", "material_mode", "textures_written", "texture_reuses", "placement", "out", "outs", "next", "glb", "warnings", "files", "rev", "installed")}
    if isinstance(keep.get("warnings"), list):
        keep["warnings"] = keep["warnings"][:15]
    print(json.dumps(keep, ensure_ascii=False)[:4000])
except Exception:
    print(txt[:3000])
    print(p.stderr.decode("utf-8", "replace")[:2000])
sys.exit(p.returncode)
